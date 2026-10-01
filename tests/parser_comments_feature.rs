#![cfg(not(feature = "parser-comments"))]

use granit_parser::{
    BufferedInput, ErrorKind, Event, FallibleBufferedInput, Marker, Options, Parser, ParserStack,
    Placement, ScalarStyle, Scanner, Span, StrInput, TokenType,
};

#[test]
fn default_parsers_suppress_comments_for_all_input_backends() {
    for yaml in [
        "# header\nroot: # empty\nnext: value # trailing\n",
        "root:\n- # empty entry\n- value # trailing\n",
        "{root: # empty flow value\n, next: value}\n",
        "--- # document start\nvalue\n... # document end\n# tail\n",
    ] {
        // Keeping the runtime option true also proves that it cannot enable an excluded feature.
        let options = granit_parser::options! { max_buffered_comment_events: 0 };
        assert!(options.emit_comments);
        let string = Parser::new_from_str_with_options(yaml, options.clone())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let buffered = Parser::new_from_iter_with_options(yaml.chars(), options.clone())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let fallible = Parser::new_from_fallible_iter_with_options(
            yaml.chars().map(Ok::<char, ErrorKind>),
            options,
        )
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        assert_eq!(string, buffered, "{yaml:?}");
        assert_eq!(string, fallible, "{yaml:?}");
        assert!(string
            .iter()
            .all(|(event, _)| !matches!(event, Event::Comment(..))));
        assert!(string
            .iter()
            .any(|(event, _)| { matches!(event, Event::Scalar(value, ..) if value == "value") }));
        let suppressed = Parser::new_from_str_with_options(
            yaml,
            granit_parser::options! { emit_comments: false },
        )
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        assert_eq!(string, suppressed, "{yaml:?}");
    }
}

#[test]
fn excluded_parser_comments_preserve_syntax_validation() {
    for (yaml, expected) in [
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
            "key: value # control \u{1}\n",
            ErrorKind::UnexpectedCharacter { character: '\u{1}' },
        ),
    ] {
        let string = Parser::new_from_str(yaml).find_map(Result::err).unwrap();
        let buffered = Parser::new_from_iter(yaml.chars())
            .find_map(Result::err)
            .unwrap();
        let fallible = Parser::new_from_fallible_iter(yaml.chars().map(Ok::<char, ErrorKind>))
            .find_map(Result::err)
            .unwrap();
        assert_eq!(string.kind(), &expected, "{yaml:?}");
        assert_eq!(string, buffered, "{yaml:?}");
        assert_eq!(string, fallible, "{yaml:?}");
    }
}

#[test]
fn standalone_scanners_still_capture_comments_for_all_input_backends() {
    let yaml = "# header\nkey: value # inline\n";
    let string = Scanner::new(StrInput::new(yaml))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let buffered = Scanner::new(BufferedInput::new(yaml.chars()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let fallible = Scanner::new(FallibleBufferedInput::new(
        yaml.chars().map(Ok::<char, ErrorKind>),
    ))
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(string, buffered);
    assert_eq!(string, fallible);
    assert_eq!(
        string
            .iter()
            .filter(|token| matches!(token.token_type(), TokenType::Comment(_)))
            .count(),
        2
    );
}

#[test]
fn default_stack_suppresses_current_and_later_custom_comments() {
    type Stack = ParserStack<'static, core::iter::Empty<char>, StrInput<'static>>;
    let span = Span::empty(Marker::new(0, 1, 0));
    let current = (Event::Comment(" current".into(), Placement::Free), span);
    let mut parser = Parser::new_from_str("# later\nvalue\n");
    assert_eq!(parser.next_event().unwrap().unwrap().0, Event::StreamStart);
    assert!(matches!(
        parser.next_event().unwrap().unwrap().0,
        Event::DocumentStart(..)
    ));
    let mut stack = Stack::with_options(Options::default());
    stack.push_custom_parser_with_current(parser, "custom".into(), current);
    let events = stack.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        events
            .into_iter()
            .map(|(event, _)| event)
            .collect::<Vec<_>>(),
        [
            Event::Scalar("value".into(), ScalarStyle::Plain, 0, None),
            Event::DocumentEnd,
            Event::StreamEnd,
        ]
    );
}
