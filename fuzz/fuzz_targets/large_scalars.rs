#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
mod common;
#[cfg(test)]
use crate::common;

use std::str;

use common::{
    parse_with_both_inputs,
    snapshots::{snapshot, ExpectedEvent, ExpectedStyle},
};
use granit_parser::{Parser, ScalarKind};
#[cfg(not(test))]
use libfuzzer_sys::fuzz_target;

const SIZE_BUCKETS: [usize; 5] = [64, 1 << 10, 16 << 10, 64 << 10, 256 << 10];
const MAX_SEED_BYTES: usize = 16 << 10;

// Stress scalar scanning at several useful sizes. The first two bytes select a
// construction and a size bucket, so even tiny inputs exercise the parser and
// only a fraction of iterations pay for the largest allocation.
#[cfg(not(test))]
fuzz_target!(|data: &[u8]| check_input(data));

pub fn check_input(data: &[u8]) {
    let mode = data.first().copied().unwrap_or(0);
    let bucket = data.get(1).copied().unwrap_or(0);
    let seed = data.get(2..).unwrap_or_default();
    let seed = cap_at_utf8_boundary(seed, MAX_SEED_BYTES);
    let target_len = SIZE_BUCKETS[usize::from(bucket) % SIZE_BUCKETS.len()];

    if mode == 254 {
        let yaml = root_block_document(seed, bucket, target_len);
        parse_with_both_inputs(&yaml);
        return;
    }

    let yaml = match mode % 8 {
        // Hex is deliberately boring YAML content: these modes remain valid
        // regardless of the input bytes and can reach deep scalar scan paths.
        0 => {
            let scalar = expanded_scalar(&hex_seed(seed), target_len);
            let yaml = format!("value: {scalar}\n");
            assert_generated_scalar(&yaml, ScalarKind::Plain, &scalar);
            yaml
        }
        1 => {
            let scalar = expanded_scalar(&hex_seed(seed), target_len);
            let yaml = format!("value: \"{scalar}\"\n");
            assert_generated_scalar(&yaml, ScalarKind::DoubleQuoted, &scalar);
            yaml
        }
        2 | 3 => {
            let scalar = expanded_scalar(&hex_seed(seed), target_len);
            let line_widths = [16, 64, 256, 1024];
            let width = line_widths[usize::from(mode >> 3) % line_widths.len()];
            let wrapped = wrap_ascii_lines(&scalar, width);
            let header = if mode % 8 == 2 { "|" } else { ">-" };
            let yaml = block_document(header, &wrapped);
            let style = if mode % 8 == 2 {
                ScalarKind::Literal
            } else {
                ScalarKind::Folded
            };
            let expected = if matches!(style, ScalarKind::Literal) {
                wrapped
            } else {
                wrapped.lines().collect::<Vec<_>>().join(" ")
            };
            assert_generated_scalar(&yaml, style, &expected);
            yaml
        }

        // These modes retain valid UTF-8 verbatim and map otherwise-invalid
        // bytes one-to-one to Unicode code points. They intentionally permit
        // invalid YAML without collapsing byte sequences to U+FFFD.
        4 => {
            let scalar = expanded_scalar(&byte_preserving_seed(seed), target_len);
            format!("{scalar}\n")
        }
        5 => {
            let scalar = expanded_scalar(&byte_preserving_seed(seed), target_len);
            format!("value: {scalar}\n")
        }
        6 => {
            let scalar = expanded_scalar(&byte_preserving_seed(seed), target_len);
            block_document("|+", &scalar)
        }
        _ => {
            let scalar = expanded_scalar(&byte_preserving_seed(seed), target_len);
            format!("value: \"{scalar}\n")
        }
    };

    parse_with_both_inputs(&yaml);
}

