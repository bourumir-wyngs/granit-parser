use granit_parser::{
    BorrowedInput, ErrorKind, Event, Parser, Span, SpannedEventReceiver, TryLoadError,
    TrySpannedEventReceiver,
};

#[derive(Default)]
struct Events<'input>(Vec<(Event<'input>, Span)>);

impl<'input> SpannedEventReceiver<'input> for Events<'input> {
    fn on_event(&mut self, event: Event<'input>, span: Span) {
        self.0.push((event, span));
    }
}

impl<'input> TrySpannedEventReceiver<'input> for Events<'input> {
    type Error = std::convert::Infallible;

    fn on_event(&mut self, event: Event<'input>, span: Span) -> Result<(), Self::Error> {
        self.0.push((event, span));
        Ok(())
    }
}

fn assert_presence<'input, I: BorrowedInput<'input>>(
    mut parser: Parser<'input, I>,
    expected: bool,
) {
    assert_eq!(parser.proceed_next_document().unwrap(), expected);
    let events = parser
        .by_ref()
        .collect::<Result<Vec<_>, _>>()
        .expect("remaining document should parse");
    assert_eq!(
        events
            .iter()
            .any(|(event, _)| matches!(event, Event::DocumentStart(..))),
        expected,
    );
    assert_eq!(
        events
            .iter()
            .filter(|(event, _)| matches!(event, Event::StreamEnd))
            .count(),
        1,
    );
    assert!(!parser.proceed_next_document().unwrap());
    assert!(parser.next().is_none());
}

#[test]
fn presence_distinguishes_empty_documents_from_clean_eof_on_all_inputs() {
    for (source, expected) in [
        ("", false),
        ("first\n", false),
        ("first\n...\n...\n# trailing\n", false),
        ("broken: [one\n", false),
        ("broken: *missing\n...\n# trailing\n...\n", false),
        ("first\n---", true),
        ("first\n---\n...\n", true),
        ("first\n...\nsecond\n", true),
        ("first\n...\n# between\n...\n---\nsecond\n", true),
        ("broken: [one\n---\nsecond\n", true),
        ("broken: *missing\n...\n# between\nsecond\n", true),
    ] {
        assert_presence(Parser::new_from_str(source), expected);
        assert_presence(Parser::new_from_iter(source.chars()), expected);
        assert_presence(
            Parser::new_from_fallible_iter(source.chars().map(Ok)),
            expected,
        );
    }
}

#[test]
fn presence_after_document_end_preserves_the_entire_remaining_event_stream() {
    for suffix in ["# trailing\n...\n# last\n", "# between\n---\nsecond\n"] {
        let source = format!("first\n...\n{suffix}");
        let mut reference = Parser::new_from_str(&source);
        while reference.next().unwrap().unwrap().0 != Event::DocumentEnd {}
        let expected = reference.collect::<Result<Vec<_>, _>>().unwrap();

        for peek_end in [false, true] {
            let mut parser = Parser::new_from_str(&source);
            while parser.peek().unwrap().unwrap().0 != Event::DocumentEnd {
                parser.next().unwrap().unwrap();
            }
            if !peek_end {
                parser.next().unwrap().unwrap();
            }
            let has_document = expected
                .iter()
                .any(|(event, _)| matches!(event, Event::DocumentStart(..)));
            assert_eq!(parser.proceed_next_document().unwrap(), has_document);
            if !has_document {
                assert!(!parser.proceed_next_document().unwrap());
                assert_eq!(parser.peek().unwrap().unwrap(), &expected[0]);
                assert!(!parser.proceed_next_document().unwrap());
            }
            assert_eq!(parser.collect::<Result<Vec<_>, _>>().unwrap(), expected);
        }
    }
}

#[test]
fn repeated_proceed_preserves_an_unread_or_peeked_stream_end() {
    let mut parser = Parser::new_from_str("first\n");
    for _ in 0..3 {
        assert!(!parser.proceed_next_document().unwrap());
    }
    assert_eq!(parser.peek().unwrap().unwrap().0, Event::StreamEnd);
    assert!(!parser.proceed_next_document().unwrap());
    assert_eq!(parser.next().unwrap().unwrap().0, Event::StreamEnd);
    assert!(!parser.proceed_next_document().unwrap());
    assert!(parser.next().is_none());
}

