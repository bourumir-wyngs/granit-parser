use std::borrow::Cow;

use granit_parser::{
    BufferedInput, Comment, Event, EventReceiver, Marker, Parser, ScalarStyle, ScanError, Scanner,
    Span, StrInput, Token, TokenType, TryEventReceiver,
};

fn parser_events(source: &str) -> Result<Vec<(Event<'_>, Span)>, ScanError> {
    Parser::new_from_str(source).collect()
}

#[test]
fn comment_type_is_a_convenience_container() {
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
fn comment_type_preserves_single_space_payload() {
    let span = Span::new(
        Marker::new(0, 1, 0).with_byte_offset(Some(0)),
        Marker::new(2, 1, 2).with_byte_offset(Some(2)),
    );
    let comment = Comment::new(span, " ");

    assert_eq!(comment.text, " ");
    assert_eq!(comment.trimmed_text(), "");
}

#[test]
fn event_comment_stores_raw_payload() {
    let event = Event::Comment(" payload ".into());

    assert_eq!(event, Event::Comment(" payload ".into()));
    assert!(!event.is_node());
    assert_eq!(event.scalar(), None);
    assert_eq!(event.tag(), None);
    assert_eq!(event.anchor_id(), None);
    assert_eq!(event.alias_id(), None);
}

#[test]
fn token_comment_uses_span_for_full_source_comment() {
    let yaml = "key: value # payload\r\n";
    let span = Span::new(
        Marker::new(11, 1, 11).with_byte_offset(Some(11)),
        Marker::new(20, 1, 20).with_byte_offset(Some(20)),
    );
    let token = Token(span, TokenType::Comment(" payload".into()));

    assert_eq!(token.0.slice(yaml), Some("# payload"));
    assert!(matches!(token.1, TokenType::Comment(ref text) if text == " payload"));
}

#[test]
fn scanner_emits_comment_tokens_in_source_order() {
    let yaml = "# top\n  # indented\nkey: value # trailing\n#eof";
    let tokens = Scanner::new(StrInput::new(yaml)).collect::<Vec<Token<'_>>>();

    let comments: Vec<_> = tokens
        .iter()
        .filter_map(|Token(span, token)| match token {
            TokenType::Comment(text) => Some((text.as_ref(), span.slice(yaml))),
            _ => None,
        })
        .collect();

    assert_eq!(
        comments,
        vec![
            (" top", Some("# top")),
            (" indented", Some("# indented")),
            (" trailing", Some("# trailing")),
            ("eof", Some("#eof")),
        ]
    );
}

#[test]
fn scanner_emits_trailing_comment_after_plain_scalar_token() {
    let tokens = Scanner::new(StrInput::new("key: value # trailing\n")).collect::<Vec<Token<'_>>>();

    let value_index = tokens
        .iter()
        .position(|Token(_, token)| {
            matches!(token, TokenType::Scalar(ScalarStyle::Plain, value) if value == "value")
        })
        .expect("plain scalar token should be emitted");
    let comment_index = tokens
        .iter()
        .position(
            |Token(_, token)| matches!(token, TokenType::Comment(text) if text == " trailing"),
        )
        .expect("comment token should be emitted");

    assert!(value_index < comment_index);
}

#[test]
fn scanner_emits_comments_after_leading_syntax_tokens() {
    struct Case<'input> {
        yaml: &'input str,
        syntax_matches: fn(&TokenType<'_>) -> bool,
        expected_comment: &'input str,
    }

    let cases = [
        Case {
            yaml: "%YAML 1.2 # directive\n---\n",
            syntax_matches: |token| matches!(token, TokenType::VersionDirective(1, 2)),
            expected_comment: " directive",
        },
        Case {
            yaml: "[ # flow start\n]\n",
            syntax_matches: |token| matches!(token, TokenType::FlowSequenceStart),
            expected_comment: " flow start",
        },
        Case {
            yaml: "? # explicit key\n: value\n",
            syntax_matches: |token| matches!(token, TokenType::Key),
            expected_comment: " explicit key",
        },
        Case {
            yaml: "key: \"value\" # quoted\n",
            syntax_matches: |token| matches!(token, TokenType::Scalar(ScalarStyle::DoubleQuoted, value) if value == "value"),
            expected_comment: " quoted",
        },
        Case {
            yaml: "key:\t# mapping value\n  nested: value\n",
            syntax_matches: |token| matches!(token, TokenType::Value),
            expected_comment: " mapping value",
        },
    ];

    for case in cases {
        let tokens = Scanner::new(StrInput::new(case.yaml)).collect::<Vec<Token<'_>>>();
        let syntax_index = tokens
            .iter()
            .position(|Token(_, token)| (case.syntax_matches)(token))
            .expect("syntax token should be emitted");
        let comment_index = tokens
            .iter()
            .position(|Token(_, token)| {
                matches!(token, TokenType::Comment(text) if text == case.expected_comment)
            })
            .expect("comment token should be emitted");

        assert!(syntax_index < comment_index, "{:?}", case.yaml);
    }
}

#[test]
fn scanner_preserves_empty_comment_payloads() {
    let yaml = "#\n# \n";
    let comments: Vec<_> = Scanner::new(StrInput::new(yaml))
        .filter_map(|Token(span, token)| match token {
            TokenType::Comment(text) => {
                Some((text.into_owned(), span.slice(yaml).map(str::to_owned)))
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        comments,
        vec![
            (String::new(), Some("#".into())),
            (" ".into(), Some("# ".into()))
        ]
    );
}

#[test]
fn scanner_comment_span_stops_before_crlf() {
    let yaml = "# crlf\r\nkey: value\n";
    let comment = Scanner::new(StrInput::new(yaml))
        .find_map(|Token(span, token)| match token {
            TokenType::Comment(text) => Some((text.into_owned(), span)),
            _ => None,
        })
        .expect("comment token should be emitted");

    assert_eq!(comment.0, " crlf");
    assert_eq!(comment.1.slice(yaml), Some("# crlf"));
}

#[test]
fn scanner_comment_text_is_borrowed_for_str_input() {
    let comment = Scanner::new(StrInput::new("# borrowed\n"))
        .find_map(|Token(_, token)| match token {
            TokenType::Comment(text) => Some(text),
            _ => None,
        })
        .expect("comment token should be emitted");

    match comment {
        Cow::Borrowed(text) => assert_eq!(text, " borrowed"),
        Cow::Owned(text) => panic!("expected borrowed comment text, got {text:?}"),
    }
}

#[test]
fn scanner_comment_text_is_owned_for_buffered_input() {
    let comment = Scanner::new(BufferedInput::new("# streamed\n".chars()))
        .find_map(|Token(_, token)| match token {
            TokenType::Comment(text) => Some(text),
            _ => None,
        })
        .expect("comment token should be emitted");

    match comment {
        Cow::Owned(text) => assert_eq!(text, " streamed"),
        Cow::Borrowed(text) => panic!("expected owned comment text, got {text:?}"),
    }
}

#[test]
fn scanner_does_not_emit_unseparated_comment_after_quoted_scalar_error() {
    let mut scanner = Scanner::new(StrInput::new("key: \"value\"#bad\n"));
    let mut saw_comment = false;

    let error = loop {
        match scanner.next_token() {
            Ok(Some(Token(_, TokenType::Comment(_)))) => saw_comment = true,
            Ok(Some(_)) => {}
            Ok(None) => panic!("expected scanner error"),
            Err(error) => break error,
        }
    };

    assert_eq!(
        error.info(),
        "comments must be separated from other tokens by whitespace"
    );
    assert!(!saw_comment);
}

#[test]
fn parser_emits_full_line_indented_and_trailing_comment_events() {
    let yaml = "# top\n  # indented\nkey: value # trailing\n#eof";
    let events = parser_events(yaml).expect("parser should accept comments");

    let comments: Vec<_> = events
        .iter()
        .filter_map(|(event, span)| match event {
            Event::Comment(text) => Some((text.as_ref(), span.slice(yaml))),
            _ => None,
        })
        .collect();

    assert_eq!(
        comments,
        vec![
            (" top", Some("# top")),
            (" indented", Some("# indented")),
            (" trailing", Some("# trailing")),
            ("eof", Some("#eof")),
        ]
    );
}

#[test]
fn parser_emits_trailing_comment_after_plain_scalar_event() {
    let events = parser_events("key: value # trailing\n").expect("parser should emit events");

    let value_index = events
        .iter()
        .position(|(event, _)| {
            matches!(event, Event::Scalar(value, ScalarStyle::Plain, ..) if value == "value")
        })
        .expect("plain scalar event should be emitted");
    let comment_index = events
        .iter()
        .position(|(event, _)| matches!(event, Event::Comment(text) if text == " trailing"))
        .expect("comment event should be emitted");

    assert!(value_index < comment_index);
}

#[test]
fn parser_preserves_empty_comment_payloads_and_crlf_span() {
    let yaml = "#\r\n# \n";
    let events = parser_events(yaml).expect("parser should accept empty comments");

    let comments: Vec<_> = events
        .iter()
        .filter_map(|(event, span)| match event {
            Event::Comment(text) => Some((text.as_ref(), span.slice(yaml))),
            _ => None,
        })
        .collect();

    assert_eq!(comments, vec![("", Some("#")), (" ", Some("# "))]);
}

#[test]
fn parser_peek_returns_and_preserves_pending_comment_event() {
    let mut parser = Parser::new_from_str("# first\nkey: value\n");

    assert!(matches!(
        parser.next_event().unwrap().unwrap().0,
        Event::StreamStart
    ));

    let first_peek = parser.peek().unwrap().unwrap().clone();
    let second_peek = parser.peek().unwrap().unwrap().clone();
    let next = parser.next_event().unwrap().unwrap();

    assert!(matches!(first_peek.0, Event::Comment(ref text) if text == " first"));
    assert_eq!(first_peek, second_peek);
    assert_eq!(first_peek, next);
}

#[derive(Default)]
struct CommentSink<'input> {
    comments: Vec<Cow<'input, str>>,
}

impl<'input> EventReceiver<'input> for CommentSink<'input> {
    fn on_event(&mut self, ev: Event<'input>) {
        if let Event::Comment(text) = ev {
            self.comments.push(text);
        }
    }
}

impl<'input> TryEventReceiver<'input> for CommentSink<'input> {
    type Error = ();

    fn on_event(&mut self, ev: Event<'input>) -> Result<(), Self::Error> {
        if let Event::Comment(text) = ev {
            self.comments.push(text);
        }
        Ok(())
    }
}

