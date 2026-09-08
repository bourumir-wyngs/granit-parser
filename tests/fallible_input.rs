use granit_parser::{
    BorrowedInput, ErrorKind, Event, EventReceiver, FallibleBufferedInput, Input, InputIoError,
    Parser, ScanError, Scanner,
};

fn io_error(message: &str) -> ErrorKind {
    ErrorKind::InputIo {
        error: InputIoError::from_message(message),
    }
}

#[test]
fn fallible_iterator_clean_eof_emits_stream_end() {
    let input = "key: value\n".chars().map(Ok::<_, ErrorKind>);
    let events = Parser::new_from_fallible_iter(input)
        .collect::<Result<Vec<_>, _>>()
        .expect("clean EOF should finish parsing");

    assert!(matches!(events.last(), Some((Event::StreamEnd, _))));
}

#[test]
fn source_error_cannot_be_mistaken_for_clean_eof() {
    let input = "key: value\n"
        .chars()
        .map(Ok)
        .chain(core::iter::once(Err(io_error("connection reset"))));
    let mut parser = Parser::new_from_fallible_iter(input);
    let mut emitted_stream_end = false;

    let error = loop {
        match parser.next() {
            Some(Ok((event, _))) => emitted_stream_end |= matches!(event, Event::StreamEnd),
            Some(Err(error)) => break error,
            None => panic!("source failure was silently treated as EOF"),
        }
    };

    assert_eq!(error.kind(), &io_error("connection reset"));
    assert!(!emitted_stream_end);
    assert!(parser.next().is_none(), "a source error must be terminal");
}

#[test]
fn source_error_takes_priority_over_eof_derived_syntax_error() {
    let input = core::iter::once(Ok('[')).chain(core::iter::once(Err(io_error("read failed"))));
    let error = Parser::new_from_fallible_iter(input)
        .find_map(Result::err)
        .expect("source failure should be reported");

    assert_eq!(error.kind(), &io_error("read failed"));
}

#[test]
fn scanner_iterator_emits_source_error() {
    let input = core::iter::once(Err(io_error("scanner read failed")));
    let mut scanner = Scanner::new(FallibleBufferedInput::new(input));

    let error = scanner
        .by_ref()
        .find_map(Result::err)
        .expect("scanner should emit the source error");

    assert_eq!(error.kind(), &io_error("scanner read failed"));
    assert!(scanner.next().is_none(), "a source error must be terminal");
}

struct ErrorThenPanic {
    next: usize,
}

impl Iterator for ErrorThenPanic {
    type Item = Result<char, ErrorKind>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = match self.next {
            0 => Ok('a'),
            1 => Err(io_error("terminal failure")),
            _ => panic!("fallible input was polled after its first error"),
        };
        self.next += 1;
        Some(item)
    }
}

#[test]
fn source_is_not_polled_after_error() {
    let mut parser = Parser::new_from_fallible_iter(ErrorThenPanic { next: 0 });
    let error = parser
        .find_map(Result::err)
        .expect("source failure should be reported");

    assert_eq!(error.kind(), &io_error("terminal failure"));
}

#[test]
fn fallible_raw_reads_preserve_breaks_and_literal_nul() {
    for break_char in ['\n', '\r', '\0'] {
        for lookahead in [0, 2] {
            let source = [Ok('a'), Ok(break_char), Ok('b')];
            let mut input = FallibleBufferedInput::new(source.into_iter());
            input.lookahead(lookahead);

            assert_eq!(input.raw_read_non_breakz_ch(), Some('a'));
            assert_eq!(input.raw_read_non_breakz_ch(), None);
            assert_eq!(input.raw_read_non_breakz_ch(), None);
            assert!(!input.next_is_z(), "a literal NUL is not source EOF");
            assert_eq!(input.raw_read_ch(), break_char);
            assert_eq!(input.raw_read_non_breakz_ch(), Some('b'));
            assert_eq!(input.raw_read_non_breakz_ch(), None);
            assert!(input.next_is_z());
            assert_eq!(input.take_source_error(), None);
        }
    }
}

#[test]
fn taking_source_error_does_not_resume_the_source() {
    for lookahead in [0, 1, 2] {
        let mut input = FallibleBufferedInput::new(ErrorThenPanic { next: 0 });
        assert_eq!(input.take_source_error(), None);
        input.lookahead(lookahead);

        assert_eq!(input.raw_read_ch(), 'a');
        input.skip_n(usize::MAX);
        assert_eq!(
            input.take_source_error(),
            Some(io_error("terminal failure"))
        );
        assert_eq!(input.take_source_error(), None);

        input.lookahead(input.bufmaxlen());
        input.skip();
        input.skip_n(usize::MAX);
        assert_eq!(input.raw_read_ch(), '\0');
        assert_eq!(input.raw_read_non_breakz_ch(), None);
        assert!(input.next_is_z());
        assert_eq!(input.take_source_error(), None);
    }
}

#[test]
fn fallible_source_is_not_polled_after_clean_eof() {
    let mut calls = 0;
    let source = core::iter::from_fn(|| {
        calls += 1;
        match calls {
            1 => Some(Ok('a')),
            2 => None,
            _ => panic!("fallible source was polled after clean EOF"),
        }
    });
    let mut input = FallibleBufferedInput::new(source);
    input.lookahead(input.bufmaxlen() + 1);

    assert_eq!(input.buflen(), input.bufmaxlen());
    assert_eq!(input.peek(), 'a');
    assert_eq!(input.peek_nth(1), '\0');
    assert!(!input.next_is_z(), "the buffered character precedes EOF");
    assert_eq!(input.raw_read_ch(), 'a');
    input.skip_n(usize::MAX);
    assert_eq!(input.raw_read_non_breakz_ch(), None);
    assert_eq!(input.raw_read_ch(), '\0');
    input.lookahead(input.bufmaxlen());
    assert!(input.next_is_z());
    assert_eq!(input.take_source_error(), None);
}

#[test]
fn fallible_streaming_input_cannot_borrow_source_slices() {
    let input = FallibleBufferedInput::new("abc".chars().map(Ok::<_, ErrorKind>));

    assert_eq!(input.byte_offset(), None);
    assert_eq!(input.slice_bytes(0, 1), None);
    assert_eq!(input.slice_borrowed(0, 1), None);
}

struct Sink;

impl EventReceiver<'static> for Sink {
    fn on_event(&mut self, _event: Event<'static>) {}
}

#[test]
fn receiver_api_returns_byte_limit_error() {
    let input = "key: value".chars().map(Ok).chain(core::iter::once(Err(
        ErrorKind::InputByteLimitExceeded { limit: 8 },
    )));
    let mut parser = Parser::new_from_fallible_iter(input);

    let error: ScanError = parser
        .load(&mut Sink, true)
        .expect_err("byte limit failure should stop receiver loading");

    assert_eq!(
        error.kind(),
        &ErrorKind::InputByteLimitExceeded { limit: 8 }
    );
}