fn assert_malformed_header_remains_available<'input, I: BorrowedInput<'input>>(
    mut parser: Parser<'input, I>,
    read_error: bool,
) {
    assert!(parser.proceed_next_document().unwrap());
    if read_error {
        let error = parser.by_ref().find_map(Result::err).unwrap();
        assert!(error.marker().line() >= 3);
    }
    assert!(parser.proceed_next_document().unwrap());
    let remaining = parser.collect::<Result<Vec<_>, _>>().unwrap();
    let scalars: Vec<_> = remaining
        .iter()
        .filter_map(|(event, _)| event.scalar().map(|(value, _)| value))
        .collect();
    assert_eq!(scalars, ["healthy"]);
}

#[test]
fn malformed_next_header_is_reported_by_normal_parsing_and_can_be_recovered() {
    for header in ["%YAML bad", "%YAML 2.0"] {
        let source = format!("first\n...\n# before header\n{header}\n---\nskipped\n---\nhealthy\n");
        for read_error in [false, true] {
            assert_malformed_header_remains_available(Parser::new_from_str(&source), read_error);
            assert_malformed_header_remains_available(
                Parser::new_from_iter(source.chars()),
                read_error,
            );
            assert_malformed_header_remains_available(
                Parser::new_from_fallible_iter(source.chars().map(Ok)),
                read_error,
            );
        }
    }
}

#[test]
fn load_and_try_load_preserve_buffered_comments_before_the_next_syntax_error() {
    for header in ["%YAML bad", "%YAML 2.0", "@"] {
        let source = format!("first\n...\n# one\n# two\n{header}\n---\nbody\n");
        let mut reference = Parser::new_from_str(&source);
        while reference.next().unwrap().unwrap().0 != Event::DocumentEnd {}
        let mut expected = Vec::new();
        let expected_error = loop {
            match reference.next().unwrap() {
                Ok(event) => expected.push(event),
                Err(error) => break error,
            }
        };

        for fallible_receiver in [false, true] {
            let mut parser = Parser::new_from_str(&source);
            assert!(parser.proceed_next_document().unwrap());
            let mut events = Events::default();
            let error = if fallible_receiver {
                match parser.try_load(&mut events, true).unwrap_err() {
                    TryLoadError::Scan(error) => error,
                    TryLoadError::Receiver(never) => match never {},
                }
            } else {
                parser.load(&mut events, true).unwrap_err()
            };
            assert_eq!(events.0, expected);
            assert_eq!(error.kind(), expected_error.kind());
            assert_eq!(error.marker(), expected_error.marker());
            assert!(parser.next().is_none());
        }
    }
}

#[test]
fn incomplete_next_header_is_not_reported_as_clean_eof() {
    let mut parser = Parser::new_from_str("first\n...\n%YAML 1.2\n");
    assert!(parser.proceed_next_document().unwrap());
    let error = parser.by_ref().find_map(Result::err).unwrap();
    assert_eq!(error.kind(), &ErrorKind::ExpectedDocumentStart);
    assert!(!parser.proceed_next_document().unwrap());
    assert_eq!(parser.next().unwrap().unwrap().0, Event::StreamEnd);
}

#[cfg(feature = "comments")]
fn assert_pending_header_presence<'input, I: BorrowedInput<'input>>(
    mut parser: Parser<'input, I>,
    consume_comment: bool,
) {
    while parser.next().unwrap().unwrap().0 != Event::DocumentEnd {}
    assert!(matches!(
        parser.peek().unwrap().unwrap().0,
        Event::Comment(..)
    ));
    if consume_comment {
        parser.next().unwrap().unwrap();
    }

    assert!(parser.proceed_next_document().unwrap());
    let error = parser.by_ref().find_map(Result::err).unwrap();
    assert_eq!(error.kind(), &ErrorKind::ExpectedDocumentStart);
    assert!(parser.next().is_none());
}

#[cfg(feature = "comments")]
#[test]
fn presence_preserves_a_header_paused_at_a_comment() {
    for header in ["%YAML 1.2", "%TAG !e! tag:example.com,2026:", "%RESERVED"] {
        for suffix in ["", "...\n---\nsecond\n"] {
            let source = format!("first\n...\n{header}\n# header\n{suffix}");
            for consume_comment in [false, true] {
                assert_pending_header_presence(Parser::new_from_str(&source), consume_comment);
                assert_pending_header_presence(
                    Parser::new_from_iter(source.chars()),
                    consume_comment,
                );
                assert_pending_header_presence(
                    Parser::new_from_fallible_iter(source.chars().map(Ok)),
                    consume_comment,
                );
            }
        }
    }
}

