use granit_parser::{ErrorKind, ScalarStyle, Scanner, StrInput, TokenType};

/// Check a scanner trace against public constructors and across input backends.
fn assert_payloads(source: &str, expected: &[TokenType<'_>]) {
    let string_tokens = Scanner::new(StrInput::new(source))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let iterated_tokens = Scanner::new(granit_parser::BufferedInput::new(source.chars()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let fallible_tokens = Scanner::new(granit_parser::FallibleBufferedInput::new(
        source.chars().map(Ok::<_, ErrorKind>),
    ))
    .collect::<Result<Vec<_>, _>>()
    .unwrap();

    for tokens in [&string_tokens, &iterated_tokens, &fallible_tokens] {
        let payloads: Vec<_> = tokens
            .iter()
            .map(|token| token.token_type().clone())
            .collect();
        assert_eq!(payloads, expected, "source: {source:?}");
        // Output equality ignores implicitness, so check this scanner guarantee explicitly.
        #[cfg(feature = "extended_details")]
        for token in tokens {
            if let TokenType::Scalar {
                style: ScalarStyle::Plain { implicit, .. },
                ..
            } = token.token_type()
            {
                assert!(!implicit, "written scalar was marked implicit: {source:?}");
            }
        }
    }
    assert_eq!(string_tokens, iterated_tokens);
    assert_eq!(string_tokens, fallible_tokens);
}

#[test]
fn constructed_directive_and_property_tokens_match_scanned_source() {
    assert_payloads(
        "%YAML 1.2\n%TAG !e! tag:example.org,2026:\n%FOO first second\n\
         ---\n{key: &a !e!thing value, alias: *a}\n...\n",
        &[
            TokenType::stream_start(),
            TokenType::version_directive(1, 2),
            TokenType::tag_directive("!e!".into(), "tag:example.org,2026:".into()),
            TokenType::reserved_directive("FOO".into(), vec!["first".into(), "second".into()]),
            TokenType::document_start(),
            TokenType::flow_mapping_start(),
            TokenType::key(),
            TokenType::scalar(ScalarStyle::plain(), "key".into()),
            TokenType::value(),
            TokenType::anchor("a".into()),
            TokenType::tag("!e!".into(), "thing".into()),
            TokenType::scalar(ScalarStyle::plain(), "value".into()),
            TokenType::flow_entry(),
            TokenType::key(),
            TokenType::scalar(ScalarStyle::plain(), "alias".into()),
            TokenType::value(),
            TokenType::alias("a".into()),
            TokenType::flow_mapping_end(),
            TokenType::document_end(),
            TokenType::stream_end(),
        ],
    );
}

#[test]
fn constructed_collection_tokens_match_nested_block_and_flow_notation() {
    assert_payloads(
        "root:\n  - [plain, \"double\"]\n  - {single: 'quoted'}\n",
        &[
            TokenType::stream_start(),
            TokenType::block_mapping_start(),
            TokenType::key(),
            TokenType::scalar(ScalarStyle::plain(), "root".into()),
            TokenType::value(),
            TokenType::block_sequence_start(),
            TokenType::block_entry(),
            TokenType::flow_sequence_start(),
            TokenType::scalar(ScalarStyle::plain(), "plain".into()),
            TokenType::flow_entry(),
            TokenType::scalar(ScalarStyle::double_quoted(), "double".into()),
            TokenType::flow_sequence_end(),
            TokenType::block_entry(),
            TokenType::flow_mapping_start(),
            TokenType::key(),
            TokenType::scalar(ScalarStyle::plain(), "single".into()),
            TokenType::value(),
            TokenType::scalar(ScalarStyle::single_quoted(), "quoted".into()),
            TokenType::flow_mapping_end(),
            TokenType::block_end(),
            TokenType::block_end(),
            TokenType::stream_end(),
        ],
    );
}

#[cfg(feature = "comments")]
#[test]
fn constructed_comment_tokens_preserve_raw_text_and_placement() {
    use granit_parser::{Comment, Placement};

    assert_payloads(
        "\"value\" # café\r\n# final",
        &[
            TokenType::stream_start(),
            TokenType::scalar(ScalarStyle::double_quoted(), "value".into()),
            TokenType::comment(Comment::new(" café").with_placement(Placement::Right)),
            TokenType::comment(Comment::new(" final")),
            TokenType::stream_end(),
        ],
    );
}
