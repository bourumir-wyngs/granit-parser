use std::borrow::Cow;

use granit_parser::{
    BorrowedInput, BufferedInput, Comment, Event, Marker, Parser, ScalarStyle, Scanner, Span,
    StrInput,
};

fn drain_scanner<'input, T>(scanner: &mut Scanner<'input, T>)
where
    T: BorrowedInput<'input>,
{
    while scanner
        .next_token()
        .expect("scanner should not fail")
        .is_some()
    {}
}

#[test]
fn comment_preserves_raw_text_and_exposes_trimmed_text() {
    let span = Span::new(
        Marker::new(0, 1, 0).with_byte_offset(Some(0)),
        Marker::new(11, 1, 11).with_byte_offset(Some(11)),
    );
    let comment = Comment::new(span, " payload ");

    assert_eq!(comment.span, span);
    assert_eq!(comment.text, " payload ");
    assert_eq!(comment.trimmed_text(), "payload");
}

#[test]
fn comment_preserves_single_space_payload() {
    let span = Span::new(
        Marker::new(0, 1, 0).with_byte_offset(Some(0)),
        Marker::new(2, 1, 2).with_byte_offset(Some(2)),
    );
    let comment = Comment::new(span, " ");

    assert_eq!(comment.text, " ");
    assert_eq!(comment.trimmed_text(), "");
}

#[test]
fn scanner_does_not_collect_comments_by_default() {
    let mut scanner = Scanner::new(StrInput::new("# comment\nkey: value\n"));

    drain_scanner(&mut scanner);

    assert!(scanner.comments().is_empty());
}

#[test]
fn scanner_collects_comments_in_source_order() {
    let yaml = "# top\n  # indented\nkey: value # trailing\n#eof";
    let mut scanner = Scanner::new(StrInput::new(yaml)).with_comments();

    drain_scanner(&mut scanner);

    let comments = scanner.comments();
    assert_eq!(comments.len(), 4);
    assert_eq!(comments[0].text, " top");
    assert_eq!(comments[0].span.slice(yaml), Some("# top"));
    assert_eq!(comments[1].text, " indented");
    assert_eq!(comments[1].span.slice(yaml), Some("# indented"));
    assert_eq!(comments[2].text, " trailing");
    assert_eq!(comments[2].span.slice(yaml), Some("# trailing"));
    assert_eq!(comments[3].text, "eof");
    assert_eq!(comments[3].span.slice(yaml), Some("#eof"));
}

#[test]
fn scanner_take_comments_drains_collected_comments() {
    let mut scanner = Scanner::new(StrInput::new("# first\n# second\n")).with_comments();

    drain_scanner(&mut scanner);

    let comments = scanner.take_comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].text, " first");
    assert_eq!(comments[1].text, " second");
    assert!(scanner.comments().is_empty());
}

#[test]
fn scanner_preserves_empty_comment_payloads() {
    let yaml = "#\n# \n";
    let mut scanner = Scanner::new(StrInput::new(yaml)).with_comments();

    drain_scanner(&mut scanner);

    let comments = scanner.comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].text, "");
    assert_eq!(comments[0].span.slice(yaml), Some("#"));
    assert_eq!(comments[1].text, " ");
    assert_eq!(comments[1].span.slice(yaml), Some("# "));
}

#[test]
fn scanner_comment_span_stops_before_crlf() {
    let yaml = "# crlf\r\nkey: value\n";
    let mut scanner = Scanner::new(StrInput::new(yaml)).with_comments();

    drain_scanner(&mut scanner);

    let comments = scanner.comments();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text, " crlf");
    assert_eq!(comments[0].span.slice(yaml), Some("# crlf"));
}

#[test]
fn scanner_comments_from_str_input_are_borrowed() {
    let mut scanner = Scanner::new(StrInput::new("# borrowed\n")).with_comments();

    drain_scanner(&mut scanner);

    match &scanner.comments()[0].text {
        Cow::Borrowed(text) => assert_eq!(*text, " borrowed"),
        Cow::Owned(text) => panic!("expected borrowed comment text, got {text:?}"),
    }
}

#[test]
fn scanner_comments_from_buffered_input_are_owned() {
    let yaml = "key: value # streamed\n";
    let mut scanner = Scanner::new(BufferedInput::new(yaml.chars())).with_comments();

    drain_scanner(&mut scanner);

    match &scanner.comments()[0].text {
        Cow::Owned(text) => assert_eq!(text, " streamed"),
        Cow::Borrowed(text) => panic!("expected owned comment text, got {text:?}"),
    }
}

#[test]
fn scanner_does_not_collect_hash_inside_scalar_content() {
    let yaml = "single: '# no'\ndouble: \"# no\"\nblock: |\n  # no\n# yes\n";
    let mut scanner = Scanner::new(StrInput::new(yaml)).with_comments();

    drain_scanner(&mut scanner);

    let comments = scanner.comments();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text, " yes");
    assert_eq!(comments[0].span.slice(yaml), Some("# yes"));
}

#[test]
fn scanner_treats_unseparated_hash_as_plain_scalar_content() {
    let mut scanner = Scanner::new(StrInput::new("key: value#not-a-comment\n")).with_comments();

    drain_scanner(&mut scanner);

    assert!(scanner.comments().is_empty());
}

