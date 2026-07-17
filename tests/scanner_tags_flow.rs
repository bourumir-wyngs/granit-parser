//! Coverage tests for scanner fallback paths around tags, tag directives, anchors and flow
//! scalars.
//!
//! Many of the exercised branches are only taken when the input does not support zero-copy
//! borrowing. They are reached through:
//! - [`BufferedInput`] (streaming input: `byte_offset()` is `None`), and
//! - custom [`Input`] implementations that advertise byte offsets but cannot (always) provide
//!   slices, which is a legal implementation of the optional slicing capability.

use std::cell::Cell;

use granit_parser::{
    BorrowedInput, BufferedInput, Event, Input, Parser, ScalarStyle, ScanError, Scanner, StrInput,
    Tag,
};

// --- Custom inputs ------------------------------------------------------------------------

/// Controls how [`OpaqueInput::slice_bytes`] behaves.
#[derive(Clone, Copy)]
enum SliceMode {
    /// `slice_bytes` delegates to the inner `StrInput` (offsets available, no zero-copy borrow).
    Delegate,
    /// `slice_bytes` always returns `None` (offsets available, no slicing at all).
    Never,
    /// `slice_bytes` returns `None` only for empty ranges.
    NoneWhenEmpty,
    /// `slice_bytes` delegates for the first call, then returns `None`.
    FirstCallOnly,
}

/// A `StrInput` wrapper that never hands out `'input`-borrowed slices, with configurable
/// `slice_bytes` behavior. This models streaming inputs that track byte offsets but have no
/// stable backing storage.
struct OpaqueInput<'a> {
    inner: StrInput<'a>,
    mode: SliceMode,
    slice_calls: Cell<usize>,
}

impl<'a> OpaqueInput<'a> {
    fn new(source: &'a str, mode: SliceMode) -> Self {
        Self {
            inner: StrInput::new(source),
            mode,
            slice_calls: Cell::new(0),
        }
    }
}

impl Input for OpaqueInput<'_> {
    fn lookahead(&mut self, count: usize) {
        self.inner.lookahead(count);
    }

    fn buflen(&self) -> usize {
        self.inner.buflen()
    }

    fn bufmaxlen(&self) -> usize {
        self.inner.bufmaxlen()
    }

    fn raw_read_ch(&mut self) -> char {
        self.inner.raw_read_ch()
    }

    fn raw_read_non_breakz_ch(&mut self) -> Option<char> {
        self.inner.raw_read_non_breakz_ch()
    }

    fn skip(&mut self) {
        self.inner.skip();
    }

    fn skip_n(&mut self, count: usize) {
        self.inner.skip_n(count);
    }

    fn peek(&self) -> char {
        self.inner.peek()
    }

    fn peek_nth(&self, n: usize) -> char {
        self.inner.peek_nth(n)
    }

    fn byte_offset(&self) -> Option<usize> {
        self.inner.byte_offset()
    }

    fn slice_bytes(&self, start: usize, end: usize) -> Option<&str> {
        match self.mode {
            SliceMode::Delegate => self.inner.slice_bytes(start, end),
            SliceMode::Never => None,
            SliceMode::NoneWhenEmpty => {
                if start == end {
                    None
                } else {
                    self.inner.slice_bytes(start, end)
                }
            }
            SliceMode::FirstCallOnly => {
                let calls = self.slice_calls.get();
                self.slice_calls.set(calls + 1);
                if calls == 0 {
                    self.inner.slice_bytes(start, end)
                } else {
                    None
                }
            }
        }
    }
}

impl<'a> BorrowedInput<'a> for OpaqueInput<'a> {
    fn slice_borrowed(&self, _start: usize, _end: usize) -> Option<&'a str> {
        None
    }
}

// --- Helpers ------------------------------------------------------------------------------

fn collect_events<'input, T: BorrowedInput<'input>>(
    input: T,
) -> Result<Vec<Event<'input>>, ScanError> {
    Parser::new(input)
        .map(|event| event.map(|(event, _)| event))
        .collect()
}

fn first_error<'input, T: BorrowedInput<'input>>(input: T) -> ScanError {
    for event in Parser::new(input) {
        if let Err(error) = event {
            return error;
        }
    }
    panic!("expected parser error");
}

fn buffered(source: &str) -> BufferedInput<std::str::Chars<'_>> {
    BufferedInput::new(source.chars())
}

