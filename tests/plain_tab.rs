use granit_parser::{ErrorKind, Event, Parser, ScalarStyle, ScanError, StructureStyle};

fn collect_events(input: &str) -> Result<Vec<Event<'_>>, ScanError> {
    let str_events = Parser::new_from_str(input).collect::<Result<Vec<_>, _>>();
    let iter_events = Parser::new_from_iter(input.chars()).collect::<Result<Vec<_>, _>>();
    assert_eq!(str_events, iter_events, "input: {input:?}");
    Ok(str_events?.into_iter().map(|(event, _)| event).collect())
}

fn collect_scalars(input: &str) -> Result<Vec<String>, ScanError> {
    Ok(collect_events(input)?
        .into_iter()
        .filter_map(|event| match event {
            Event::Scalar(value, ..) => Some(value.into_owned()),
            _ => None,
        })
        .collect())
}

#[test]
fn tabs_separate_mapping_values_in_block_and_flow_contexts() {
    for value in [
        "1", "-1", "true", "false", "null", "value", "-item", "_value", "äöü",
    ] {
        for separator in ["\t", "\t\t", "\t ", " \t"] {
            for (yaml, mapping_style, key_style) in [
                (
                    format!("{{\"key\":{separator}{value}}}"),
                    StructureStyle::Flow,
                    ScalarStyle::DoubleQuoted,
                ),
                (
                    format!("key:{separator}{value}\n"),
                    StructureStyle::Block,
                    ScalarStyle::Plain,
                ),
                (
                    format!("? key\n:{separator}{value}\n"),
                    StructureStyle::Block,
                    ScalarStyle::Plain,
                ),
            ] {
                assert_eq!(
                    collect_events(&yaml).unwrap(),
                    [
                        Event::StreamStart,
                        Event::DocumentStart(false, None),
                        Event::MappingStart(mapping_style, 0, None),
                        Event::Scalar("key".into(), key_style, 0, None),
                        Event::Scalar(value.into(), ScalarStyle::Plain, 0, None),
                        Event::MappingEnd,
                        Event::DocumentEnd,
                        Event::StreamEnd,
                    ],
                    "input: {yaml:?}",
                );
            }
        }
    }
}

#[test]
fn tabs_after_colons_preserve_value_and_comment_parsing() {
    for (yaml, expected) in [
        ("key:\t\"value\"\n", "value"),
        ("key:\t|\n  value\n", "value\n"),
        ("key:\t[value]\n", "value"),
        ("[key:\tvalue]\n", "value"),
        ("? key\n:\tvalue\n", "value"),
        ("? key\n:\t[value]\n", "value"),
        ("  key:\tvalue\n", "value"),
        ("key:\tvalue # comment\n", "value"),
        ("key:\t# comment\n  value\n", "value"),
        ("key:\t\n  value\n", "value"),
        ("{key:\t# comment\nvalue}\n", "value"),
    ] {
        assert_eq!(
            collect_scalars(yaml).unwrap(),
            ["key", expected],
            "input: {yaml:?}",
        );
    }
}

#[test]
fn tabs_after_colons_preserve_empty_values_and_following_entries() {
    for yaml in [
        "key:\t",
        "key:\t\n",
        "{key:\t}",
        "[key:\t]",
        "key:\t# empty\n",
        "{key:\t# empty\n}",
        "[key:\t# empty\n]",
    ] {
        assert_eq!(
            collect_scalars(yaml).unwrap(),
            ["key", "~"],
            "input: {yaml:?}"
        );
    }

    for yaml in [
        "key:\t# empty\nnext:\tvalue\n",
        "{key:\t, next:\tvalue}",
        "{key:\t# empty\n, next:\tvalue}",
        "[key:\t, next:\tvalue]",
    ] {
        assert_eq!(
            collect_scalars(yaml).unwrap(),
            ["key", "~", "next", "value"],
            "input: {yaml:?}",
        );
    }
}

