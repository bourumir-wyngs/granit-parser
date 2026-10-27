//! Keep required byte ranges in a consumer of string-parser events.
//!
//! Run with `cargo run --example string_spans` (also works with `--no-default-features`).
//!
//! `Parser::new_from_str` supplies byte offsets for every event span, including empty
//! spans. The shared `Span` type uses `Option` because iterator inputs may lack offsets.
//! Convert that option once at the consumer boundary, then keep an ordinary range.
//! If only source text is needed, `Span::slice(source)` already provides checked slicing.

use granit_parser::{Event, Parser};
use std::{borrow::Cow, ops::Range};

/// Application data tied to the same source snapshot used by the parser.
#[derive(Debug)]
struct SourceScalar<'source> {
    value: Cow<'source, str>,
    bytes: Range<usize>,
    raw: &'source str,
}

fn collect_scalars(source: &str) -> Result<Vec<SourceScalar<'_>>, String> {
    let mut scalars = Vec::new();
    for next in Parser::new_from_str(source) {
        // Propagate parse failures instead of returning a partial collection as success.
        let (event, span) = next.map_err(|error| format!("YAML parse error: {error:?}"))?;

        // This is guaranteed for this parser constructor. Treat an absent range as an
        // error at the boundary, including if this code later switches input backends.
        // Zero and character indexes are not substitutes for missing byte offsets.
        let bytes = span
            .byte_range()
            .ok_or("this consumer requires source byte offsets")?;
        let raw = source
            .get(bytes.clone())
            .ok_or("event span is not a valid range in the original source")?;

        if let Event::Scalar(value, ..) = event {
            scalars.push(SourceScalar { value, bytes, raw });
        }
    }
    Ok(scalars)
}

fn main() -> Result<(), String> {
    // The Unicode key makes character indexes differ from byte offsets. The quoted
    // escape makes decoded text differ from its spelling. The last value is implicit.
    let source = "café: \"\\u263A\"\r\nempty:";
    println!("Original source: {source:?}");
    let scalars = collect_scalars(source)?;

    // Downstream code uses required byte ranges without unwrapping optional offsets.
    // Keep the original source unchanged; normalizing CRLF would invalidate later ranges.
    for scalar in &scalars {
        println!(
            "value={:?}, bytes={:?}, source={:?}",
            scalar.value, scalar.bytes, scalar.raw
        );
    }

    assert_eq!(scalars.len(), 4);
    assert_eq!(scalars[0].value, "café");
    assert_eq!(scalars[0].bytes, 0..5); // Four characters, five UTF-8 bytes.
    assert_eq!(scalars[1].value, "☺");
    assert_eq!(scalars[1].bytes, 7..15);
    assert_eq!(scalars[1].raw, r#""\u263A""#); // Quotes and escape are preserved.
    assert_eq!(scalars[2].bytes, 17..22); // Both CRLF bytes contribute to this offset.

    // This implicit null has a real, empty range at the ':' indicator. It is not a missing range,
    // and its synthetic scalar value must not be used to calculate the source length.
    assert_eq!(scalars[3].value, "~");
    assert_eq!(scalars[3].bytes, 22..22);
    assert_eq!(scalars[3].raw, "");

    // Even when the iterator came from a string, this constructor does not retain
    // byte offsets. Consumers must handle unavailability explicitly.
    for next in Parser::new_from_iter(source.chars()) {
        let (_, span) = next.map_err(|error| format!("YAML parse error: {error:?}"))?;
        assert_eq!(span.byte_range(), None);
        assert_eq!(span.slice(source), None);
    }

    Ok(())
}