fn root_block_document(seed: &[u8], bucket: u8, target_len: usize) -> String {
    let variant = seed.first().copied().unwrap_or(0);
    let scalar = expanded_scalar(&hex_seed(seed.get(1..).unwrap_or_default()), target_len);
    let width = [16, 64, 256, 1024][usize::from(bucket / 5) % 4];
    let mut wrapped = wrap_ascii_lines(&scalar, width);
    // A non-ASCII prefix makes byte-offset assertions distinct from character offsets.
    wrapped.insert(0, 'λ');
    let folded = variant & 1 != 0;
    let (indicator, style) = if folded {
        ('>', ScalarKind::Folded)
    } else {
        ('|', ScalarKind::Literal)
    };
    let chomp = ["", "-", "+"][usize::from((variant >> 1) & 3) % 3];
    let newline = ["\n", "\r\n", "\r"][usize::from((variant >> 3) & 3) % 3];
    let leading = usize::from((variant >> 5) & 1);
    let trailing = usize::from(variant >> 6);
    let mut expected = "\n".repeat(leading);
    if folded {
        expected.push_str(&wrapped.lines().collect::<Vec<_>>().join(" "));
    } else {
        expected.push_str(wrapped.trim_end_matches('\n'));
    }
    expected.push_str(&"\n".repeat(match chomp {
        "-" => 0,
        "+" => 1 + trailing,
        _ => 1,
    }));

    let prefix = format!(
        "{indicator}{chomp}{newline}{}{}{}",
        newline.repeat(leading),
        wrapped.replace('\n', newline),
        newline.repeat(trailing),
    );
    let separator = [newline, " ", "\t"][usize::from(bucket / 20) % 3];
    let yaml = format!("{prefix}---{separator}second{newline}");
    let events = Parser::new_from_str(&yaml)
        .collect::<Result<Vec<_>, _>>()
        .expect("zero-indented root block scalar must preserve the next document");
    let (_, actual_style) = events[2]
        .0
        .scalar()
        .expect("root block document must contain its scalar");
    assert_eq!(actual_style.kind(), style);
    assert_eq!(
        events
            .iter()
            .map(|(event, _)| snapshot(event))
            .collect::<Vec<_>>(),
        [
            ExpectedEvent::StreamStart,
            ExpectedEvent::document_start(false, None),
            ExpectedEvent::new_scalar(expected.into(), actual_style, false, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::document_start(true, None),
            ExpectedEvent::new_scalar("second".into(), ExpectedStyle::plain(), false, 0, None),
            ExpectedEvent::document_end(false),
            ExpectedEvent::StreamEnd,
        ],
        "root block scalar folding, chomping, or document boundary changed",
    );
    assert_eq!(events[2].1.indent, Some(0));
    assert_eq!(events[2].1.end.index(), prefix.chars().count());
    assert_eq!(events[2].1.end.byte_offset(), Some(prefix.len()));
    assert_eq!(events[2].1.end, events[4].1.start);
    assert_eq!(events[4].1.slice(&yaml), Some("---"));
    yaml
}

fn hex_seed(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    if data.is_empty() {
        return String::from("a");
    }

    let mut encoded = String::with_capacity(data.len() * 2);
    for &byte in data {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn assert_generated_scalar(input: &str, expected_style: ScalarKind, expected_value: &str) {
    let events = Parser::new_from_str(input)
        .collect::<Result<Vec<_>, _>>()
        .expect("generated scalar document must parse");
    let (value, style) = events
        .iter()
        .filter_map(|(event, _)| event.scalar())
        .next_back()
        .expect("generated document must emit its scalar value");

    assert_eq!(style.kind(), expected_style);
    assert_eq!(value, expected_value);
}

fn byte_preserving_seed(data: &[u8]) -> String {
    if data.is_empty() {
        String::from("a")
    } else if let Ok(text) = str::from_utf8(data) {
        text.to_owned()
    } else {
        data.iter().copied().map(char::from).collect()
    }
}

fn expanded_scalar(seed: &str, target_len: usize) -> String {
    let repeats = target_len.div_ceil(seed.len());
    let mut scalar = seed.repeat(repeats);
    let mut end = target_len.min(scalar.len());
    while !scalar.is_char_boundary(end) {
        end -= 1;
    }
    scalar.truncate(end);
    scalar
}

fn wrap_ascii_lines(scalar: &str, width: usize) -> String {
    debug_assert!(scalar.is_ascii());

    let line_count = scalar.len().div_ceil(width);
    let mut wrapped = String::with_capacity(scalar.len() + line_count);
    for line in scalar.as_bytes().chunks(width) {
        // `scalar` is hex-only, so byte chunks always lie on UTF-8 boundaries.
        wrapped.push_str(str::from_utf8(line).expect("hex seed is ASCII"));
        wrapped.push('\n');
    }
    wrapped
}

fn block_document(header: &str, scalar: &str) -> String {
    let line_breaks = scalar
        .chars()
        .filter(|character| matches!(character, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'))
        .count();
    let mut yaml = String::with_capacity(header.len() + scalar.len() + line_breaks * 2 + 10);
    yaml.push_str("value: ");
    yaml.push_str(header);
    yaml.push('\n');
    yaml.push_str("  ");

    let mut chars = scalar.chars().peekable();
    while let Some(ch) = chars.next() {
        yaml.push(ch);

        let line_break = match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    yaml.push(chars.next().expect("peeked LF"));
                }
                true
            }
            '\n' | '\u{85}' | '\u{2028}' | '\u{2029}' => true,
            _ => false,
        };

        if line_break && chars.peek().is_some() {
            yaml.push_str("  ");
        }
    }

    if !matches!(
        scalar.chars().next_back(),
        Some('\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
    ) {
        yaml.push('\n');
    }

    yaml
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
