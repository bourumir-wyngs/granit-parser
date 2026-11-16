#[path = "support/snapshots.rs"]
mod snapshots;
use snapshots::{snapshot, ExpectedEvent, ExpectedStyle};

use granit_parser::{Event, Marker, Parser, ScanError, Span, StructureStyle};

fn seq(style: StructureStyle) -> ExpectedEvent {
    ExpectedEvent::sequence_start(style, 0, None)
}

fn map(style: StructureStyle) -> ExpectedEvent {
    ExpectedEvent::mapping_start(style, 0, None)
}

/// Run the parser through the string.
///
/// The parser is run through both the `StrInput` and `BufferedInput` variants. The resulting
/// events are then compared and must match.
///
/// # Returns
/// This function returns the events and associated spans if parsing succeeds, the error the parser returned otherwise.
///
/// # Panics
/// This function panics if there is a mismatch between the 2 parser invocations with the different
/// input traits.
fn run_parser_with_span(input: &str) -> Result<Vec<(Event<'_>, Span)>, ScanError> {
    let mut str_events = vec![];
    let mut str_error = None;
    let mut iter_events = vec![];
    let mut iter_error = None;

    for x in Parser::new_from_str(input) {
        match x {
            Ok(event) => str_events.push(event),
            Err(e) => {
                str_error = Some(e);
                break;
            }
        }
    }
    for x in Parser::new_from_iter(input.chars()) {
        match x {
            Ok(event) => iter_events.push(event),
            Err(e) => {
                iter_error = Some(e);
                break;
            }
        }
    }

    // eprintln!("str_events");
    // for x in &str_events {
    //     eprintln!("\t{x:?}");
    // }
    // eprintln!("iter_events");
    // for x in &iter_events {
    //     eprintln!("\t{x:?}");
    // }

    assert_eq!(snapshot(&str_events), snapshot(&iter_events));
    assert_eq!(str_error, iter_error);

    if let Some(err) = str_error {
        Err(err)
    } else {
        Ok(str_events)
    }
}

