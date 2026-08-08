use granit_parser::{
    BufferedInput, ErrorKind, Event, FallibleBufferedInput, Options, Parser, ScalarStyle,
    ScanError, Scanner, StrInput, Token, TokenType,
};

fn no_comments() -> Options {
    granit_parser::options! {
        emit_comments: false,
    }
}

fn no_comments_with_zero_buffer() -> Options {
    granit_parser::options! {
        emit_comments: false,
        max_buffered_comment_events: 0,
    }
}

fn parse_str(source: &str, options: Options) -> Result<Vec<Event<'_>>, ScanError> {
    Parser::with_options(StrInput::new(source), options)
        .map(|result| result.map(|(event, _)| event))
        .collect()
}

fn parse_iter(source: &str, options: Options) -> Result<Vec<Event<'static>>, ScanError> {
    Parser::with_options(BufferedInput::new(source.chars()), options)
        .map(|result| result.map(|(event, _)| event))
        .collect()
}

fn scan_str(source: &str, options: Options) -> Result<Vec<Token<'_>>, ScanError> {
    Scanner::with_options(StrInput::new(source), options).collect()
}

fn scan_iter(source: &str, options: Options) -> Result<Vec<Token<'static>>, ScanError> {
    Scanner::with_options(BufferedInput::new(source.chars()), options).collect()
}

fn assert_no_comment_events(events: &[Event<'_>]) {
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, Event::Comment(..))),
        "comment event escaped suppression: {events:?}",
    );
}