/// Extract `(value, tag)` from the first tagged scalar event.
fn first_tagged_scalar(events: &[Event<'_>]) -> (String, Tag) {
    events
        .iter()
        .find_map(|event| {
            if let Event::Scalar(value, _, _, Some(tag)) = event {
                Some((value.to_string(), tag.clone().into_owned()))
            } else {
                None
            }
        })
        .expect("expected a tagged scalar event")
}

// --- Streaming input (`byte_offset() == None`): owned tag scanning paths -------------------

#[test]
fn streaming_tag_directive_resolves_shorthand_tag() {
    let events = collect_events(buffered(
        "%TAG !e! tag:example.com,2000:app/\n---\n!e!foo bar\n",
    ))
    .expect("valid YAML must parse from a streaming input");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "bar");
    assert_eq!(tag.handle(), "tag:example.com,2000:app/");
    assert_eq!(tag.suffix(), "foo");
    assert_eq!(tag.original_handle(), "!e!");
}

#[test]
fn streaming_tag_directive_handle_without_trailing_bang_errors() {
    let error = first_error(buffered("%TAG !e tag:example.com\n---\nx\n"));
    assert_eq!(
        error.info(),
        "while parsing a tag directive, did not find expected '!'"
    );
}

#[test]
fn streaming_tag_directive_prefix_rejects_invalid_global_tag_char() {
    let error = first_error(buffered("%TAG !e! }bad\n---\nx\n"));
    assert_eq!(error.info(), "invalid global tag character");
}

#[test]
fn streaming_tag_directive_prefix_decodes_uri_escape() {
    let events = collect_events(buffered("%TAG !e! tag:ex%61mple/\n---\n!e!x 1\n"))
        .expect("escaped tag prefix must parse from a streaming input");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "1");
    assert_eq!(tag.handle(), "tag:example/");
    assert_eq!(tag.suffix(), "x");
}

#[test]
fn streaming_anchor_without_name_errors() {
    let error = first_error(buffered("&\n"));
    assert_eq!(
        error.info(),
        "while scanning an anchor or alias, did not find expected alphabetic or numeric character"
    );
}

// --- Plain `StrInput` edge cases ------------------------------------------------------------

#[test]
fn tag_directive_prefix_starting_with_uri_escape_is_decoded() {
    let events = collect_events(StrInput::new("%TAG !e! %61pp/\n--- !e!x 1\n"))
        .expect("prefix starting with an escape must parse");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "1");
    assert_eq!(tag.handle(), "app/");
    assert_eq!(tag.suffix(), "x");
}

#[test]
fn verbatim_tag_with_uri_escape_is_decoded() {
    let events = collect_events(StrInput::new("--- !<tag:ex%61mple> 1\n"))
        .expect("verbatim tag with escape must parse");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "1");
    assert_eq!(format!("{}{}", tag.handle(), tag.suffix()), "tag:example");
}

#[test]
fn directive_name_with_control_character_errors() {
    let error = first_error(StrInput::new("%YA\u{7}ML 1.2\n---\nx\n"));
    assert_eq!(
        error.info(),
        "while scanning a directive, found unexpected non-alphabetical character"
    );
}

#[test]
fn required_simple_key_at_eof_without_newline_errors() {
    // `b` opens a required simple key inside the block mapping; the stream ends on the same
    // line, so the error is reported by the stream-end check rather than key staleness.
    let error = first_error(StrInput::new("a: 1\nb"));
    assert_eq!(error.info(), "simple key expected ':'");
    assert_eq!(error.marker().line(), 2);
    assert_eq!(error.marker().col(), 0);
}

#[test]
fn scanner_error_is_terminal() {
    let mut scanner = Scanner::new(StrInput::new("[\n--- ]\n"));

    let error = scanner
        .by_ref()
        .find_map(Result::err)
        .expect("scanner should emit an error");

    assert_eq!(error.info(), "unclosed bracket '['");
    assert!(scanner.next().is_none());
}

// --- Offsets available but no zero-copy borrowing (`SliceMode::Delegate`) -------------------

#[test]
fn no_borrow_input_resolves_tag_directive_via_owned_slices() {
    let events = collect_events(OpaqueInput::new(
        "%TAG !e! tag:e/\n---\n!e!foo 1\n",
        SliceMode::Delegate,
    ))
    .expect("tag directive must parse without zero-copy borrowing");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "1");
    assert_eq!(tag.handle(), "tag:e/");
    assert_eq!(tag.suffix(), "foo");
    assert_eq!(tag.original_handle(), "!e!");
}

#[test]
fn no_borrow_input_rejects_tag_directive_handle_without_bang() {
    let error = first_error(OpaqueInput::new(
        "%TAG !e tag:e/\n---\nx\n",
        SliceMode::Delegate,
    ));
    assert_eq!(
        error.info(),
        "while parsing a tag directive, did not find expected '!'"
    );
}

