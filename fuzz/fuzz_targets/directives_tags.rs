#![no_main]

mod common;

use common::parse_with_both_inputs;
use granit_parser::Parser;
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_LEN: usize = 8 * 1024;
const HEX: &[u8; 16] = b"0123456789abcdef";

fn quoted_scalar(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '\0' => quoted.push_str("\\0"),
            '\x07' => quoted.push_str("\\a"),
            '\x08' => quoted.push_str("\\b"),
            '\t' => quoted.push_str("\\t"),
            '\n' => quoted.push_str("\\n"),
            '\x0b' => quoted.push_str("\\v"),
            '\x0c' => quoted.push_str("\\f"),
            '\r' => quoted.push_str("\\r"),
            '\x1b' => quoted.push_str("\\e"),
            '\u{85}' => quoted.push_str("\\N"),
            '\u{2028}' => quoted.push_str("\\L"),
            '\u{2029}' => quoted.push_str("\\P"),
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\u{feff}' | '\u{fffe}' | '\u{ffff}' => quoted.push('_'),
            character if character.is_control() => quoted.push('_'),
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

fn uri_component(value: &str) -> String {
    let mut component = String::with_capacity(value.len() * 2 + 1);
    component.push('x');
    for &byte in value.as_bytes() {
        component.push(char::from(HEX[usize::from(byte >> 4)]));
        component.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    component
}

fn assert_single_tag(
    input: &str,
    expected_handle: &str,
    expected_suffix: &str,
    expected_original_handle: &str,
    expect_alias: bool,
) {
    let events = Parser::new_from_str(input)
        .collect::<Result<Vec<_>, _>>()
        .expect("generated directive and tag YAML must parse");
    let tagged_events = events
        .iter()
        .filter(|(event, _)| event.tag().is_some())
        .collect::<Vec<_>>();

    assert_eq!(tagged_events.len(), 1);
    let tagged = &tagged_events[0].0;
    let tag = tagged.tag().expect("tagged event disappeared");
    assert_eq!(tag.handle(), expected_handle);
    assert_eq!(tag.suffix(), expected_suffix);
    assert_eq!(tag.original_handle(), expected_original_handle);

    let aliases = events
        .iter()
        .filter_map(|(event, _)| event.alias_id())
        .collect::<Vec<_>>();
    if expect_alias {
        let anchor_id = tagged
            .anchor_id()
            .expect("tagged node must also carry the generated anchor");
        assert_eq!(aliases, [anchor_id]);
    } else {
        assert!(aliases.is_empty());
    }
}

// Select one construction per iteration. Valid branches sanitize directive/tag
// components and escape scalar data so tag resolution can be asserted. Raw branches
// preserve malformed directives, percent escapes, node properties, and comments.
fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_LEN {
        return;
    }

    let (selector, payload_bytes): (u8, &[u8]) = match data.split_first() {
        Some((&selector, payload)) => (selector, payload),
        None => (0, &[]),
    };
    let Ok(payload) = core::str::from_utf8(payload_bytes) else {
        return;
    };

    let yaml = match selector % 6 {
        0 => {
            let scalar = quoted_scalar(payload);
            let prefix = format!("tag:example.com,2026:{}", uri_component(payload));
            let yaml =
                format!("%YAML 1.2\n%TAG !e! {prefix}\n---\nkey: !e!item {scalar}\n");
            assert_single_tag(&yaml, &prefix, "item", "!e!", false);
            yaml
        }
        1 => {
            let scalar = quoted_scalar(payload);
            let uri = format!("tag:example.com,2026:{}", uri_component(payload));
            let yaml = format!("---\n!<{uri}> {scalar}\n");
            assert_single_tag(&yaml, "", &uri, "", false);
            yaml
        }
        2 => {
            let scalar = quoted_scalar(payload);
            let suffix = format!("local-{}", uri_component(payload));
            let yaml = format!(
                "---\nnode:\n  &anchor !{suffix}\n  # generated\n  {scalar}\nalias: *anchor\n"
            );
            assert_single_tag(&yaml, "!", &suffix, "!", true);
            yaml
        }
        3 => format!(
            "%YAML 1.2\n%TAG !e! tag:example.com,2026:{payload}\n---\nkey: !e!item {payload}\n# {payload}\n"
        ),
        4 => format!("%FOO {payload}\n---\n!<tag:example.com,2026:{payload}> value\n"),
        _ => format!(
            "---\nnode:\n  &anchor !local{payload}\n  # {payload}\n  value\nalias: *anchor\n"
        ),
    };

    parse_with_both_inputs(&yaml);
});
