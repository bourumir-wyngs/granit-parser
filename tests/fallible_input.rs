use granit_parser::{
    ErrorKind, Event, EventReceiver, FallibleBufferedInput, InputIoError, Parser, ScanError,
    Scanner,
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

    assert_eq!(error.kind(), io_error("connection reset"));
    assert!(!emitted_stream_end);
    assert!(parser.next().is_none(), "a source error must be terminal");
}

#[test]
fn source_error_takes_priority_over_eof_derived_syntax_error() {
    let input = core::iter::once(Ok('[')).chain(core::iter::once(Err(io_error("read failed"))));
    let error = Parser::new_from_fallible_iter(input)
        .find_map(Result::err)
        .expect("source failure should be reported");

    assert_eq!(error.kind(), io_error("read failed"));
}

#[test]
fn scanner_iterator_records_source_error() {
    let input = core::iter::once(Err(io_error("scanner read failed")));
    let mut scanner = Scanner::new(FallibleBufferedInput::new(input));

    assert!(scanner.next().is_none());
    assert_eq!(
        scanner
            .get_error()
            .expect("scanner should retain the source error")
            .kind(),
        io_error("scanner read failed")
    );
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

    assert_eq!(error.kind(), io_error("terminal failure"));
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

    assert_eq!(error.kind(), ErrorKind::InputByteLimitExceeded { limit: 8 });
}
