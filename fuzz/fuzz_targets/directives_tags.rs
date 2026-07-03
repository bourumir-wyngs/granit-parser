#![no_main]

mod common;

use common::parse_with_both_inputs;
use libfuzzer_sys::fuzz_target;

// Bias inputs toward directives, tag resolution, comments, and node properties.
// Invalid YAML is acceptable; parser panics are findings.
fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 {
        return;
    }

    let s = String::from_utf8_lossy(data);

    let yaml_directives =
        format!("%YAML 1.2\n%TAG !e! tag:example.com,2026:{s}\n---\nkey: !e!item {s}\n# {s}\n");
    let yaml_reserved = format!("%FOO {s}\n---\n!<tag:example.com,2026:{s}> value\n");
    let yaml_properties = format!("---\n&a !local{s}\n# {s}\nvalue\nalias: *a\n");

    for yaml in [&yaml_directives, &yaml_reserved, &yaml_properties] {
        parse_with_both_inputs(yaml);
    }
});
