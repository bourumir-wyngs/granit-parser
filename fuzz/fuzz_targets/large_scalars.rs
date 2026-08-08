#![no_main]

mod common;

use common::parse_with_both_inputs;
use libfuzzer_sys::fuzz_target;

const SIZE_BUCKETS: [usize; 5] = [64, 1 << 10, 16 << 10, 64 << 10, 256 << 10];
const MAX_SEED_BYTES: usize = 16 << 10;

// Stress scalar scanning at several useful sizes. The first two bytes select a
// construction and a size bucket, so even tiny inputs exercise the parser and
// only a fraction of iterations pay for the largest allocation.
fuzz_target!(|data: &[u8]| {
    let mode = data.first().copied().unwrap_or(0);
    let bucket = data.get(1).copied().unwrap_or(0);
    let seed = data.get(2..).unwrap_or_default();
    let seed = &seed[..seed.len().min(MAX_SEED_BYTES)];
    let target_len = SIZE_BUCKETS[usize::from(bucket) % SIZE_BUCKETS.len()];

    let yaml = match mode % 8 {
        // Hex is deliberately boring YAML content: these modes remain valid
        // regardless of the input bytes and can reach deep scalar scan paths.
        0 => {
            let scalar = expanded_scalar(&hex_seed(seed), target_len);
            format!("value: {scalar}\n")
        }
        1 => {
            let scalar = expanded_scalar(&hex_seed(seed), target_len);
            format!("value: \"{scalar}\"\n")
        }
        2 | 3 => {
            let scalar = expanded_scalar(&hex_seed(seed), target_len);
            let line_widths = [16, 64, 256, 1024];
            let width = line_widths[usize::from(mode >> 3) % line_widths.len()];
            let wrapped = wrap_ascii_lines(&scalar, width);
            let header = if mode % 8 == 2 { "|" } else { ">-" };
            block_document(header, &wrapped)
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
});

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
    let mut scalar = String::with_capacity(target_len);

    while scalar.len() < target_len {
        let remaining = target_len - scalar.len();
        if seed.len() <= remaining {
            scalar.push_str(seed);
            continue;
        }

        let mut end = remaining;
        while !seed.is_char_boundary(end) {
            end -= 1;
        }
        scalar.push_str(&seed[..end]);
        break;
    }

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
    let mut yaml = String::with_capacity(header.len() + scalar.len() * 3 + 10);
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
