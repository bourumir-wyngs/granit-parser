#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::float_cmp)]
use granit_parser::{Event, Marker, Parser, ScanError};

fn char_index_to_byte_index(s: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    s.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or_else(|| s.len())
}

fn span_offsets(input: &str, start: Marker, end: Marker) -> (usize, usize) {
    let start_b = start
        .byte_offset()
        .unwrap_or_else(|| char_index_to_byte_index(input, start.index()));
    let end_b = end
        .byte_offset()
        .unwrap_or_else(|| char_index_to_byte_index(input, end.index()));
    (start_b, end_b)
}

/// Run the parser through the string, returning all the scalars, and collecting their spans to strings.
fn run_parser_and_deref_scalar_spans(input: &str) -> Result<Vec<(String, String)>, ScanError> {
    let mut events = vec![];
    for x in Parser::new_from_str(input) {
        let x = x?;
        if let Event::Scalar(s, ..) = x.0 {
            let (start, end) = span_offsets(input, x.1.start, x.1.end);
            let input_s = &input[start..end];
            events.push((s.into(), input_s.to_string()));
        }
    }
    Ok(events)
}

/// Run the parser through the string, returning all the scalars, and collecting their spans to strings.
fn run_parser_and_deref_seq_spans(input: &str) -> Result<Vec<String>, ScanError> {
    let mut events = vec![];
    let mut start_stack = vec![];
    for x in Parser::new_from_str(input) {
        let x = x?;
        match x.0 {
            Event::SequenceStart(_, _) => start_stack.push(x.1.start),
            Event::SequenceEnd => {
                let start = start_stack.pop().unwrap();
                let (start, end) = span_offsets(input, start, x.1.end);
                let input_s = &input[start..end];
                events.push(input_s.to_string());
            }
            _ => {}
        }
    }
    Ok(events)
}

fn deref_pairs(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect()
}

#[test]
fn test_plain() {
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: bar").unwrap()),
        [("foo", "foo"), ("bar", "bar"),]
    );
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: bar ").unwrap()),
        [("foo", "foo"), ("bar", "bar"),]
    );
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo :  \t  bar\t ").unwrap()),
        [("foo", "foo"), ("bar", "bar"),]
    );

    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo :  \n  - bar\n  - baz\n ").unwrap()),
        [("foo", "foo"), ("bar", "bar"), ("baz", "baz")]
    );
}

#[test]
fn test_plain_utf8() {
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("a: \u{4F60}\u{5273}").unwrap()),
        [("a", "a"), ("\u{4F60}\u{5273}", "\u{4F60}\u{5273}")]
    );
}

#[test]
fn test_quoted() {
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans(r#"foo: "bar""#).unwrap()),
        [("foo", "foo"), ("bar", r#""bar""#),]
    );
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans(r"foo: 'bar'").unwrap()),
        [("foo", "foo"), ("bar", r"'bar'"),]
    );

    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans(r#"foo: "bar ""#).unwrap()),
        [("foo", "foo"), ("bar ", r#""bar ""#),]
    );
}

#[test]
fn test_literal() {
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: |\n  bar").unwrap()),
        [("foo", "foo"), ("bar\n", "bar"),]
    );
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: |\n  bar\n  more").unwrap()),
        [("foo", "foo"), ("bar\nmore\n", "bar\n  more"),]
    );
}

#[test]
fn test_block() {
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: >\n  bar").unwrap()),
        [("foo", "foo"), ("bar\n", "bar"),]
    );
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: >\n  bar\n  more").unwrap()),
        [("foo", "foo"), ("bar more\n", "bar\n  more"),]
    );
}

#[test]
fn test_seq() {
    assert_eq!(
        run_parser_and_deref_seq_spans("[a, b]").unwrap(),
        ["[a, b]"]
    );
    assert_eq!(
        run_parser_and_deref_seq_spans("- a\n- b").unwrap(),
        ["- a\n- b"]
    );
    assert_eq!(
        run_parser_and_deref_seq_spans("foo:\n  - a\n  - b").unwrap(),
        ["- a\n  - b"]
    );
    assert_eq!(
        run_parser_and_deref_seq_spans("foo:\n  - a\n  - bar:\n    - b\n    - c").unwrap(),
        ["b\n    - c", "- a\n  - bar:\n    - b\n    - c"]
    );
}

#[test]
fn test_literal_utf8() {
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: |\n  \u{4F60}\u{5273}").unwrap()),
        [("foo", "foo"), ("\u{4F60}\u{5273}\n", "\u{4F60}\u{5273}"),]
    );
    assert_eq!(
        deref_pairs(
            &run_parser_and_deref_scalar_spans(
                "foo: |\n  one:\u{4F60}\u{5273}\n  two:\u{4F60}\u{5273}"
            )
            .unwrap()
        ),
        [
            ("foo", "foo"),
            (
                "one:\u{4F60}\u{5273}\ntwo:\u{4F60}\u{5273}\n",
                "one:\u{4F60}\u{5273}\n  two:\u{4F60}\u{5273}"
            ),
        ]
    );
}

#[test]
fn test_block_utf8() {
    assert_eq!(
        deref_pairs(&run_parser_and_deref_scalar_spans("foo: >\n  \u{4F60}\u{5273}").unwrap()),
        [("foo", "foo"), ("\u{4F60}\u{5273}\n", "\u{4F60}\u{5273}")],
    );
    assert_eq!(
        deref_pairs(
            &run_parser_and_deref_scalar_spans(
                "foo: >\n  one:\u{4F60}\u{5273}\n  two:\u{4F60}\u{5273}"
            )
            .unwrap()
        ),
        [
            ("foo", "foo"),
            (
                "one:\u{4F60}\u{5273} two:\u{4F60}\u{5273}\n",
                "one:\u{4F60}\u{5273}\n  two:\u{4F60}\u{5273}"
            )
        ],
    );
}
