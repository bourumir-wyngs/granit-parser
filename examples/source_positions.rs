//! Convert parser locations into UTF-16 editor positions using the original source.
//!
//! Run with `cargo run --example source_positions`.
//!
//! There are three different units to keep apart:
//! - `Marker::index()` counts Unicode scalar values (`char`s) from zero.
//! - `Marker::byte_offset()` counts UTF-8 bytes from zero, when the input supports it.
//! - UTF-16 editor columns count UTF-16 code units from zero: `🦀` occupies two units.
//!
//! Parser lines start at one; parser columns start at zero and normally count `char`s,
//! not bytes, UTF-16 units, grapheme clusters, or screen cells. A document-prefix BOM
//! advances the parser's index but not its column. Deriving editor positions from the
//! original source also handles this distinction.
//!
//! This example assumes UTF-16 positions. LSP defaults to UTF-16, but a client and
//! server can negotiate another encoding; use the encoding selected for your session.
//! See the [LSP position contract](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position).

use granit_parser::{Event, Marker, Parser, ScanError};

/// A small editor-position type, independent of any LSP crate.
/// Both fields are zero-based. An actual protocol adapter should also check that
/// these `usize` values fit its wire-format integer bounds before sending them.
#[derive(Debug, PartialEq, Eq)]
struct Utf16Position {
    line: usize,
    character: usize,
}

/// Cache line starts once instead of scanning the entire file for every marker.
/// Keep this index and its markers tied to the same, unchanged source snapshot.
struct SourcePositions<'source> {
    source: &'source str,
    line_starts: Vec<usize>,
}

impl<'source> SourcePositions<'source> {
    fn new(source: &'source str) -> Self {
        let mut line_starts = vec![0];
        let bytes = source.as_bytes();
        let mut offset = 0;

        // YAML and LSP recognize LF, CRLF, and lone CR. `str::lines()` alone does
        // not provide the offsets and lone-CR handling needed here. CRLF advances
        // one line even though it occupies two bytes and two Rust `char`s.
        while offset < bytes.len() {
            match bytes[offset] {
                b'\r' => {
                    offset += 1;
                    if bytes.get(offset) == Some(&b'\n') {
                        offset += 1;
                    }
                    line_starts.push(offset);
                }
                b'\n' => {
                    offset += 1;
                    line_starts.push(offset);
                }
                // ASCII CR and LF cannot occur inside a multibyte UTF-8 character,
                // so scanning bytes is safe when only detecting line boundaries.
                _ => offset += 1,
            }
        }

        Self {
            source,
            line_starts,
        }
    }

    /// Return `None` for a missing byte offset or one outside valid UTF-8 boundaries.
    /// An in-bounds offset cannot prove source identity: callers must retain the
    /// exact source that produced the marker, without normalizing its line endings.
    fn utf16_position(&self, marker: Marker) -> Option<Utf16Position> {
        let offset = marker.byte_offset()?;
        self.source.get(..offset)?;

        // The first line always starts at zero, so at least one start is <= offset.
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let prefix = self.source.get(self.line_starts[line]..offset)?;

        // Editors have no position inside CRLF. An offset between CR and LF maps
        // to the preceding line's end; after the complete break it maps to column
        // zero of the next line. Line-break characters never contribute to columns.
        let prefix = prefix.trim_end_matches(['\r', '\n']);
        Some(Utf16Position {
            line,
            character: prefix.encode_utf16().count(),
        })
    }
}

fn main() -> Result<(), ScanError> {
    // `é` is one Rust char, two UTF-8 bytes, and one UTF-16 unit. `🦀` is one
    // Rust char, four UTF-8 bytes, and two UTF-16 units. Include every line ending.
    let yaml = "title: café\r\n🦀: value\nlast: fin\rtail: end\n";
    // Normalize line endings ONLY for display: a lone CR can overwrite terminal text.
    // Show the escaped original too, since its exact line endings matter for byte offsets.
    let display_yaml = yaml.replace("\r\n", "\n").replace('\r', "\n");
    println!("YAML:\n{display_yaml}");
    println!("Original source with escaped line endings: {yaml:?}\n");
    let positions = SourcePositions::new(yaml);

    for next in Parser::new_from_str(yaml) {
        let (event, span) = next?;
        let Event::Scalar(value, ..) = event else {
            continue;
        };

        // Spans are half-open: convert start and end independently, since an event
        // may span several lines. Never calculate the end from the decoded scalar's
        // length: quoting, escapes, and YAML folding can change that length.
        let start = positions
            .utf16_position(span.start)
            .expect("this string-backed parser supplies valid source byte offsets");
        let end = positions
            .utf16_position(span.end)
            .expect("this string-backed parser supplies valid source byte offsets");

        // `slice` and `byte_range` already use UTF-8 offsets. Do not index `yaml`
        // with `Marker::index()` or `Span::len()`, whose units are Rust chars.
        let source_text = span
            .slice(yaml)
            .expect("the span belongs to this unchanged string");
        println!("{value:?}: source={source_text:?}, editor={start:?}..{end:?}");

        if value == "value" {
            // Three Rust chars precede this value on its line: 🦀, ':', and ' '.
            // The crab contributes two UTF-16 units, so the editor column is four.
            assert_eq!((span.start.line(), span.start.col()), (2, 3));
            assert_eq!(
                start,
                Utf16Position {
                    line: 1,
                    character: 4
                }
            );
            assert_eq!(
                end,
                Utf16Position {
                    line: 1,
                    character: 9
                }
            );
            let prefix = "title: café\r\n🦀: ";
            assert_eq!(span.start.index(), prefix.chars().count());
            assert_eq!(span.start.byte_offset(), Some(prefix.len()));
        } else if value == "🦀" {
            assert_eq!(span.len(), 1);
            assert_eq!(
                span.byte_range().expect("string input has offsets").len(),
                4
            );
            assert_eq!(
                end,
                Utf16Position {
                    line: 1,
                    character: 2
                }
            );
        } else if value == "tail" {
            // The lone CR before this key starts a new line, just as LF and CRLF do.
            assert_eq!(span.start.line(), 4);
            assert_eq!(
                start,
                Utf16Position {
                    line: 3,
                    character: 0
                }
            );
        }
    }

    // A character iterator need not have stable backing text or meaningful byte
    // offsets. Missing offsets are not zero: return an unavailable position, or
    // let your application track source positions while decoding the input.
    for next in Parser::new_from_iter(yaml.chars()) {
        let (event, span) = next?;
        if let Event::Scalar(..) = event {
            assert_eq!(span.byte_range(), None);
            assert_eq!(positions.utf16_position(span.start), None);
        }
    }

    Ok(())
}
