use granit_parser::{options, Event, Options, Parser, ScanError};

/// Parse `yaml` with every parser input backend and return the last scalar value.
fn last_scalar(yaml: &str, options: Options) -> [(&'static str, Result<String, ScanError>); 3] {
    fn collect<'a>(
        events: impl Iterator<Item = Result<(Event<'a>, granit_parser::Span), ScanError>>,
    ) -> Result<String, ScanError> {
        let mut last = None;
        for event in events {
            if let (Event::Scalar { value, .. }, _) = event? {
                last = Some(value.into_owned());
            }
        }
        Ok(last.expect("a scalar"))
    }

    [
        (
            "string parser",
            collect(Parser::new_from_str_with_options(yaml, options.clone())),
        ),
        (
            "iterator parser",
            collect(Parser::new_from_iter_with_options(
                yaml.chars(),
                options.clone(),
            )),
        ),
        (
            "fallible parser",
            collect(Parser::new_from_fallible_iter_with_options(
                yaml.chars().map(Ok),
                options,
            )),
        ),
    ]
}

/// Input, value by default (YAML test suite), value with the option (libyaml, `PyYAML`).
const CASES: [(&str, &str, &str); 20] = [
    ("key: |\n  text", "text\n", "text"),
    ("key: >\n  text", "text\n", "text"),
    ("key: |+\n  text", "text\n", "text"),
    ("key: |-\n  text", "text", "text"),
    ("key: |\n  one\n\n  two", "one\n\ntwo\n", "one\n\ntwo"),
    ("key: >\n  one\n  two", "one two\n", "one two"),
    ("key: |2\n   text", " text\n", " text"),
    ("- |\n  text", "text\n", "text"),
    // yaml-test-suite L24T, second case.
    ("foo: |\n  x\n   ", "x\n \n", "x\n "),
    ("key: |\n  text\n", "text\n", "text\n"),
    // Whitespace-only final scalars use the scanner's early EOF return.
    ("key: |+\n  ", "\n", ""),
    ("key: >+\n  ", "\n", ""),
    ("key: |2+\n  ", "\n", ""),
    ("key: >2+\n  ", "\n", ""),
    // The header break is never scalar content.
    ("key: |+\n", "", ""),
    ("key: >+\n", "", ""),
    // Keep actual empty lines, including before an unterminated indentation-only tail.
    ("key: |+\n  \n", "\n", "\n"),
    ("key: >+\n  \n", "\n", "\n"),
    ("key: |+\n  \n  ", "\n", "\n"),
    ("key: >+\n  \n  ", "\n", "\n"),
];

#[test]
fn block_scalar_at_eof_gets_a_line_feed_by_default() {
    assert!(!Options::default().block_scalar_eof_without_line_feed);
    for (yaml, expected, _) in CASES {
        for (input, value) in last_scalar(yaml, Options::default()) {
            assert_eq!(value.unwrap(), expected, "{input}: {yaml:?}");
        }
    }
}

#[test]
fn block_scalar_at_eof_gets_no_line_feed_when_enabled() {
    for strict_indentation in [false, true] {
        let options = options! {
            block_scalar_eof_without_line_feed: true,
            strict_indentation: strict_indentation,
        };
        for (yaml, _, expected) in CASES {
            for (input, value) in last_scalar(yaml, options.clone()) {
                assert_eq!(value.unwrap(), expected, "{input}: {yaml:?}");
            }
        }
    }
}
