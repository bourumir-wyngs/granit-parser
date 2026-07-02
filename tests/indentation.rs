use granit_parser::{Event, Parser, ScalarStyle, StructureStyle};

fn scalar_value<'a>(ev: &'a Event<'_>) -> Option<&'a str> {
    match ev {
        Event::Scalar(v, ..) => Some(v.as_ref()),
        _ => None,
    }
}

fn block_scalar_indents(yaml: &str) -> Vec<(String, Option<usize>)> {
    Parser::new_from_str(yaml)
        .map(|event| event.expect("valid yaml"))
        .filter_map(|(event, span)| match event {
            Event::Scalar(value, ScalarStyle::Literal | ScalarStyle::Folded, ..) => {
                Some((value.into_owned(), span.indent))
            }
            _ => None,
        })
        .collect()
}

fn first_error_info(yaml: &str) -> Option<String> {
    Parser::new_from_str(yaml).find_map(|event| event.err().map(|err| err.info().to_owned()))
}

#[test]
fn indentation_is_reported_for_block_mapping_keys_only() {
    let yaml = "a: b\n";

    let mut scalars = Vec::new();
    for x in Parser::new_from_str(yaml) {
        let (ev, span) = x.expect("valid yaml");
        if let Some(v) = scalar_value(&ev) {
            scalars.push((v.to_string(), span.indent));
        }
    }

    // In a mapping, the first scalar is the key and must carry indentation (col=0).
    // The value must not carry indentation.
    assert!(scalars.contains(&("a".to_string(), Some(0))));
    assert!(scalars.contains(&("b".to_string(), None)));
}

#[test]
fn indentation_is_not_reported_in_flow_mappings() {
    let yaml = "{ a: b }\n";

    for x in Parser::new_from_str(yaml) {
        let (ev, span) = x.expect("valid yaml");
        if let Some(v) = scalar_value(&ev) {
            if v == "a" || v == "b" {
                assert_eq!(span.indent, None);
            }
        }
    }
}

#[test]
fn indentation_is_reported_for_nested_block_mapping_keys() {
    let yaml = "a:\n  b: c\n";

    let mut a_indent = None;
    let mut b_indent = None;
    let mut c_indent = None;

    for x in Parser::new_from_str(yaml) {
        let (ev, span) = x.expect("valid yaml");
        if let Some(v) = scalar_value(&ev) {
            match v {
                "a" => a_indent = span.indent,
                "b" => b_indent = span.indent,
                "c" => c_indent = span.indent,
                _ => {}
            }
        }
    }

    assert_eq!(a_indent, Some(0));
    assert_eq!(b_indent, Some(2));
    assert_eq!(c_indent, None);
}

#[test]
fn queued_key_node_after_comment_keeps_key_indent() {
    let yaml = "? - # key sequence comment\n    item\n: value\n";

    let mut key_sequence_indent = None;

    for next in Parser::new_from_str(yaml) {
        let (event, span) = next.expect("valid yaml");
        if matches!(event, Event::SequenceStart(..)) {
            key_sequence_indent = span.indent;
            break;
        }
    }

    assert_eq!(key_sequence_indent, Some(0));
}

#[test]
fn indentation_is_reported_for_block_scalar_content() {
    let yaml = "key: |\n  body\n";

    assert_eq!(
        block_scalar_indents(yaml),
        vec![("body\n".to_string(), Some(2))]
    );
}

#[test]
fn indentation_is_not_reported_for_whitespace_only_block_scalar_content() {
    let yaml = "key: |+\n  \n";

    assert_eq!(block_scalar_indents(yaml), vec![("\n".to_string(), None)]);
}

#[test]
fn root_block_sequence_can_have_anchor_on_previous_line() {
    let yaml = "&anchor\n- a\n- b\n";
    let events = Parser::new_from_str(yaml)
        .map(|event| event.expect("valid yaml").0)
        .collect::<Vec<_>>();

    assert!(events
        .iter()
        .any(|event| matches!(event, Event::SequenceStart(StructureStyle::Block, 1, None))));
}

#[test]
fn indented_mapping_value_sequence_can_have_anchor_and_comment_on_previous_lines() {
    let yaml = "seq:\n  &anchor\n  # c\n  - a\n  - b\n";
    let events = Parser::new_from_str(yaml)
        .map(|event| event.expect("valid yaml").0)
        .collect::<Vec<_>>();

    assert!(events
        .iter()
        .any(|event| matches!(event, Event::SequenceStart(StructureStyle::Block, 1, None))));
}

#[test]
fn unindented_mapping_value_sequence_after_anchor_is_rejected() {
    assert_eq!(
        first_error_info("seq:\n&anchor\n- a\n- b\n").as_deref(),
        Some("simple key expect ':'")
    );
}

#[test]
fn unindented_mapping_value_sequence_after_anchor_comment_is_rejected() {
    assert_eq!(
        first_error_info("seq:\n&anchor\n# c\n- a\n- b\n").as_deref(),
        Some("simple key expect ':'")
    );
}

#[test]
fn unindented_mapping_value_sequence_after_tag_comment_is_rejected() {
    assert_eq!(
        first_error_info("seq:\n!tag\n# c\n- a\n- b\n").as_deref(),
        Some("simple key expect ':'")
    );
}
