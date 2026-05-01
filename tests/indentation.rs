use granit_parser::{Event, Parser};

fn scalar_value<'a>(ev: &'a Event<'_>) -> Option<&'a str> {
    match ev {
        Event::Scalar(v, ..) => Some(v.as_ref()),
        _ => None,
    }
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
