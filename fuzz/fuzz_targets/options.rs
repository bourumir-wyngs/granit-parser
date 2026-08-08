#![no_main]

mod common;

use std::str;

use common::{check_comment_suppression, parse_with_options, scan_with_options};
use granit_parser::Options;
use libfuzzer_sys::fuzz_target;

const COMMENT_LIMITS: &[usize] = &[0, 1, 31, 32, 33, 95, 96, 97, 255];
const KEY_LIMITS: &[usize] = &[0, 1, 15, 16, 17, 127, 128, 129, 1023, 1024, 1025, 4096];
const FLOW_LIMITS: &[usize] = &[0, 1, 2, 254, 255, 256, 512];

// Exercise option boundaries independently of the grammar-specific targets. Bytes before the
// UTF-8 payload choose the oracle and values around meaningful resource-limit boundaries.
fuzz_target!(|data: &[u8]| {
    let [mode, flags, comment, key, flow, input @ ..] = data else {
        return;
    };
    if input.len() > 64 * 1024 {
        return;
    }
    let Ok(input) = str::from_utf8(input) else {
        return;
    };

    if mode % 3 == 2 {
        check_comment_suppression(input);
        return;
    }

    let mut options = Options::default();
    options.emit_comments = flags & 1 != 0;
    options.max_buffered_comment_events = COMMENT_LIMITS[*comment as usize % COMMENT_LIMITS.len()];
    options.simple_key_max_lookahead = KEY_LIMITS[*key as usize % KEY_LIMITS.len()];
    options.flow_nesting_limit = FLOW_LIMITS[*flow as usize % FLOW_LIMITS.len()];

    if mode % 3 == 0 {
        parse_with_options(input, options);
    } else {
        scan_with_options(input, options);
    }
});
