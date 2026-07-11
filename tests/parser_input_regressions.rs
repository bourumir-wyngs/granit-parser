use granit_parser::{BufferedInput, ErrorKind, Event, Input, Parser, ScanError, StrInput};

fn first_str_error(input: &str) -> ScanError {
    Parser::new_from_str(input)
        .find_map(Result::err)
        .expect("input should fail")
}

fn first_iter_error(input: &str) -> ScanError {
    Parser::new_from_iter(input.chars())
        .find_map(Result::err)
        .expect("input should fail")
}

fn first_str_scalar(input: &str) -> String {
    Parser::new_from_str(input)
        .find_map(|item| match item.expect("input should parse") {
            (Event::Scalar(value, ..), _) => Some(value.into_owned()),
            _ => None,
        })
        .expect("input should contain a scalar")
}

fn first_iter_scalar(input: &str) -> String {
    Parser::new_from_iter(input.chars())
        .find_map(|item| match item.expect("input should parse") {
            (Event::Scalar(value, ..), _) => Some(value.into_owned()),
            _ => None,
        })
        .expect("input should contain a scalar")
}

#[test]
fn parser_iterator_terminates_after_scan_error() {
    let parser = Parser::new_from_str("foo:\n  bar\ninvalid\n");
    let mut errors = 0usize;
    let mut events = 0usize;

    for item in parser {
        events += 1;
        if item.is_err() {
            errors += 1;
        }
        assert!(
            events < 1000,
            "Parser iterator did not terminate after a scan error"
        );
    }

    assert_eq!(errors, 1, "error should be yielded exactly once");
}

#[test]
fn buffered_skip_n_matches_str_input_and_saturates_at_eof() {
    let mut buffered = BufferedInput::new("abc".chars());
    buffered.lookahead(1);
    buffered.skip_n(2);
    buffered.lookahead(1);

    let mut str_input = StrInput::new("abc");
    str_input.lookahead(1);
    str_input.skip_n(2);
    str_input.lookahead(1);

    assert_eq!(buffered.peek(), str_input.peek());

    buffered.skip_n(8);
    str_input.skip_n(8);
    assert_eq!(buffered.peek(), str_input.peek());
}

#[test]
fn buffered_skip_without_lookahead_matches_str_input() {
    let mut buffered = BufferedInput::new("ab".chars());
    buffered.skip();
    buffered.lookahead(1);

    let mut str_input = StrInput::new("ab");
    str_input.skip();
    str_input.lookahead(1);

    assert_eq!(buffered.peek(), str_input.peek());
}

#[test]
fn buffered_raw_reads_use_logical_stream_front() {
    let mut buffered = BufferedInput::new("ab".chars());
    buffered.lookahead(1);

    let mut str_input = StrInput::new("ab");
    str_input.lookahead(1);

    assert_eq!(buffered.raw_read_ch(), str_input.raw_read_ch());
    buffered.lookahead(1);
    str_input.lookahead(1);
    assert_eq!(buffered.peek(), str_input.peek());
}

#[test]
fn buffered_buflen_matches_str_input_lookahead_window() {
    let mut buffered = BufferedInput::new("ab".chars());
    buffered.lookahead(2);
    buffered.skip();
    buffered.skip();

    let mut str_input = StrInput::new("ab");
    str_input.lookahead(2);
    str_input.skip();
    str_input.skip();

    assert_eq!(buffered.buflen(), str_input.buflen());
    assert_eq!(buffered.buf_is_empty(), str_input.buf_is_empty());
    assert_eq!(buffered.peek(), str_input.peek());
}

#[test]
fn non_printable_source_characters_are_rejected_by_both_inputs() {
    for character in ['\0', '\u{1}'] {
        let inputs = [
            format!("key: before{character}after\n"),
            format!("'before{character}after'\n"),
            format!("\"before{character}after\"\n"),
            format!("key: |\n  before{character}after\n"),
            format!("key: >\n  before{character}after\n"),
            format!("# before{character}after\nkey: value\n"),
        ];

        for input in &inputs {
            assert_eq!(
                first_str_error(input).kind(),
                ErrorKind::UnexpectedCharacter { character },
                "string input accepted {input:?}",
            );
            assert_eq!(
                first_iter_error(input).kind(),
                ErrorKind::UnexpectedCharacter { character },
                "iterator input accepted {input:?}",
            );
        }
    }
}

#[test]
fn escaped_nul_in_double_quoted_scalar_remains_valid() {
    let input = "\"\\0\"\n";

    assert_eq!(first_str_scalar(input), "\0");
    assert_eq!(first_iter_scalar(input), "\0");
}