fn assert_no_comment_tokens(tokens: &[Token<'_>]) {
    assert!(
        tokens
            .iter()
            .all(|token| !matches!(token.token_type(), TokenType::Comment(_))),
        "comment token escaped suppression: {tokens:?}",
    );
}

fn assert_parser_error(source: &str, expected: &ErrorKind) {
    let str_error = Parser::with_options(StrInput::new(source), no_comments())
        .find_map(Result::err)
        .expect("string parser should reject invalid YAML");
    assert_eq!(str_error.kind(), expected, "string input: {source:?}");

    let iter_error = Parser::with_options(BufferedInput::new(source.chars()), no_comments())
        .find_map(Result::err)
        .expect("iterator parser should reject invalid YAML");
    assert_eq!(iter_error.kind(), expected, "iterator input: {source:?}");
}

fn comment_run(count: usize) -> String {
    let mut comments = String::new();
    for index in 0..count {
        comments.push_str("# comment ");
        comments.push_str(&index.to_string());
        comments.push('\n');
    }
    comments
}

#[test]
fn scanner_suppresses_every_lexical_comment_for_string_and_iterator_input() {
    let yaml = concat!(
        "# before document\n",
        "--- # after document start\n",
        "key: value # right\n",
        "flow: [one, # after flow entry\n",
        "  two] # after flow end\n",
        "... # after document end\n",
        "# before stream end\n",
    );

    let str_tokens = scan_str(yaml, no_comments()).expect("string scanner should accept comments");
    assert_no_comment_tokens(&str_tokens);

    let iter_tokens =
        scan_iter(yaml, no_comments()).expect("iterator scanner should accept comments");
    assert_no_comment_tokens(&iter_tokens);

    for tokens in [&str_tokens[..], &iter_tokens[..]] {
        assert!(tokens.iter().any(|token| {
            matches!(token.token_type(), TokenType::Scalar(_, value) if value == "value")
        }));
        assert!(tokens.iter().any(|token| {
            matches!(token.token_type(), TokenType::Scalar(_, value) if value == "two")
        }));
    }
}

#[test]
fn parser_suppresses_every_comment_event_for_string_and_iterator_input() {
    let yaml = concat!(
        "# leading\n",
        "root: # above sequence\n",
        "  - one # right\n",
        "  - [two, # flow\n",
        "     three]\n",
        "# trailing\n",
    );

    let str_events = parse_str(yaml, no_comments()).expect("string parser should accept comments");
    assert_no_comment_events(&str_events);

    let iter_events =
        parse_iter(yaml, no_comments()).expect("iterator parser should accept comments");
    assert_no_comment_events(&iter_events);
}

#[test]
fn disabled_comments_match_enabled_yaml_semantics_after_filtering_comments() {
    let yaml = concat!(
        "# header\n",
        "---\n",
        "root: # before sequence\n",
        "  - one # right\n",
        "  - [two, # in flow\n",
        "     three]\n",
        "empty: # empty value\n",
        "next: &anchor value\n",
        "alias: *anchor # alias\n",
        "# footer\n",
    );

    let mut enabled_str =
        parse_str(yaml, Options::default()).expect("enabled string parser should accept YAML");
    enabled_str.retain(|event| !matches!(event, Event::Comment(..)));
    let disabled_str =
        parse_str(yaml, no_comments()).expect("disabled string parser should accept YAML");
    assert_eq!(disabled_str, enabled_str);

    let mut enabled_iter =
        parse_iter(yaml, Options::default()).expect("enabled iterator parser should accept YAML");
    enabled_iter.retain(|event| !matches!(event, Event::Comment(..)));
    let disabled_iter =
        parse_iter(yaml, no_comments()).expect("disabled iterator parser should accept YAML");
    assert_eq!(disabled_iter, enabled_iter);
}

#[test]
fn disabled_comments_bypass_zero_buffer_limit_in_every_ambiguous_entry() {
    let comments = comment_run(64);
    let cases = [
        (
            "block mapping value",
            format!("key: {comments}next: value\n"),
        ),
        ("block sequence entry", format!("- {comments}- value\n")),
        (
            "indentless sequence entry",
            format!("key:\n- {comments}next: value\n"),
        ),
        (
            "later indentless sequence entry",
            format!("key:\n- first\n- {comments}  second\nnext: value\n"),
        ),
        ("flow mapping value", format!("root: {{key: {comments}}}\n")),
    ];

    for (name, yaml) in cases {
        let str_events = parse_str(&yaml, no_comments_with_zero_buffer())
            .unwrap_or_else(|error| panic!("{name}, string input: {error}"));
        assert_no_comment_events(&str_events);

        let iter_events = parse_iter(&yaml, no_comments_with_zero_buffer())
            .unwrap_or_else(|error| panic!("{name}, iterator input: {error}"));
        assert_no_comment_events(&iter_events);
    }
}

#[test]
fn suppressing_comments_does_not_relax_comment_syntax_validation() {
    let cases = [
        (
            "key: \"value\"# unseparated\n",
            ErrorKind::CommentNotSeparated,
        ),
        ("block: ># unseparated\n", ErrorKind::CommentNotSeparated),
        (
            "word1  # interrupts scalar\nword2\n",
            ErrorKind::CommentInterceptedScalar,
        ),
        (
            "? # first\n  # second\n\tkey\n: value\n",
            ErrorKind::TabNotAllowed,
        ),
        (
            "key: \"value\" # control \u{1}\n",
            ErrorKind::UnexpectedCharacter { character: '\u{1}' },
        ),
    ];

    for (source, expected) in cases {
        assert_parser_error(source, &expected);
    }
}

#[test]
fn suppressed_streaming_comment_preserves_source_errors() {
    let limit = ErrorKind::InputByteLimitExceeded { limit: 13 };
    let source = "# unfinished\n";
    let input = source
        .chars()
        .map(Ok)
        .chain(core::iter::once(Err(limit.clone())));
    let mut parser = Parser::with_options(FallibleBufferedInput::new(input), no_comments());
    let mut events = Vec::new();
    let error = loop {
        match parser.next() {
            Some(Ok((event, _))) => events.push(event),
            Some(Err(error)) => break error,
            None => panic!("source failure after an ignored comment must remain visible"),
        }
    };

    assert_no_comment_events(&events);
    assert!(events
        .iter()
        .all(|event| !matches!(event, Event::StreamEnd)));
    assert_eq!(error.kind(), &limit);
    assert_eq!(error.marker().index(), source.chars().count());
    assert_eq!((error.marker().line(), error.marker().col()), (2, 0));
    assert!(
        parser.next().is_none(),
        "the parser must fuse after the error"
    );
}

fn assert_literal_hash_scalars(events: &[Event<'_>]) {
    let expected = [
        (ScalarStyle::Plain, "value#plain"),
        (ScalarStyle::SingleQuoted, "# single"),
        (ScalarStyle::DoubleQuoted, "# double"),
        (ScalarStyle::Literal, "# block\n"),
    ];

    for (style, expected_value) in expected {
        assert!(
            events.iter().any(|event| {
                matches!(event, Event::Scalar(value, actual_style, ..)
                    if *actual_style == style && value == expected_value)
            }),
            "missing {style:?} scalar {expected_value:?} in {events:?}",
        );
    }
}

#[test]
fn hash_characters_inside_scalars_remain_data_when_comments_are_disabled() {
    let yaml = concat!(
        "plain: value#plain\n",
        "single: '# single'\n",
        "double: \"# double\"\n",
        "block: |\n",
        "  # block\n",
    );

    let str_events = parse_str(yaml, no_comments()).expect("string parser should accept scalars");
    assert_no_comment_events(&str_events);
    assert_literal_hash_scalars(&str_events);

    let iter_events =
        parse_iter(yaml, no_comments()).expect("iterator parser should accept scalars");
    assert_no_comment_events(&iter_events);
    assert_literal_hash_scalars(&iter_events);
}
