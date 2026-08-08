use std::{cell::Cell, rc::Rc};

use granit_parser::{
    BorrowedInput, BufferedInput, ErrorKind, Event, FallibleBufferedInput, Options, Parser,
    ScalarStyle, ScanError, Scanner, Span, StrInput, Token, TokenType,
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

fn scan_non_comment_prefix_until_error<'input, T>(
    mut scanner: Scanner<'input, T>,
) -> (Vec<Token<'input>>, usize, ScanError)
where
    T: BorrowedInput<'input>,
{
    let mut prefix = Vec::new();
    let mut comment_count = 0;

    loop {
        match scanner.next() {
            Some(Ok(token)) => {
                if matches!(token.token_type(), TokenType::Comment(_)) {
                    comment_count += 1;
                } else {
                    prefix.push(token);
                }
            }
            Some(Err(error)) => {
                assert!(scanner.next().is_none(), "scanner must fuse after an error");
                return (prefix, comment_count, error);
            }
            None => panic!("expected scanner error"),
        }
    }
}

fn parse_non_comment_prefix_until_error<'input, T>(
    mut parser: Parser<'input, T>,
) -> (Vec<(Event<'input>, Span)>, usize, ScanError)
where
    T: BorrowedInput<'input>,
{
    let mut prefix = Vec::new();
    let mut comment_count = 0;

    loop {
        match parser.next() {
            Some(Ok(event)) => {
                if matches!(event.0, Event::Comment(..)) {
                    comment_count += 1;
                } else {
                    prefix.push(event);
                }
            }
            Some(Err(error)) => {
                assert!(parser.next().is_none(), "parser must fuse after an error");
                return (prefix, comment_count, error);
            }
            None => panic!("expected parser error"),
        }
    }
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

type ExpectedScalar = Option<(ScalarStyle, &'static str)>;

fn has_completed_token(tokens: &[Token<'_>], expected_scalar: &ExpectedScalar) -> bool {
    match expected_scalar {
        Some((style, value)) => tokens.iter().any(|token| {
            matches!(token.token_type(), TokenType::Scalar(actual_style, actual_value)
                if actual_style == style && actual_value == value)
        }),
        None => {
            tokens
                .iter()
                .any(|token| matches!(token.token_type(), TokenType::BlockMappingStart))
                && tokens
                    .iter()
                    .any(|token| matches!(token.token_type(), TokenType::Key))
        }
    }
}

fn has_completed_event(events: &[(Event<'_>, Span)], expected_scalar: &ExpectedScalar) -> bool {
    match expected_scalar {
        Some((style, value)) => events.iter().any(|(event, _)| {
            matches!(event, Event::Scalar(actual_value, actual_style, ..)
                if actual_style == style && actual_value == value)
        }),
        None => events
            .iter()
            .any(|(event, _)| matches!(event, Event::MappingStart(..))),
    }
}

fn assert_scanner_prefix<'input, T>(
    context: &str,
    enabled: Scanner<'input, T>,
    disabled: Scanner<'input, T>,
    expected_error: &ErrorKind,
    expected_scalar: &ExpectedScalar,
) where
    T: BorrowedInput<'input>,
{
    let (enabled_tokens, enabled_comments, enabled_error) =
        scan_non_comment_prefix_until_error(enabled);
    let (disabled_tokens, disabled_comments, disabled_error) =
        scan_non_comment_prefix_until_error(disabled);

    assert_eq!(enabled_error.kind(), expected_error, "{context}");
    assert_eq!(disabled_error.kind(), enabled_error.kind(), "{context}");
    assert_eq!(disabled_error.marker(), enabled_error.marker(), "{context}");
    assert_eq!(disabled_tokens, enabled_tokens, "{context}");
    assert!(
        enabled_comments > 0,
        "{context}: fixture emitted no comments"
    );
    assert_eq!(disabled_comments, 0, "{context}");
    assert!(
        has_completed_token(&enabled_tokens, expected_scalar),
        "{context}: completed token was missing"
    );
}

fn assert_parser_prefix<'input, T>(
    context: &str,
    enabled: Parser<'input, T>,
    disabled: Parser<'input, T>,
    expected_error: &ErrorKind,
    expected_scalar: &ExpectedScalar,
) where
    T: BorrowedInput<'input>,
{
    let (enabled_events, enabled_comments, enabled_error) =
        parse_non_comment_prefix_until_error(enabled);
    let (disabled_events, disabled_comments, disabled_error) =
        parse_non_comment_prefix_until_error(disabled);

    assert_eq!(enabled_error.kind(), expected_error, "{context}");
    assert_eq!(disabled_error.kind(), enabled_error.kind(), "{context}");
    assert_eq!(disabled_error.marker(), enabled_error.marker(), "{context}");
    assert_eq!(disabled_events, enabled_events, "{context}");
    assert!(
        enabled_comments > 0,
        "{context}: fixture emitted no comments"
    );
    assert_eq!(disabled_comments, 0, "{context}");
    assert!(
        has_completed_event(&enabled_events, expected_scalar),
        "{context}: completed event was missing"
    );
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
fn disabled_comments_preserve_non_comment_prefixes_before_errors() {
    for (name, source, expected_error, expected_scalar) in [
        (
            "completed explicit key",
            "? # ignored\n\tbad\n",
            ErrorKind::TabNotAllowed,
            None,
        ),
        (
            "completed quoted scalar",
            "\"value\"\n# ignored\n@\n",
            ErrorKind::UnexpectedCharacter { character: '@' },
            Some((ScalarStyle::DoubleQuoted, "value")),
        ),
        (
            "completed plain scalar",
            "foo\n# ignored\n@\n",
            ErrorKind::UnexpectedCharacter { character: '@' },
            Some((ScalarStyle::Plain, "foo")),
        ),
    ] {
        assert_scanner_prefix(
            &format!("{name}, string scanner"),
            Scanner::with_options(StrInput::new(source), Options::default()),
            Scanner::with_options(StrInput::new(source), no_comments()),
            &expected_error,
            &expected_scalar,
        );
        assert_scanner_prefix(
            &format!("{name}, iterator scanner"),
            Scanner::with_options(BufferedInput::new(source.chars()), Options::default()),
            Scanner::with_options(BufferedInput::new(source.chars()), no_comments()),
            &expected_error,
            &expected_scalar,
        );
        assert_parser_prefix(
            &format!("{name}, string parser"),
            Parser::with_options(StrInput::new(source), Options::default()),
            Parser::with_options(StrInput::new(source), no_comments()),
            &expected_error,
            &expected_scalar,
        );
        assert_parser_prefix(
            &format!("{name}, iterator parser"),
            Parser::with_options(BufferedInput::new(source.chars()), Options::default()),
            Parser::with_options(BufferedInput::new(source.chars()), no_comments()),
            &expected_error,
            &expected_scalar,
        );
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

#[test]
fn completed_plain_scalar_is_returned_before_long_suppressed_comment_tail_is_read() {
    let yaml = format!("foo\n{}...\n", comment_run(4_096));
    let total_chars = yaml.chars().count();
    let chars_read = Rc::new(Cell::new(0));
    let observed_chars_read = Rc::clone(&chars_read);
    let input = yaml.chars().inspect(move |_| {
        observed_chars_read.set(observed_chars_read.get() + 1);
    });
    let mut parser = Parser::with_options(BufferedInput::new(input), no_comments());

    loop {
        let (event, _) = parser
            .next_event()
            .expect("parser should return the completed scalar")
            .expect("the comment tail should not fail");
        match event {
            Event::Comment(..) => panic!("disabled comments must not be emitted"),
            Event::Scalar(value, ScalarStyle::Plain, ..) if value == "foo" => break,
            _ => {}
        }
    }

    assert!(
        chars_read.get() < 128,
        "parser read {} of {total_chars} characters before returning the completed scalar",
        chars_read.get(),
    );
}

#[test]
fn completed_plain_scalar_is_returned_before_fallible_suppressed_comment_tail_fails() {
    let yaml = format!("foo\n{}", comment_run(4_096));
    let source_error = ErrorKind::InputByteLimitExceeded { limit: 64 };
    let input = yaml
        .chars()
        .map(Ok)
        .chain(core::iter::once(Err(source_error.clone())));
    let mut parser = Parser::with_options(FallibleBufferedInput::new(input), no_comments());

    loop {
        match parser
            .next_event()
            .expect("parser should return the completed scalar")
        {
            Ok((Event::Comment(..), _)) => panic!("disabled comments must not be emitted"),
            Ok((Event::Scalar(value, ScalarStyle::Plain, ..), _)) if value == "foo" => break,
            Ok(_) => {}
            Err(error) => panic!("source failure overtook the completed scalar: {error}"),
        }
    }

    let error = parser
        .find_map(Result::err)
        .expect("source failure after the comment tail must remain visible");
    assert_eq!(error.kind(), &source_error);
}