#[test]
fn no_borrow_input_scans_primary_handle_tag_as_owned() {
    let events = collect_events(OpaqueInput::new("!foo 1\n", SliceMode::Delegate))
        .expect("local tag must parse without zero-copy borrowing");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "1");
    assert_eq!(tag.handle(), "!");
    assert_eq!(tag.suffix(), "foo");
}

#[test]
fn no_borrow_input_scans_anchor_and_alias_as_owned() {
    let events = collect_events(OpaqueInput::new("- &a 1\n- *a\n", SliceMode::Delegate))
        .expect("anchors must parse without zero-copy borrowing");

    let anchor_id = events
        .iter()
        .find_map(|event| {
            if let Event::Scalar(value, ScalarStyle::Plain, anchor_id, None) = event {
                (value == "1").then_some(*anchor_id)
            } else {
                None
            }
        })
        .expect("expected the anchored scalar");
    assert_ne!(anchor_id, 0, "anchored scalar must be assigned an id");
    assert!(
        events.contains(&Event::Alias(anchor_id)),
        "alias must reference the anchored scalar id"
    );
}

// --- Offsets available but `slice_bytes` unavailable (`SliceMode::Never`) -------------------

#[test]
fn sliceless_input_reports_internal_error_when_promoting_flow_scalar() {
    let error = first_error(OpaqueInput::new("'a''b'\n", SliceMode::Never));
    assert_eq!(
        error.info(),
        "internal error: input advertised offsets but did not provide a slice"
    );
}

#[test]
fn sliceless_input_reports_internal_error_in_tag_directive_handle() {
    let error = first_error(OpaqueInput::new(
        "%TAG !e! tag:e/\n---\nx\n",
        SliceMode::Never,
    ));
    assert_eq!(
        error.info(),
        "internal error: input advertised slicing but did not provide a slice"
    );
}

#[test]
fn sliceless_input_reports_internal_error_in_tag_handle() {
    let error = first_error(OpaqueInput::new("!!str 1\n", SliceMode::Never));
    assert_eq!(
        error.info(),
        "internal error: input advertised slicing but did not provide a slice"
    );
}

#[test]
fn sliceless_input_reports_internal_error_in_anchor() {
    let error = first_error(OpaqueInput::new("&a 1\n", SliceMode::Never));
    assert_eq!(
        error.info(),
        "internal error: input advertised slicing but did not provide a slice"
    );
}

// --- `slice_bytes` unavailable only for empty ranges (`SliceMode::NoneWhenEmpty`) -----------

#[test]
fn empty_range_sliceless_input_decodes_tag_directive_prefix_escape() {
    let events = collect_events(OpaqueInput::new(
        "%TAG !e! %61pp/\n--- !e!x 1\n",
        SliceMode::NoneWhenEmpty,
    ))
    .expect("prefix starting with an escape must parse without empty-range slices");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "1");
    assert_eq!(tag.handle(), "app/");
    assert_eq!(tag.suffix(), "x");
}

#[test]
fn empty_range_sliceless_input_decodes_tag_suffix_escape() {
    let events = collect_events(OpaqueInput::new("!%61bc 1\n", SliceMode::NoneWhenEmpty))
        .expect("tag suffix starting with an escape must parse without empty-range slices");

    let (value, tag) = first_tagged_scalar(&events);
    assert_eq!(value, "1");
    assert_eq!(tag.handle(), "!");
    assert_eq!(tag.suffix(), "abc");
}

// --- `slice_bytes` fails after the first call (`SliceMode::FirstCallOnly`) ------------------

#[test]
fn slice_loss_between_handle_and_prefix_reports_internal_error() {
    // The tag directive handle is sliced successfully (first call), then slicing the prefix
    // fails, hitting the defensive fallback in the prefix scanner.
    let error = first_error(OpaqueInput::new(
        "%TAG !e! tag:e/\n---\nx\n",
        SliceMode::FirstCallOnly,
    ));
    assert_eq!(
        error.info(),
        "internal error: input advertised slicing but did not provide a slice"
    );
}

#[test]
fn slice_loss_between_tag_handle_and_suffix_reports_internal_error() {
    // The tag handle `!e!` is sliced successfully (first call), then slicing the suffix fails,
    // hitting the defensive fallback in the shorthand suffix scanner.
    let error = first_error(OpaqueInput::new("!e!foo 1\n", SliceMode::FirstCallOnly));
    assert_eq!(
        error.info(),
        "internal error: input advertised slicing but did not provide a slice"
    );
}