#[test]
fn scanner_collects_comments_after_syntax_elements() {
    let cases = [
        (
            "%YAML 1.2 # directive\n--- # document start\nvalue\n",
            &[" directive", " document start"][..],
        ),
        ("value\n... # document end\n", &[" document end"][..]),
        (
            "[a, # flow entry\n  'b' # quoted scalar\n] # flow end\n",
            &[" flow entry", " quoted scalar", " flow end"][..],
        ),
        (
            "[ # flow sequence start\n  a\n] # flow sequence end\n",
            &[" flow sequence start", " flow sequence end"][..],
        ),
        (
            "{ # flow mapping start\n  key: value\n} # flow mapping end\n",
            &[" flow mapping start", " flow mapping end"][..],
        ),
        ("- # block entry\n  value\n", &[" block entry"][..]),
        (
            "double: \"value\" # double quoted\nsingle: 'value' # single quoted\n",
            &[" double quoted", " single quoted"][..],
        ),
        ("key: | # block header\n  text\n", &[" block header"][..]),
        (
            "key: a\n\t# plain scalar tab line\nnext: b\n",
            &[" plain scalar tab line"][..],
        ),
        (
            "key:\t# mapping value\n  nested: value\n",
            &[" mapping value"][..],
        ),
    ];

    for (yaml, expected) in cases {
        let mut scanner = Scanner::new(StrInput::new(yaml)).with_comments();

        drain_scanner(&mut scanner);

        let texts: Vec<_> = scanner
            .comments()
            .iter()
            .map(|comment| comment.text.as_ref())
            .collect();
        assert_eq!(texts, expected);
    }
}

#[test]
fn scanner_collects_comment_after_explicit_key_whitespace() {
    let mut scanner = Scanner::new(StrInput::new("? # explicit key\n: value\n")).with_comments();

    drain_scanner(&mut scanner);

    assert_eq!(scanner.comments()[0].text, " explicit key");
}

#[test]
fn scanner_unseparated_comment_error_does_not_record_comment() {
    let mut scanner = Scanner::new(StrInput::new("key: \"value\"#bad\n")).with_comments();

    let error = loop {
        match scanner.next_token() {
            Ok(Some(_)) => {}
            Ok(None) => panic!("expected scanner error"),
            Err(error) => break error,
        }
    };

    assert_eq!(
        error.info(),
        "comments must be separated from other tokens by whitespace"
    );
    assert!(scanner.comments().is_empty());
}

#[test]
fn parser_does_not_collect_comments_by_default() {
    let mut parser = Parser::new_from_str("# comment\nkey: value\n");

    while parser.next_event().transpose().unwrap().is_some() {}

    assert!(parser.comments().is_empty());
}

#[test]
fn parser_with_comments_keeps_event_stream_unchanged() {
    let yaml = "# top\nkey: value # trailing\n";

    let expected: Vec<_> = Parser::new_from_str(yaml)
        .collect::<Result<_, _>>()
        .expect("baseline parse should succeed");
    let actual: Vec<_> = Parser::new_from_str(yaml)
        .with_comments()
        .collect::<Result<_, _>>()
        .expect("comment-enabled parse should succeed");

    assert_eq!(actual, expected);
}

#[test]
fn parser_collects_comments_after_full_parse() {
    let yaml = "# top\nkey: value # trailing\n";
    let mut parser = Parser::new_from_str(yaml).with_comments();

    while parser.next_event().transpose().unwrap().is_some() {}

    let comments = parser.comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].text, " top");
    assert_eq!(comments[0].span.slice(yaml), Some("# top"));
    assert_eq!(comments[1].text, " trailing");
    assert_eq!(comments[1].span.slice(yaml), Some("# trailing"));
}

#[test]
fn parser_take_comments_drains_collected_comments() {
    let mut parser = Parser::new_from_str("# first\nvalue # second\n").with_comments();

    while parser.next_event().transpose().unwrap().is_some() {}

    let comments = parser.take_comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].text, " first");
    assert_eq!(comments[1].text, " second");
    assert!(parser.comments().is_empty());
}

#[test]
fn parser_comments_are_available_with_load() {
    struct Sink(Vec<Event<'static>>);

    impl granit_parser::EventReceiver<'static> for Sink {
        fn on_event(&mut self, ev: Event<'static>) {
            self.0.push(ev);
        }
    }

    let mut parser =
        Parser::new_from_iter("# top\nkey: value # trailing\n".chars()).with_comments();
    let mut sink = Sink(Vec::new());

    parser
        .load(&mut sink, false)
        .expect("comment-enabled parse should succeed");

    assert_eq!(
        sink.0,
        vec![
            Event::StreamStart,
            Event::DocumentStart(false),
            Event::MappingStart(0, None),
            Event::Scalar("key".into(), ScalarStyle::Plain, 0, None),
            Event::Scalar("value".into(), ScalarStyle::Plain, 0, None),
            Event::MappingEnd,
            Event::DocumentEnd,
        ]
    );

    let comments = parser.comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].text, " top");
    assert_eq!(comments[1].text, " trailing");
}