#[test]
fn tabs_after_colons_preserve_tagged_anchors_and_aliases() {
    for (yaml, style) in [
        ("key:\t&a !local value\ncopy:\t*a\n", StructureStyle::Block),
        ("{key:\t&a !local value, copy:\t*a}", StructureStyle::Flow),
    ] {
        assert_eq!(
            collect_events(yaml).unwrap(),
            [
                Event::StreamStart,
                Event::DocumentStart(false, None),
                Event::MappingStart(style, 0, None),
                Event::Scalar("key".into(), ScalarStyle::Plain, 0, None),
                Event::Scalar(
                    "value".into(),
                    ScalarStyle::Plain,
                    1,
                    Some(std::borrow::Cow::Owned(granit_parser::Tag::new(
                        "!", "local"
                    ))),
                ),
                Event::Scalar("copy".into(), ScalarStyle::Plain, 0, None),
                Event::Alias(1),
                Event::MappingEnd,
                Event::DocumentEnd,
                Event::StreamEnd,
            ],
            "input: {yaml:?}",
        );
    }
}

#[test]
fn tabs_after_colons_do_not_allow_tab_indentation() {
    for yaml in [
        "key:\n\tvalue\n",
        "key:\t\n\tvalue\n",
        "key:\t# comment\n\tvalue\n",
    ] {
        assert_eq!(
            collect_scalars(yaml).unwrap_err().kind(),
            &ErrorKind::TabInBlockIndentation,
            "input: {yaml:?}",
        );
    }
}

#[test]
fn tabs_after_colons_cannot_indent_compact_block_collections() {
    for (value, expected) in [
        ("- value", vec!["key", "value"]),
        ("child: value", vec!["key", "child", "value"]),
        ("\"child\": value", vec!["key", "child", "value"]),
    ] {
        let yaml = format!("? key\n: {value}\n");
        assert_eq!(collect_scalars(&yaml).unwrap(), expected);

        for separator in ["\t", "\t\t", "\t ", " \t"] {
            let yaml = format!("? key\n:{separator}{value}\n");
            assert!(collect_scalars(&yaml).is_err(), "input: {yaml:?}");

            // A line break permits a block collection with its own space indentation.
            let yaml = format!("? key\n:{separator}# comment\n  {value}\n");
            assert_eq!(collect_scalars(&yaml).unwrap(), expected, "input: {yaml:?}");
        }
    }
}

#[test]
fn tab_in_block_literal_body_is_allowed() {
    // A tab in the body of a literal block scalar (|) should be accepted.
    let yaml = "key: |\n  a\tb"; // 'a\tb' inside the block content
    let scalars = collect_scalars(yaml).expect("parser should accept tab inside block scalar body");
    // Literal style preserves newlines; a single content line ends with a trailing \n
    assert_eq!(scalars, vec!["key".to_string(), "a\tb\n".to_string()]);
}

#[test]
fn tab_in_block_folded_body_is_allowed() {
    // A tab in the body of a folded block scalar (>) should be accepted as content.
    let yaml = "key: >\n  a\tb";
    let scalars =
        collect_scalars(yaml).expect("parser should accept tab inside folded block scalar body");
    // For a single content line, folded and literal both end with a trailing \n
    assert_eq!(scalars, vec!["key".to_string(), "a\tb\n".to_string()]);
}

#[test]
fn tab_at_start_of_block_scalar_is_rejected() {
    // If the first content character of the block scalar is a tab, it must be rejected.
    // This means the content line starts with a tab instead of spaces for indentation.
    let yaml = "key: |\n\tvalue";

    let mut got_err: Option<ScanError> = None;
    for item in Parser::new_from_str(yaml) {
        match item {
            Ok(_) => {}
            Err(e) => {
                got_err = Some(e);
                break;
            }
        }
    }

    let err =
        got_err.expect("expected a ScanError due to leading tab at start of block scalar content");
    // The scanner has a specific error for this case.
    assert!(
        err.info()
            .contains("a block scalar content cannot start with a tab"),
        "unexpected error message: {}",
        err.info()
    );
}

#[test]
fn explicit_indent_root_literal_can_end_before_document_start() {
    let yaml = "|2\n---\n";
    let scalars =
        collect_scalars(yaml).expect("parser should accept empty explicit-indented block scalar");
    assert_eq!(scalars.first().map(String::as_str), Some(""));
}

#[test]
fn explicit_indent_root_literal_can_end_before_comment() {
    let yaml = "|2\n# comment\n";
    let scalars = collect_scalars(yaml)
        .expect("parser should accept empty explicit-indented block scalar before comment");
    assert_eq!(scalars, vec![String::new()]);
}
