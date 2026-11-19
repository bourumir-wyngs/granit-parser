use granit_parser::{
    input::BufferedInput, options, ErrorKind, Event, Parser, ScalarKind, ScanError, Scanner,
    StrInput, TokenType,
};

fn scalars(input: &str, options: granit_parser::Options) -> Result<Vec<String>, ScanError> {
    let parser = Parser::new_from_str_with_options(input, options);
    let mut out = Vec::new();
    for next in parser {
        if let (Event::Scalar { value, .. }, _) = next? {
            out.push(value.into_owned());
        }
    }
    Ok(out)
}

#[test]
fn a_top_level_block_scalar_may_start_at_column_zero_by_default() {
    for header in ["|", ">"] {
        let input = format!("{header}\nfoo\n");
        assert_eq!(scalars(&input, options! {}).unwrap(), ["foo\n"]);
    }
}

#[test]
fn with_the_option_column_zero_reserved_indicators_are_scanned_after_the_scalar() {
    for header in ["|", ">"] {
        for blank_lines in ["", "\n"] {
            let input = format!("{header}\n{blank_lines}@");
            let error =
                scalars(&input, options! { block_scalar_indent_at_least_one: true }).unwrap_err();
            assert_eq!(
                error.kind(),
                &ErrorKind::UnexpectedCharacter { character: '@' },
                "{input:?}"
            );
            assert_eq!(error.marker().line(), 2 + blank_lines.len());
            assert_eq!(error.marker().col(), 0);
        }
    }
}

#[test]
fn scanner_finishes_the_block_scalar_before_column_zero_content() {
    for (header, kind) in [("|", ScalarKind::Literal), (">", ScalarKind::Folded)] {
        for (suffix, blank_lines, expected) in [("", "", ""), ("+", "\n", "\n")] {
            let input = format!("{header}{suffix}\n{blank_lines}foo\n");
            let options = options! { block_scalar_indent_at_least_one: true };
            let string_tokens = Scanner::with_options(StrInput::new(&input), options.clone())
                .map(|token| token.map(|token| token.into_parts().1))
                .collect::<Result<Vec<_>, _>>();
            let streaming_tokens =
                Scanner::with_options(BufferedInput::new(input.chars()), options)
                    .map(|token| token.map(|token| token.into_parts().1))
                    .collect::<Result<Vec<_>, _>>();

            for tokens in [string_tokens, streaming_tokens] {
                let tokens = tokens.expect("column-zero content is a separate token");
                assert!(
                    matches!(
                        tokens.as_slice(),
                        [TokenType::StreamStart { .. },
                         TokenType::Scalar { style, value, .. },
                         TokenType::Scalar { style: next_style, value: next_value, .. },
                         TokenType::StreamEnd { .. }]
                        if style.kind() == kind && value == expected
                            && next_style.kind() == ScalarKind::Plain && next_value == "foo"
                    ),
                    "{input:?}: {tokens:?}"
                );
            }

            // Separate tokens do not make two root nodes a valid YAML document.
            let error =
                scalars(&input, options! { block_scalar_indent_at_least_one: true }).unwrap_err();
            assert_eq!(error.kind(), &ErrorKind::ExpectedDocumentStart, "{input:?}");
        }
    }
}

#[test]
fn the_option_preserves_indentation_validation() {
    for header in ["|", ">"] {
        for input in [
            format!("{header}1\nfoo\n"),
            format!("{header}\n \nfoo\n"),
            format!("{header}\n  \nfoo\n"),
            format!("key: {header}\n  \n foo\n"),
        ] {
            for enabled in [false, true] {
                let error = scalars(
                    &input,
                    options! { block_scalar_indent_at_least_one: enabled },
                )
                .unwrap_err();
                assert_eq!(
                    error.kind(),
                    &ErrorKind::InvalidBlockScalarIndent,
                    "{input:?}"
                );
            }
        }
    }
}

#[test]
fn the_option_preserves_indented_content_and_document_boundaries() {
    for header in ["|", ">"] {
        for (tail, expected) in [
            (" foo\n", vec!["foo\n"]),
            ("---\nnext\n", vec!["", "next"]),
            ("# comment\n---\nnext\n", vec!["", "next"]),
            ("...\nnext\n", vec!["", "next"]),
        ] {
            let input = format!("{header}\n{tail}");
            assert_eq!(
                scalars(&input, options! { block_scalar_indent_at_least_one: true }).unwrap(),
                expected,
                "{input:?}"
            );
        }
        let input = format!("key: {header}\nother: value\n");
        assert_eq!(
            scalars(&input, options! { block_scalar_indent_at_least_one: true }).unwrap(),
            ["key", "", "other", "value"]
        );
    }
}
