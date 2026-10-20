use granit_parser::{
    options, BufferedInput, ErrorKind, FallibleBufferedInput, Options, Parser, ScanError, Scanner,
    StrInput,
};

fn first_errors(yaml: &str, options: Options) -> [(&str, Option<ScanError>); 6] {
    [
        (
            "string scanner",
            Scanner::with_options(StrInput::new(yaml), options.clone()).find_map(Result::err),
        ),
        (
            "iterator scanner",
            Scanner::with_options(BufferedInput::new(yaml.chars()), options.clone())
                .find_map(Result::err),
        ),
        (
            "fallible scanner",
            Scanner::with_options(
                FallibleBufferedInput::new(yaml.chars().map(Ok)),
                options.clone(),
            )
            .find_map(Result::err),
        ),
        (
            "string parser",
            Parser::new_from_str_with_options(yaml, options.clone()).find_map(Result::err),
        ),
        (
            "iterator parser",
            Parser::new_from_iter_with_options(yaml.chars(), options.clone()).find_map(Result::err),
        ),
        (
            "fallible parser",
            Parser::new_from_fallible_iter_with_options(yaml.chars().map(Ok), options)
                .find_map(Result::err),
        ),
    ]
}

fn assert_disallowed(yaml: &str, options: Options) {
    let byte_index = yaml.find('%').expect("input must contain a directive");
    let prefix = &yaml[..byte_index];
    let char_index = prefix.chars().count();
    let line = prefix.chars().filter(|&ch| ch == '\n').count() + 1;

    for (backend, error) in first_errors(yaml, options) {
        let error = error.unwrap_or_else(|| panic!("{backend} accepted {yaml:?}"));
        assert_eq!(
            error.kind(),
            &ErrorKind::DirectiveDisallowed,
            "{backend}: {yaml:?}",
        );
        assert_eq!(error.marker().index(), char_index, "{backend}: {yaml:?}");
        assert_eq!(error.marker().line(), line, "{backend}: {yaml:?}");
        assert_eq!(error.marker().col(), 0, "{backend}: {yaml:?}");
        if backend.starts_with("string") {
            assert_eq!(error.marker().byte_offset(), Some(byte_index));
        }
    }
}

#[test]
fn all_directives_are_disallowed_in_every_document() {
    for directive in [
        "%YAML 1.2",
        "%TAG !e! tag:example.com,2026:",
        "%FOO",
        "%FOO a b",
    ] {
        for prefix in [
            "",
            "\u{feff}",
            "# café 漢字\n\n",
            "\u{feff}# before\r\n\r\n",
            "---\nfirst\n...\n",
            "---\nfirst\n...\n\u{feff}# before next\n\n",
        ] {
            let yaml = format!("{prefix}{directive}\n---\nvalue\n");
            for emit_comments in [false, true] {
                assert_disallowed(
                    &yaml,
                    options! { disallow_directives: true, emit_comments: emit_comments },
                );
            }
        }
    }
}

#[test]
fn prohibition_precedes_directive_validation_and_limits() {
    for yaml in [
        "%",
        "%\n",
        "%YAML nope\n---\nvalue\n",
        "%TAG invalid\n---\nvalue\n",
        "%FOO a b\n---\nvalue\n",
    ] {
        assert_disallowed(
            yaml,
            options! {
                disallow_directives: true,
                max_directive_bytes: 0,
                max_reserved_directive_params: 0,
            },
        );
    }
}

#[test]
fn directives_remain_enabled_by_default_and_explicit_false() {
    for yaml in [
        "%YAML 1.2\n---\nvalue\n",
        "\u{feff}%TAG !e! tag:example.com,2026:\n---\n!e!value text\n",
        "%FOO option\n---\nvalue\n",
        "---\nfirst\n...\n%YAML 1.2\n%FOO option\n---\nsecond\n",
    ] {
        for options in [Options::default(), options! { disallow_directives: false }] {
            for (backend, error) in first_errors(yaml, options) {
                assert_eq!(error, None, "{backend}: {yaml:?}");
            }
        }

        let expected = Parser::new_from_str(yaml)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let explicit_false =
            Parser::new_from_str_with_options(yaml, options! { disallow_directives: false })
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        assert_eq!(explicit_false, expected, "input: {yaml:?}");
    }
}

#[test]
fn percent_in_scalar_content_and_comments_is_unaffected() {
    for yaml in [
        "'\u{feff}%TAG !e! ignored'\n",
        "\"%YAML 1.2\"\n",
        "'before\n%FOO after'\n",
        "\"before\n%TAG after\"\n",
        "plain: before%FOO after\n",
        "plain: before\n  %FOO after\n",
        "# %YAML 1.2\nvalue # %TAG !e! ignored\n# %FOO\n",
        "literal: |\n  %YAML 1.2\n  %TAG !e! ignored\n  %FOO\n",
        "folded: >\n  %YAML 1.2\n  %TAG !e! ignored\n  %FOO\n",
    ] {
        for emit_comments in [false, true] {
            let enabled = options! { disallow_directives: true, emit_comments: emit_comments };
            let disabled = options! { emit_comments: emit_comments };
            for (backend, error) in first_errors(yaml, enabled.clone()) {
                assert_eq!(error, None, "{backend}: {yaml:?}");
            }

            let expected = Parser::new_from_str_with_options(yaml, disabled.clone())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let actual = Parser::new_from_str_with_options(yaml, enabled.clone())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(actual, expected, "input: {yaml:?}");

            let expected = Scanner::with_options(StrInput::new(yaml), disabled)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let actual = Scanner::with_options(StrInput::new(yaml), enabled)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(actual, expected, "input: {yaml:?}");
        }
    }
}

#[test]
fn percent_outside_directive_position_retains_existing_errors() {
    for yaml in ["  %FOO\n", "key: %FOO\n", "[%FOO]\n"] {
        let expected = first_errors(yaml, Options::default());
        let actual = first_errors(yaml, options! { disallow_directives: true });
        assert!(expected.iter().all(|(_, error)| error.is_some()));
        assert_eq!(actual, expected, "input: {yaml:?}");
    }
}
