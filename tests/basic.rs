#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::float_cmp)]
#[path = "support/snapshots.rs"]
mod snapshots;
use snapshots::{snapshot, ExpectedEvent, ExpectedStyle};

mod support;
#[cfg(feature = "comments")]
use granit_parser::Placement;
use granit_parser::{Event, Parser, ScalarStyle, ScanError, StructureStyle, YamlVersion};

/// Run the parser through the string.
///
/// # Returns
/// This functions returns the events if parsing succeeds, the error the parser returned otherwise.
fn run_parser(input: &str) -> Result<Vec<Event<'_>>, ScanError> {
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
        Ok(str_events.into_iter().map(|x| x.0).collect())
    }
}

fn collection_styles(input: &str) -> Vec<(&'static str, StructureStyle)> {
    run_parser(input)
        .unwrap()
        .into_iter()
        .filter_map(|event| match event {
            Event::SequenceStart { style, .. } => Some(("sequence", style)),
            Event::MappingStart { style, .. } => Some(("mapping", style)),
            _ => None,
        })
        .collect()
}

fn expected_events(events: impl IntoIterator<Item = ExpectedEvent>) -> Vec<ExpectedEvent> {
    events.into_iter().collect()
}

#[test]
fn test_fail() {
    let s = "
# syntax error
scalar
key: [1, 2]]
key1:a2
";
    let Err(error) = run_parser(s) else { panic!() };
    assert_eq!(
        error.info(),
        "mapping values are not allowed in this context"
    );
    assert_eq!(
        error.to_string(),
        "mapping values are not allowed in this context at char 26 line 4 column 4"
    );
}

#[test]
fn test_sequence_structure_styles() {
    assert_eq!(
        collection_styles("- block\n- [flow]\n"),
        vec![
            ("sequence", StructureStyle::Block),
            ("sequence", StructureStyle::Flow),
        ]
    );

    assert_eq!(
        collection_styles("[flow, [nested]]\n"),
        vec![
            ("sequence", StructureStyle::Flow),
            ("sequence", StructureStyle::Flow),
        ]
    );
}

#[test]
fn test_mapping_structure_styles() {
    assert_eq!(
        collection_styles("block:\n  child: value\nflow: {child: value}\n"),
        vec![
            ("mapping", StructureStyle::Block),
            ("mapping", StructureStyle::Block),
            ("mapping", StructureStyle::Flow),
        ]
    );

    assert_eq!(
        collection_styles("[implicit: flow]\n"),
        vec![
            ("sequence", StructureStyle::Flow),
            ("mapping", StructureStyle::Flow),
        ]
    );
}