#[test]
fn parser_load_and_try_load_deliver_comment_events() {
    let mut load_parser = Parser::new_from_str("# load\nkey: value\n");
    let mut load_sink = CommentSink::default();
    load_parser
        .load(&mut load_sink, true)
        .expect("load should deliver comments");

    let mut try_load_parser = Parser::new_from_str("# try\nkey: value\n");
    let mut try_load_sink = CommentSink::default();
    try_load_parser
        .try_load(&mut try_load_sink, true)
        .expect("try_load should deliver comments");

    assert_eq!(load_sink.comments, vec![Cow::Borrowed(" load")]);
    assert_eq!(try_load_sink.comments, vec![Cow::Borrowed(" try")]);
}

#[test]
fn parser_emits_comments_around_markers_flow_collections_and_stream_end() {
    let yaml = "# before doc\n--- # after start\n[ # after flow start\n  a, # after entry\n  b\n] # after flow end\n... # after end\n# before stream end\n";
    let events = parser_events(yaml).expect("parser should emit comments in source order");

    let names: Vec<String> = events
        .iter()
        .filter_map(|(event, _)| match event {
            Event::StreamStart => Some("StreamStart".into()),
            Event::DocumentStart(_) => Some("DocumentStart".into()),
            Event::SequenceStart(..) => Some("SequenceStart".into()),
            Event::SequenceEnd => Some("SequenceEnd".into()),
            Event::DocumentEnd => Some("DocumentEnd".into()),
            Event::StreamEnd => Some("StreamEnd".into()),
            Event::Scalar(value, ..) => Some(format!("Scalar({value})")),
            Event::Comment(text) => Some(format!("Comment({text})")),
            Event::Nothing | Event::Alias(_) | Event::MappingStart(..) | Event::MappingEnd => None,
        })
        .collect();

    assert_eq!(
        names,
        vec![
            "StreamStart",
            "Comment( before doc)",
            "DocumentStart",
            "Comment( after start)",
            "SequenceStart",
            "Comment( after flow start)",
            "Scalar(a)",
            "Comment( after entry)",
            "Scalar(b)",
            "SequenceEnd",
            "Comment( after flow end)",
            "DocumentEnd",
            "Comment( after end)",
            "Comment( before stream end)",
            "StreamEnd",
        ]
    );
}

#[test]
fn parser_keeps_comment_events_out_of_mapping_state_and_node_properties() {
    let yaml = "? # key\n: &a # anchor\n  value\nref: *a # alias\n";
    let events =
        parser_events(yaml).expect("parser should preserve comments around mapping syntax");

    assert!(events
        .iter()
        .any(|(event, _)| matches!(event, Event::Comment(text) if text == " key")));
    assert!(events
        .iter()
        .any(|(event, _)| matches!(event, Event::Comment(text) if text == " anchor")));
    assert!(events
        .iter()
        .any(|(event, _)| matches!(event, Event::Comment(text) if text == " alias")));

    let anchored_value = events
        .iter()
        .find_map(|(event, _)| match event {
            Event::Scalar(value, _, anchor_id, _) if value == "value" => Some(*anchor_id),
            _ => None,
        })
        .expect("anchored scalar should be emitted");

    assert_ne!(anchored_value, 0);
    assert!(events
        .iter()
        .any(|(event, _)| matches!(event, Event::Alias(alias_id) if *alias_id == anchored_value)));
}
