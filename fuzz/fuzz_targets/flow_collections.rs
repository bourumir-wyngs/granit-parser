#![no_main]

mod common;

use common::parse_with_both_inputs;
use granit_parser::Parser;
use libfuzzer_sys::fuzz_target;

const MAX_PAYLOAD_BYTES: usize = 16 << 10;
const MAX_NESTING_DEPTH: usize = 32;

// Select one construction per iteration so coverage feedback is attributable
// to that construction and executions stay cheap. Valid, raw, malformed, and
// nested flow inputs all remain reachable from short mutations.
fuzz_target!(|data: &[u8]| {
    let mode = data.first().copied().unwrap_or(0);
    let shape = data.get(1).copied().unwrap_or(0);
    let payload = data.get(2..).unwrap_or_default();
    let payload = cap_at_utf8_boundary(payload, MAX_PAYLOAD_BYTES);

    let (yaml, must_be_valid) = match mode % 6 {
        0 => (valid_sequence(payload), true),
        1 => (valid_mapping(payload), true),
        2 => (valid_nested(payload, shape), true),
        3 => {
            let raw = byte_preserving_text(payload);
            (format!("[{raw}]"), false)
        }
        4 => {
            let raw = byte_preserving_text(payload);
            (format!("{{key: {raw}, broken"), false)
        }
        _ => (malformed_nested(payload, shape), false),
    };

    if must_be_valid {
        Parser::new_from_str(&yaml)
            .collect::<Result<Vec<_>, _>>()
            .expect("generated flow collection must parse");
    }
    parse_with_both_inputs(&yaml);
});

fn valid_sequence(payload: &[u8]) -> String {
    let mut yaml = String::with_capacity(payload.len() * 5 + 2);
    yaml.push('[');
    for (index, &byte) in payload.iter().enumerate() {
        if index != 0 {
            yaml.push(',');
        }
        push_quoted_byte(&mut yaml, byte);
    }
    yaml.push(']');
    yaml
}

fn valid_mapping(payload: &[u8]) -> String {
    let pair_count = payload.len().div_ceil(2);
    let mut yaml = String::with_capacity(pair_count * 10 + 2);
    yaml.push('{');
    for (index, pair) in payload.chunks(2).enumerate() {
        if index != 0 {
            yaml.push(',');
        }
        push_quoted_byte(&mut yaml, pair[0]);
        yaml.push(':');
        push_quoted_byte(&mut yaml, pair.get(1).copied().unwrap_or(0));
    }
    yaml.push('}');
    yaml
}

fn valid_nested(payload: &[u8], shape: u8) -> String {
    let depth = 1 + usize::from(shape) % MAX_NESTING_DEPTH;
    let mut yaml = String::with_capacity(payload.len() * 2 + depth * 6 + 2);

    for level in 0..depth {
        if nesting_is_mapping(shape, level) {
            yaml.push_str("{k: ");
        } else {
            yaml.push('[');
        }
    }

    push_quoted_bytes(&mut yaml, payload);

    for level in (0..depth).rev() {
        yaml.push(if nesting_is_mapping(shape, level) {
            '}'
        } else {
            ']'
        });
    }
    yaml
}

fn malformed_nested(payload: &[u8], shape: u8) -> String {
    let depth = 1 + usize::from(shape) % MAX_NESTING_DEPTH;
    let mut yaml = String::with_capacity(payload.len() * 2 + depth * 4 + 2);

    for level in 0..depth {
        yaml.push(if nesting_is_mapping(shape, level) {
            '{'
        } else {
            '['
        });
    }
    yaml.push_str(&byte_preserving_text(payload));

    // Close only part of the stack, using the wrong delimiter for each level.
    for level in (depth / 2..depth).rev() {
        yaml.push(if nesting_is_mapping(shape, level) {
            ']'
        } else {
            '}'
        });
    }
    yaml
}

fn nesting_is_mapping(shape: u8, level: usize) -> bool {
    (usize::from(shape.rotate_right((level % 8) as u32)) + level) & 1 == 0
}

fn cap_at_utf8_boundary(data: &[u8], max_len: usize) -> &[u8] {
    if data.len() <= max_len {
        return data;
    }

    let mut end = max_len;
    while end > 0 && data[end] & 0xc0 == 0x80 {
        end -= 1;
    }
    &data[..end]
}

fn push_quoted_byte(yaml: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    yaml.push('"');
    yaml.push(char::from(HEX[usize::from(byte >> 4)]));
    yaml.push(char::from(HEX[usize::from(byte & 0x0f)]));
    yaml.push('"');
}

fn push_quoted_bytes(yaml: &mut String, payload: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    yaml.push('"');
    for &byte in payload {
        yaml.push(char::from(HEX[usize::from(byte >> 4)]));
        yaml.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    yaml.push('"');
}

fn byte_preserving_text(payload: &[u8]) -> String {
    if let Ok(text) = str::from_utf8(payload) {
        text.to_owned()
    } else {
        payload.iter().copied().map(char::from).collect()
    }
}