/// Run the parser through the string.
///
/// The parser is run through both the `StrInput` and `BufferedInput` variants. The resulting
/// events are then compared and must match.
///
/// # Returns
/// This function returns the events if parsing succeeds, the error the parser returned otherwise.
///
/// # Panics
/// This function panics if there is a mismatch between the 2 parser invocations with the different
/// input traits.
fn run_parser(input: &str) -> Result<Vec<Event<'_>>, ScanError> {
    Ok(run_parser_with_span(input)?
        .into_iter()
        .map(|x| x.0)
        .collect())
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_issue1() {
    // https://github.com/saphyr-rs/saphyr-parser/issues/1

    // Implicit mappings with a nested flow sequence prematurely end the mapping.
    //
    // [a: [42]]
    //        ^ This closing sequence character
    // ^ is interpreted as closing this sequence, where the implicit mapping starts.
    //
    // The current implementation does not allow for nested implicit mapping:
    //
    // [a: [b: c]]
    //      ^ this implicit mapping would be ignored
    let reference = r"
- a:
  - 42
";

    let expected = [
        ExpectedEvent::StreamStart,
        ExpectedEvent::document_start(false, None),
        seq(StructureStyle::Block),
        map(StructureStyle::Block),
        ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
        seq(StructureStyle::Block),
        ExpectedEvent::new_scalar("42".into(), ExpectedStyle::plain(), false, 0, None),
        ExpectedEvent::SequenceEnd,
        ExpectedEvent::MappingEnd,
        ExpectedEvent::SequenceEnd,
        ExpectedEvent::document_end(false),
        ExpectedEvent::StreamEnd,
    ];
    assert_eq!(
        snapshot(&run_parser(reference).unwrap()),
        snapshot(&expected)
    );
    let expected_flow = [
        ExpectedEvent::StreamStart,
        ExpectedEvent::document_start(false, None),
        seq(StructureStyle::Flow),
        map(StructureStyle::Flow),
        ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
        seq(StructureStyle::Flow),
        ExpectedEvent::new_scalar("42".into(), ExpectedStyle::plain(), false, 0, None),
        ExpectedEvent::SequenceEnd,
        ExpectedEvent::MappingEnd,
        ExpectedEvent::SequenceEnd,
        ExpectedEvent::document_end(false),
        ExpectedEvent::StreamEnd,
    ];
    assert_eq!(
        snapshot(&run_parser("[{a: [42]}]").unwrap()),
        snapshot(&expected_flow)
    );
    assert_eq!(
        snapshot(&run_parser("[a: [42]]").unwrap()),
        snapshot(&expected_flow)
    );
    assert_eq!(
        snapshot(&run_parser("[a: [1, 2]]").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
            seq(StructureStyle::Flow),
            ExpectedEvent::new_scalar("1".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("2".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    let expected_mapping = [
        ExpectedEvent::StreamStart,
        ExpectedEvent::document_start(false, None),
        seq(StructureStyle::Flow),
        map(StructureStyle::Flow),
        ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
        map(StructureStyle::Flow),
        ExpectedEvent::new_scalar("b".into(), ExpectedStyle::plain(), false, 0, None),
        ExpectedEvent::new_scalar("c".into(), ExpectedStyle::plain(), false, 0, None),
        ExpectedEvent::new_scalar("d".into(), ExpectedStyle::plain(), false, 0, None),
        ExpectedEvent::new_scalar("e".into(), ExpectedStyle::plain(), false, 0, None),
        ExpectedEvent::MappingEnd,
        ExpectedEvent::MappingEnd,
        ExpectedEvent::SequenceEnd,
        ExpectedEvent::document_end(false),
        ExpectedEvent::StreamEnd,
    ];
    assert_eq!(
        snapshot(&run_parser("[a: {b: c, d: e}]").unwrap()),
        snapshot(&expected_mapping)
    );

    // Other test cases derived from the bug

    // Implicit mapping in a complex key.
    assert_eq!(
        snapshot(&run_parser("[foo: [bar]]: baz").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            map(StructureStyle::Block),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("foo".into(), ExpectedStyle::plain(), false, 0, None),
            seq(StructureStyle::Flow),
            ExpectedEvent::new_scalar("bar".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::new_scalar("baz".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    // Implicit mappings with implicit null keys
    assert_eq!(
        snapshot(&run_parser("[:]").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    // Nested implicit mappings with implicit null keys
    assert_eq!(
        snapshot(&run_parser("[: [:]]").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    // Interleaved nested implicit and non-implicit mappings.
    assert_eq!(
        snapshot(&run_parser("[a: [  [b:]]]").unwrap()),
        //          ^   ^  ^ has an implicit mapping
        //          |   ` has no implicit mapping
        //          ` has an implicit mapping
        // We must make sure that the `MappingEnd` events are correctly issued for the first and
        // third nested sequences, but not the second.
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
            seq(StructureStyle::Flow),
            // No `MappingStart` here.
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("b".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            // No `MappingEnd` here.
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    // There needs to be a space between a `:` in a flow sequence and the value.
    assert!(run_parser("[:[:]]").is_err());
    assert!(run_parser("[a:[42]]").is_err());

    // Double-quoted keys may have a value without a space for JSON-compatibility.
    assert_eq!(
        snapshot(&run_parser(r#"["a":[]]"#).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("a".into(), ExpectedStyle::DoubleQuoted, false, 0, None),
            seq(StructureStyle::Flow),
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_flow_sequence_empty_key_with_value() {
    assert_eq!(
        snapshot(&run_parser("[: value]").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::new_scalar("value".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_flow_sequence_empty_key_state_is_flow_scoped() {
    assert_eq!(
        snapshot(&run_parser("[ : [foo] ]").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            seq(StructureStyle::Flow),
            ExpectedEvent::new_scalar("foo".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    assert_eq!(
        snapshot(&run_parser("[ : {a: b} ]").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("b".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    assert_eq!(
        snapshot(&run_parser("[{}, : foo]").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Flow),
            map(StructureStyle::Flow),
            ExpectedEvent::MappingEnd,
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::new_scalar("foo".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_flow_mapping_quoted_key_colon_on_next_line() {
    assert_eq!(
        snapshot(&run_parser("{\"foo\"\n: \"bar\"}\n").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("foo".into(), ExpectedStyle::DoubleQuoted, false, 0, None),
            ExpectedEvent::new_scalar("bar".into(), ExpectedStyle::DoubleQuoted, false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_flow_mapping_plain_key_spans_line_break() {
    assert_eq!(
        snapshot(&run_parser("- { single line: value}\n- { multi\n  line: value}\n").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Block),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("single line".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("value".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("multi line".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("value".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_flow_mapping_key_and_colon_on_separate_lines() {
    assert_eq!(
        snapshot(&run_parser("k: {\n k\n :\n v\n }\n").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("k".into(), ExpectedStyle::plain(), false, 0, None),
            map(StructureStyle::Flow),
            ExpectedEvent::new_scalar("k".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("v".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_pr12() {
    assert_eq!(
        snapshot(&run_parser("---\n- |\n  a").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(true, None),
            seq(StructureStyle::Block),
            ExpectedEvent::new_scalar(
                "a\n".into(),
                {
                    let mut syntax = granit_parser::BlockScalarSyntax::new(
                        granit_parser::Chomping::Clip,
                        None,
                        2,
                    )
                    .unwrap();
                    syntax.header_chars_before_span = 4;
                    syntax.header_bytes_before_span = 4;
                    ExpectedStyle::literal(syntax)
                },
                false,
                0,
                None
            ),
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_issue14() {
    // The following input creates an infinite loop.
    // https://github.com/saphyr-rs/saphyr/issues/14
    let s = "{---";
    let Err(error) = run_parser(s) else { panic!() };
    assert_eq!(error.info(), "unclosed bracket '{'");
    assert_eq!(error.marker().index(), 0);
}

#[test]
fn test_issue14_v2() {
    let s = "{...";
    let Err(error) = run_parser(s) else { panic!() };
    assert_eq!(error.info(), "unclosed bracket '{'");
    assert_eq!(error.marker().index(), 0);
}

#[test]
fn test_granit_issue14_colon_in_alias_name_is_not_value_indicator() {
    // YAML 1.2.2:
    //
    //   ns-anchor-char ::= ns-char - c-flow-indicator
    //
    // ':' is not a c-flow-indicator, so in `*foo: bar` the alias name is `foo:`.
    // This must not be parsed as alias `foo` used as a mapping key.
    //
    // The first line intentionally defines `&foo`. A buggy parser that stops the
    // alias at ':' will therefore parse the second line as a valid mapping entry.
    // A fixed parser consumes `foo:` as the alias name and then rejects the stray
    // `bar` node.
    let input = "foo: &foo value\n*foo: bar\n";

    assert!(
        run_parser(input).is_err(),
        "`*foo: bar` must be rejected: ':' is part of the alias name `foo:`, \
         not a mapping value indicator after alias `foo`"
    );
}

#[test]
fn test_granit_issue14_space_before_colon_keeps_alias_key_mapping_valid() {
    // With an explicit separation space before ':', the alias name is `foo`.
    // This is the valid alias-key mapping form.
    let input = "foo: &foo value\n*foo : bar\n";

    assert_eq!(
        snapshot(&run_parser(input).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("foo".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("value".into(), ExpectedStyle::plain(), false, 1, None),
            ExpectedEvent::alias(1),
            ExpectedEvent::new_scalar("bar".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_issue13() {
    // The following input creates an infinite loop.
    // https://github.com/saphyr-rs/saphyr/issues/13
    let s = r"---
array:
  - object:
      array:
        - object:
            array:
              - text: >-
                  Line 1
                  Line 2
...";

    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(true, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("array".into(), ExpectedStyle::plain(), false, 0, None),
            seq(StructureStyle::Block),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("object".into(), ExpectedStyle::plain(), false, 0, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("array".into(), ExpectedStyle::plain(), false, 0, None),
            seq(StructureStyle::Block),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("object".into(), ExpectedStyle::plain(), false, 0, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("array".into(), ExpectedStyle::plain(), false, 0, None),
            seq(StructureStyle::Block),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("text".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar(
                "Line 1 Line 2".into(),
                {
                    let mut syntax = granit_parser::BlockScalarSyntax::new(
                        granit_parser::Chomping::Strip,
                        None,
                        18,
                    )
                    .unwrap();
                    syntax.header_chars_before_span = 21;
                    syntax.header_bytes_before_span = 21;
                    ExpectedStyle::folded(syntax)
                },
                false,
                0,
                None
            ),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(true),
            ExpectedEvent::StreamEnd
        ]
    );
}

#[test]
fn test_issue22() {
    // The ellipsis is parsed as a document end.
    // https://github.com/saphyr-rs/saphyr/issues/22
    let s = "comment: hello ... world";
    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("comment".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar(
                "hello ... world".into(),
                ExpectedStyle::plain(),
                false,
                0,
                None
            ),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd
        ]
    );
}

#[test]
#[rustfmt::skip]
fn test_issue37() {
    // MarkedYaml: Empty scalars get the span of the next value.
    // With the fix they get the position of the ":" for block maps and the "-" for block arrays.
    let s = r"---
    hash_block_null_value:
    hash_flow: {hash_flow_null_value: null}
    array_block_null_value:
      -
      - ~
      - null
    array_flow_null_value: [~, null]
    indentless_array_block_null_value:
    -
    - ~
    - null
    ";

    assert_eq!( snapshot(&run_parser_with_span(s).unwrap()),[
            (ExpectedEvent::StreamStart,                                                          Span::new(Marker::new(0, 1, 0).with_byte_offset(Some(0)), Marker::new(0, 1, 0).with_byte_offset(Some(0)))),
            (ExpectedEvent::document_start(true, None),                                                  Span::new(Marker::new(0, 1, 0).with_byte_offset(Some(0)), Marker::new(3, 1, 3).with_byte_offset(Some(3)))),
            (map(StructureStyle::Block),                                                Span::new(Marker::new(8, 2, 4).with_byte_offset(Some(8)), Marker::new(8, 2, 4).with_byte_offset(Some(8)))),
            (ExpectedEvent::new_scalar("hash_block_null_value".into(), ExpectedStyle::plain(), false, 0, None),  Span::new(Marker::new(8, 2, 4).with_byte_offset(Some(8)), Marker::new(29, 2, 25).with_byte_offset(Some(29))).with_indent(Some(4))),

            (ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),                      Span::new(Marker::new(29, 2, 25).with_byte_offset(Some(29)), Marker::new(29, 2, 25).with_byte_offset(Some(29)))),

            (ExpectedEvent::new_scalar("hash_flow".into(), ExpectedStyle::plain(), false, 0, None),              Span::new(Marker::new(35, 3, 4).with_byte_offset(Some(35)), Marker::new(44, 3, 13).with_byte_offset(Some(44))).with_indent(Some(4))),
            (map(StructureStyle::Flow),                                                 Span::new(Marker::new(46, 3, 15).with_byte_offset(Some(46)), Marker::new(47, 3, 16).with_byte_offset(Some(47)))),
            (ExpectedEvent::new_scalar("hash_flow_null_value".into(), ExpectedStyle::plain(), false, 0, None),   Span::new(Marker::new(47, 3, 16).with_byte_offset(Some(47)), Marker::new(67, 3, 36).with_byte_offset(Some(67)))),
            (ExpectedEvent::new_scalar("null".into(), ExpectedStyle::plain(), false, 0, None),                   Span::new(Marker::new(69, 3, 38).with_byte_offset(Some(69)), Marker::new(73, 3, 42).with_byte_offset(Some(73)))),
            (ExpectedEvent::MappingEnd,                                                           Span::new(Marker::new(73, 3, 42).with_byte_offset(Some(73)), Marker::new(74, 3, 43).with_byte_offset(Some(74)))),
            (ExpectedEvent::new_scalar("array_block_null_value".into(), ExpectedStyle::plain(), false, 0, None), Span::new(Marker::new(79, 4, 4).with_byte_offset(Some(79)), Marker::new(101, 4, 26).with_byte_offset(Some(101))).with_indent(Some(4))),
            (seq(StructureStyle::Block),                                               Span::new(Marker::new(109, 5, 6).with_byte_offset(Some(109)), Marker::new(109, 5, 6).with_byte_offset(Some(109)))),

            (ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),                      Span::new(Marker::new(110, 5, 7).with_byte_offset(Some(110)), Marker::new(110, 5, 7).with_byte_offset(Some(110)))),

            (ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), false, 0, None),                                 Span::new(Marker::new(119, 6, 8).with_byte_offset(Some(119)), Marker::new(120, 6, 9).with_byte_offset(Some(120)))),
            (ExpectedEvent::new_scalar("null".into(), ExpectedStyle::plain(), false, 0, None),                              Span::new(Marker::new(129, 7, 8).with_byte_offset(Some(129)), Marker::new(133, 7, 12).with_byte_offset(Some(133)))),
            (ExpectedEvent::SequenceEnd,                                                                     Span::new(Marker::new(138, 8, 4).with_byte_offset(Some(138)), Marker::new(138, 8, 4).with_byte_offset(Some(138)))),
            (ExpectedEvent::new_scalar("array_flow_null_value".into(), ExpectedStyle::plain(), false, 0, None),             Span::new(Marker::new(138, 8, 4).with_byte_offset(Some(138)), Marker::new(159, 8, 25).with_byte_offset(Some(159))).with_indent(Some(4))),
            (seq(StructureStyle::Flow),                                                           Span::new(Marker::new(161, 8, 27).with_byte_offset(Some(161)), Marker::new(162, 8, 28).with_byte_offset(Some(162)))),
            (ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), false, 0, None),                                 Span::new(Marker::new(162, 8, 28).with_byte_offset(Some(162)), Marker::new(163, 8, 29).with_byte_offset(Some(163)))),
            (ExpectedEvent::new_scalar("null".into(), ExpectedStyle::plain(), false, 0, None),                              Span::new(Marker::new(165, 8, 31).with_byte_offset(Some(165)), Marker::new(169, 8, 35).with_byte_offset(Some(169)))),
            (ExpectedEvent::SequenceEnd,                                                                     Span::new(Marker::new(169, 8, 35).with_byte_offset(Some(169)), Marker::new(170, 8, 36).with_byte_offset(Some(170)))),
            (ExpectedEvent::new_scalar("indentless_array_block_null_value".into(), ExpectedStyle::plain(), false, 0, None), Span::new(Marker::new(175, 9, 4).with_byte_offset(Some(175)), Marker::new(208, 9, 37).with_byte_offset(Some(208))).with_indent(Some(4))),
            (seq(StructureStyle::Block),                                                          Span::new(Marker::new(215, 10, 5).with_byte_offset(Some(215)), Marker::new(215, 10, 5).with_byte_offset(Some(215)))),

            (ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),    Span::new(Marker::new(215, 10, 5).with_byte_offset(Some(215)), Marker::new(215, 10, 5).with_byte_offset(Some(215)))),

            (ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), false, 0, None),    Span::new(Marker::new(222, 11, 6).with_byte_offset(Some(222)), Marker::new(223, 11, 7).with_byte_offset(Some(223)))),
            (ExpectedEvent::new_scalar("null".into(), ExpectedStyle::plain(), false, 0, None), Span::new(Marker::new(230, 12, 6).with_byte_offset(Some(230)), Marker::new(234, 12, 10).with_byte_offset(Some(234)))),
            // The input ends after four spaces on line 13, without a final line break.
            (ExpectedEvent::SequenceEnd,                                        Span::new(Marker::new(239, 13, 4).with_byte_offset(Some(239)), Marker::new(239, 13, 4).with_byte_offset(Some(239)))),
            (ExpectedEvent::MappingEnd,                                         Span::new(Marker::new(239, 13, 4).with_byte_offset(Some(239)), Marker::new(239, 13, 4).with_byte_offset(Some(239)))),
            (ExpectedEvent::document_end(false),                                        Span::new(Marker::new(239, 13, 4).with_byte_offset(Some(239)), Marker::new(239, 13, 4).with_byte_offset(Some(239)))),
            (ExpectedEvent::StreamEnd,                                          Span::new(Marker::new(239, 13, 4).with_byte_offset(Some(239)), Marker::new(239, 13, 4).with_byte_offset(Some(239))))
        ]);
}

#[test]
fn test_issue84() {
    // https://github.com/saphyr-rs/saphyr/issues/84
    let s = r"hello:
  world: this is a string
    --- still a string";

    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("hello".into(), ExpectedStyle::plain(), false, 0, None),
            map(StructureStyle::Block),
            ExpectedEvent::new_scalar("world".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar(
                "this is a string --- still a string".into(),
                ExpectedStyle::plain(),
                false,
                0,
                None
            ),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_granit_issue14_anchor_and_alias_names_may_end_with_colon() {
    // YAML 1.2.2:
    //
    //   ns-anchor-char ::= ns-char - c-flow-indicator
    //   ns-anchor-name ::= ns-anchor-char+
    //
    // `:` is not a flow indicator. Therefore both `&foo:` and `*foo:`
    // use the anchor/alias name `foo:`.
    //
    // A buggy scanner that stops anchor/alias names before `:` will tokenize
    // this as `&foo` / `*foo` followed by a mapping value indicator, producing
    // either a parse error or the wrong event stream.
    let input = "- &foo: value\n- *foo:\n";

    assert_eq!(
        snapshot(&run_parser(input).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            seq(StructureStyle::Block),
            ExpectedEvent::new_scalar("value".into(), ExpectedStyle::plain(), false, 1, None),
            ExpectedEvent::alias(1),
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}