#[test]
fn test_empty_doc() {
    assert_eq!(
        snapshot(&run_parser("").unwrap()),
        [ExpectedEvent::StreamStart, ExpectedEvent::StreamEnd]
    );

    assert_eq!(
        snapshot(&run_parser("---").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(true, None),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_utf() {
    assert_eq!(
        snapshot(&run_parser("a: \u{4F60}\u{5273}").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::mapping_start(StructureStyle::Block, 0, None),
            ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar(
                "\u{4F60}\u{5273}".into(),
                ExpectedStyle::plain(),
                false,
                0,
                None
            ),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_comments() {
    let s = "
# This is a comment
a: b # This is another comment
##
  #
";

    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        expected_events([
            ExpectedEvent::StreamStart,
            #[cfg(feature = "comments")]
            ExpectedEvent::comment(" This is a comment".into(), Placement::Above),
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::mapping_start(StructureStyle::Block, 0, None),
            ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("b".into(), ExpectedStyle::plain(), false, 0, None),
            #[cfg(feature = "comments")]
            ExpectedEvent::comment(" This is another comment".into(), Placement::Right),
            #[cfg(feature = "comments")]
            ExpectedEvent::comment("#".into(), Placement::Above),
            #[cfg(feature = "comments")]
            ExpectedEvent::comment("".into(), Placement::Above),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ])
    );
}

#[test]
fn test_quoting() {
    let s = "
- plain
- 'squote'
- \"dquote\"
";

    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::sequence_start(StructureStyle::Block, 0, None),
            ExpectedEvent::new_scalar("plain".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar("squote".into(), ExpectedStyle::SingleQuoted, false, 0, None),
            ExpectedEvent::new_scalar("dquote".into(), ExpectedStyle::DoubleQuoted, false, 0, None),
            ExpectedEvent::SequenceEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_multi_doc() {
    let s = "
a scalar
---
a scalar
---
a scalar
";
    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::new_scalar("a scalar".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::document_start(true, None),
            ExpectedEvent::new_scalar("a scalar".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::document_start(true, None),
            ExpectedEvent::new_scalar("a scalar".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_github_27() {
    // https://github.com/chyh1990/yaml-rust/issues/27
    assert_eq!(
        snapshot(&run_parser("&a").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 1, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_missing_node_with_anchor_is_null_scalar_but_tag_keeps_empty_content() {
    let scalar_events = |input: &str| -> Vec<(
        String,
        granit_parser::ScalarKind,
        bool,
        usize,
        Option<String>,
    )> {
        run_parser(input)
            .unwrap()
            .into_iter()
            .filter_map(|event| match event {
                Event::Scalar {
                    value,
                    style,
                    anchor_id: anchor,
                    tag,
                    ..
                } => Some((
                    value.into_owned(),
                    style.kind(),
                    matches!(style, ScalarStyle::Plain { implicit: true, .. }),
                    anchor,
                    tag.map(|tag| tag.original()),
                )),
                _ => None,
            })
            .collect()
    };

    assert_eq!(
        scalar_events("a: &x\n"),
        vec![
            (
                "a".to_string(),
                granit_parser::ScalarKind::Plain,
                false,
                0,
                None
            ),
            (
                "~".to_string(),
                granit_parser::ScalarKind::Plain,
                true,
                1,
                None
            ),
        ]
    );
    assert_eq!(
        scalar_events("a: !!str\n"),
        vec![
            (
                "a".to_string(),
                granit_parser::ScalarKind::Plain,
                false,
                0,
                None
            ),
            (
                String::new(),
                granit_parser::ScalarKind::Plain,
                true,
                0,
                Some("!!str".to_string()),
            ),
        ]
    );
}

#[test]
fn test_bad_hyphen() {
    // See: https://github.com/chyh1990/yaml-rust/issues/23
    assert!(run_parser("{-").is_err());
}

#[test]
fn test_issue_65() {
    // See: https://github.com/chyh1990/yaml-rust/issues/65
    let b = "\n\"ll\\\"ll\\\r\n\"ll\\\"ll\\\r\r\r\rU\r\r\rU";
    assert!(run_parser(b).is_err());
}

#[test]
fn test_issue_65_mwe() {
    // A MWE for `test_issue_65`. The error over there is that there is invalid trailing content
    // after a double quoted string.
    let b = r#""foo" l"#;
    assert!(run_parser(b).is_err());
}

#[test]
fn test_comment_after_tag() {
    // https://github.com/Ethiraric/yaml-rust2/issues/21#issuecomment-2053513507
    let s = "
%YAML 1.2
# This is a comment
--- #-------
foobar";

    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        expected_events([
            ExpectedEvent::StreamStart,
            #[cfg(feature = "comments")]
            ExpectedEvent::comment(" This is a comment".into(), Placement::Above),
            ExpectedEvent::document_start(true, Some(YamlVersion::new(1, 2))),
            #[cfg(feature = "comments")]
            ExpectedEvent::comment("-------".into(), Placement::Right),
            ExpectedEvent::new_scalar("foobar".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ])
    );
}

#[test]
fn test_directive_followed_by_comment_then_content_errors() {
    for yaml in ["%YAML 1.2\n# c\nfoo\n--- bar\n", "%YAML 1.2\n# c\n"] {
        let error = run_parser(yaml)
            .expect_err("directives must still be followed by an explicit document start");

        assert_eq!(error.info(), "did not find expected <document start>");
    }
}

#[test]
fn test_empty_block_scalar_value_does_not_depend_on_eof() {
    fn value_of(yaml: &str) -> String {
        run_parser(yaml)
            .unwrap()
            .into_iter()
            .find_map(|event| match event {
                Event::Scalar {
                    value,
                    style: ScalarStyle::Literal { .. },
                    ..
                } => Some(value.into_owned()),
                _ => None,
            })
            .unwrap()
    }

    assert_eq!(value_of("a: |\nb: c\n"), value_of("a: |\n"));
    assert_eq!(value_of("a: |+\nb: c\n"), value_of("a: |+\n"));
    assert_eq!(value_of("a: |+\n\n"), "\n");
}

#[test]
fn test_large_block_scalar_indent() {
    // https://github.com/Ethiraric/yaml-rust2/issues/29
    // https://github.com/saphyr-rs/saphyr-parser/issues/2
    // Tests the `loop` fallback of `skip_block_scalar_indent`. The indent in the YAML string must
    // be greater than `BUFFER_LEN - 2`. The second line is further indented with spaces, and the
    // resulting string should be "a\n    b".
    let s = "
a: |-
                  a
                      b
";

    assert_eq!(
        snapshot(&run_parser(s).unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::mapping_start(StructureStyle::Block, 0, None),
            ExpectedEvent::new_scalar("a".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::new_scalar(
                "a\n    b".into(),
                {
                    let mut syntax = granit_parser::BlockScalarSyntax::new(
                        granit_parser::Chomping::Strip,
                        None,
                        18,
                    )
                    .unwrap();
                    syntax.header_chars_before_span = 21;
                    syntax.header_bytes_before_span = 21;
                    ExpectedStyle::literal(syntax)
                },
                false,
                0,
                None
            ),
            ExpectedEvent::MappingEnd,
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );
}

#[test]
fn test_bad_docstart() {
    run_parser("---This used to cause an infinite loop").unwrap();
    assert_eq!(
        snapshot(&run_parser("----").unwrap()),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::new_scalar("----".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ]
    );

    assert_eq!(
        snapshot(&run_parser("--- #comment").unwrap()),
        expected_events([
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(true, None),
            #[cfg(feature = "comments")]
            ExpectedEvent::comment("comment".into(), Placement::Right),
            ExpectedEvent::new_scalar("~".into(), ExpectedStyle::plain(), true, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ])
    );

    assert_eq!(
        snapshot(&run_parser("---- #comment").unwrap()),
        expected_events([
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::new_scalar("----".into(), ExpectedStyle::plain(), false, 0, None),
            #[cfg(feature = "comments")]
            ExpectedEvent::comment("comment".into(), Placement::Right),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ])
    );
}

#[test]
fn test_indentation_equality() {
    let four_spaces = run_parser(
        r"
hash:
    with:
        indentations
",
    )
    .unwrap();

    let two_spaces = run_parser(
        r"
hash:
  with:
    indentations
",
    )
    .unwrap();

    let one_space = run_parser(
        r"
hash:
 with:
  indentations
",
    )
    .unwrap();

    let mixed_spaces = run_parser(
        r"
hash:
     with:
               indentations
",
    )
    .unwrap();

    for (((a, b), c), d) in four_spaces
        .iter()
        .zip(two_spaces.iter())
        .zip(one_space.iter())
        .zip(mixed_spaces.iter())
    {
        assert_eq!(snapshot(&a), snapshot(&b));
        assert_eq!(snapshot(&a), snapshot(&c));
        assert_eq!(snapshot(&a), snapshot(&d));
    }
}

#[test]
fn test_recursion_depth_check_objects() {
    let s = "{a:".repeat(10_000) + &"}".repeat(10_000);
    assert!(run_parser(&s).is_err());
}

#[test]
fn test_recursion_depth_check_arrays() {
    let s = "[".repeat(10_000) + &"]".repeat(10_000);
    assert!(run_parser(&s).is_err());
}
