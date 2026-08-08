#![no_main]

mod common;

use common::parse_with_both_inputs;
use granit_parser::Parser;
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_LEN: usize = 8 * 1024;

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

fn assert_duplicate_scalar_key_is_preserved(input: &str) {
    let events = Parser::new_from_str(input)
        .collect::<Result<Vec<_>, _>>()
        .expect("generated duplicate-key YAML must parse");
    let scalars = events
        .iter()
        .filter_map(|(event, _)| event.scalar().map(|(value, _)| value))
        .collect::<Vec<_>>();

    // Both valid constructions contain exactly two scalar key/value pairs.
    assert_eq!(scalars.len(), 4);
    assert_eq!(scalars[0], scalars[2]);
}

// The parser is event-based and intentionally preserves duplicate mapping keys.
// Select one construction per iteration: valid escaped keys exercise successful
// block/flow parsing, while raw cases retain malformed and nested syntax coverage.
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

    let yaml = match selector % 5 {
        0 => {
            let key = quoted_scalar(payload);
            let yaml = format!("{key}: first\n{key}: second\n");
            assert_duplicate_scalar_key_is_preserved(&yaml);
            yaml
        }
        1 => {
            let key = quoted_scalar(payload);
            let yaml = format!("{{{key}: first, {key}: second}}\n");
            assert_duplicate_scalar_key_is_preserved(&yaml);
            yaml
        }
        2 => format!("a: 1\na: 2\nkey: {payload}\nkey: {payload}\n"),
        3 => format!(
            "outer:\n  inner: {{x: 1, x: 2}}\n  arr: [{{k: {payload}}}, {{k: {payload}}}]\n"
        ),
        _ => format!("{{'{payload}': 1, '{payload}': 2}}\n"),
    };

    parse_with_both_inputs(&yaml);
});