#[cfg(feature = "comments")]
#[test]
fn pending_header_probe_keeps_source_failures_terminal() {
    let failure = ErrorKind::UnknownAnchor;
    let source = format!("first\n...\n%YAML 1.2\n# {}", "comment ".repeat(100));
    let input = source
        .chars()
        .map(Ok)
        .chain(std::iter::once(Err(failure.clone())))
        .chain(std::iter::from_fn(|| panic!("source polled after failure")));
    let mut parser = Parser::new_from_fallible_iter(input);
    while parser.next().unwrap().unwrap().0 != Event::DocumentEnd {}
    assert!(matches!(
        parser.peek().unwrap().unwrap().0,
        Event::Comment(..)
    ));

    assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
    assert!(parser.peek().is_none());
    assert!(parser.next().is_none());
    let mut events = Events::default();
    parser.load(&mut events, true).unwrap();
    parser.try_load(&mut events, true).unwrap();
    assert_eq!(events.0, []);
    assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
}

#[test]
fn source_failure_while_probing_past_comments_is_terminal() {
    for failure in [
        ErrorKind::InputDecoding {
            message: "interrupted lookahead".into(),
        },
        ErrorKind::UnknownAnchor,
    ] {
        let source = format!("first\n...\n# {}", "comment ".repeat(100));
        let input = source
            .chars()
            .map(Ok)
            .chain(std::iter::once(Err(failure.clone())))
            .chain(std::iter::from_fn(|| panic!("source polled after failure")));
        let mut parser = Parser::new_from_fallible_iter(input);
        while parser.next().unwrap().unwrap().0 != Event::DocumentEnd {}
        assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
        assert!(parser.peek().is_none());
        assert!(parser.next().is_none());
        let mut events = Events::default();
        parser.load(&mut events, true).unwrap();
        parser.try_load(&mut events, true).unwrap();
        assert_eq!(events.0, []);
        assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
    }
}

#[cfg(feature = "comments")]
#[test]
fn terminal_probe_error_discards_a_previously_peeked_comment() {
    let failure = ErrorKind::UnknownAnchor;
    let source = format!("first\n...\n# one\n# two\n# {}", "comment ".repeat(100));
    let input = source
        .chars()
        .map(Ok)
        .chain(std::iter::once(Err(failure.clone())))
        .chain(std::iter::from_fn(|| panic!("source polled after failure")));
    let mut parser = Parser::new_from_fallible_iter(input);
    while parser.next().unwrap().unwrap().0 != Event::DocumentEnd {}
    assert!(matches!(
        parser.peek().unwrap().unwrap().0,
        Event::Comment(..)
    ));
    assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
    assert!(parser.peek().is_none());
    assert!(parser.next().is_none());
    let mut events = Events::default();
    parser.load(&mut events, true).unwrap();
    parser.try_load(&mut events, true).unwrap();
    assert_eq!(events.0, []);
}

#[cfg(feature = "comments")]
#[test]
fn probing_respects_the_comment_buffer_limit_and_disabled_comments() {
    use granit_parser::options;

    for limit in [0, 1] {
        let source = "first\n...\n# one\n...\n# two\n";
        let mut parser = Parser::new_from_str_with_options(
            source,
            options! { max_buffered_comment_events: limit },
        );
        assert_eq!(
            parser.proceed_next_document().unwrap_err().kind(),
            &ErrorKind::TooManyComments,
        );

        let mut parser = Parser::new_from_str_with_options(
            source,
            options! {
                max_buffered_comment_events: limit,
                emit_comments: false,
            },
        );
        assert!(!parser.proceed_next_document().unwrap());
        assert_eq!(parser.next().unwrap().unwrap().0, Event::StreamEnd);

        let mut parser = Parser::new_from_str_with_options(
            "first\n...\n# one\n# two\n---\nhealthy\n",
            options! { max_buffered_comment_events: limit },
        );
        assert_eq!(
            parser.proceed_next_document().unwrap_err().kind(),
            &ErrorKind::TooManyComments,
        );
        assert!(parser.peek().is_none());
        assert!(parser.next().is_none());
        assert!(parser.proceed_next_document().unwrap());
        let remaining = parser.collect::<Result<Vec<_>, _>>().unwrap();
        assert!(remaining
            .iter()
            .any(|(event, _)| event.scalar().is_some_and(|(value, _)| value == "healthy")));
    }
}
