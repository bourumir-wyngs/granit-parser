use std::borrow::Cow;

use granit_parser::{
    ErrorKind, Event, Options, Parser, ParserStack, ScalarStyle, Scanner, Span,
    SpannedEventReceiver, StrInput, StructureStyle,
};

fn tooling_options() -> Options {
    granit_parser::options! { emit_unresolved_aliases: true }
}

#[test]
fn ordinary_parsing_and_explicit_false_still_reject_unknown_anchors() {
    let yaml = "first: *missing\nsecond: later\n";
    for mut parser in [
        Parser::new_from_str(yaml),
        Parser::new_from_str_with_options(
            yaml,
            granit_parser::options! { emit_unresolved_aliases: false },
        ),
    ] {
        let mut saw_later_node = false;
        let error = loop {
            match parser.next().expect("unknown anchor must be reported") {
                Ok((Event::Scalar(value, ..), _)) => saw_later_node |= value == "later",
                Ok((Event::UnresolvedAlias(_), _)) => {
                    panic!("strict parsing emitted an unresolved alias")
                }
                Ok(_) => {}
                Err(error) => break error,
            }
        };
        assert_eq!(error.kind(), &ErrorKind::UnknownAnchor);
        assert_eq!((error.marker().line(), error.marker().col()), (1, 7));
        assert!(!saw_later_node);
        assert!(parser.next().is_none());
        assert!(parser.peek().is_none());
    }
}

#[test]
fn unresolved_alias_preserves_name_span_and_following_structure() {
    let yaml = "first: *míssing\nsecond: [one, two]\nthird: \"hello\"\n";
    let events = Parser::new_from_str_with_options(yaml, tooling_options())
        .collect::<Result<Vec<_>, _>>()
        .expect("unresolved references should not stop structural parsing");

    assert_eq!(
        events
            .iter()
            .map(|(event, _)| event.clone())
            .collect::<Vec<_>>(),
        [
            Event::StreamStart,
            Event::DocumentStart(false, None),
            Event::MappingStart(StructureStyle::Block, 0, None),
            Event::Scalar("first".into(), ScalarStyle::Plain, 0, None),
            Event::UnresolvedAlias("míssing".into()),
            Event::Scalar("second".into(), ScalarStyle::Plain, 0, None),
            Event::SequenceStart(StructureStyle::Flow, 0, None),
            Event::Scalar("one".into(), ScalarStyle::Plain, 0, None),
            Event::Scalar("two".into(), ScalarStyle::Plain, 0, None),
            Event::SequenceEnd,
            Event::Scalar("third".into(), ScalarStyle::Plain, 0, None),
            Event::Scalar("hello".into(), ScalarStyle::DoubleQuoted, 0, None),
            Event::MappingEnd,
            Event::DocumentEnd,
            Event::StreamEnd,
        ]
    );
    let (alias, span) = &events[4];
    assert!(matches!(
        alias,
        Event::UnresolvedAlias(Cow::Borrowed("míssing"))
    ));
    assert!(alias.is_alias());
    assert!(alias.is_node());
    assert_eq!(alias.alias_id(), None);
    assert_eq!(alias.anchor_id(), None);
    assert_eq!(span.slice(yaml), Some("*míssing"));
    assert_eq!(span.start.index(), 7);
    assert_eq!(span.end.index(), 15);
    assert_eq!((span.start.line(), span.start.col()), (1, 7));
    assert_eq!((span.end.line(), span.end.col()), (1, 15));
    assert_eq!(span.byte_range(), Some(7..16));
    assert!(!events[3].0.is_alias());
}

#[test]
fn unresolved_aliases_complete_nodes_in_all_grammar_positions() {
    for yaml in [
        "*missing\n",
        "- *missing\n- later\n",
        "entries:\n- *missing\nnext: later\n",
        "[*missing, later]\n",
        "{*missing : later}\n",
        "{key: *missing, next: later}\n",
        "[*missing : later]\n",
        "[key: *missing, later]\n",
        "? *missing\n: later\n",
    ] {
        let events = Parser::new_from_str_with_options(yaml, tooling_options())
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| panic!("structural parsing failed for {yaml:?}: {error:?}"));
        assert_eq!(
            events.iter().filter(|(event, _)| event.is_alias()).count(),
            1,
            "{yaml:?}",
        );
        assert!(
            matches!(events.last(), Some((Event::StreamEnd, _))),
            "{yaml:?}"
        );
    }
}

#[test]
fn unresolved_mapping_key_consumes_its_indentation_hint() {
    let yaml = "outer:\n  *missing : value\n  next: later\n";
    let events = Parser::new_from_str_with_options(yaml, tooling_options())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let alias = events
        .iter()
        .position(|(event, _)| event.is_alias())
        .unwrap();
    assert_eq!(events[alias].1.indent, Some(2));
    assert!(matches!(&events[alias + 1].0, Event::Scalar(value, ..) if value == "value"));
    assert_eq!(events[alias + 1].1.indent, None);
    assert_eq!(events[alias + 2].1.indent, Some(2));
}

#[test]
fn repeated_names_resolve_only_to_preceding_definitions_and_do_not_allocate_ids() {
    let yaml = "[*x, *x, &x one, *x, &x two, *x]\n";
    let mut parser = Parser::new_from_str_with_options(yaml, tooling_options());
    parser.set_anchor_offset(23);
    let events = parser.by_ref().collect::<Result<Vec<_>, _>>().unwrap();
    let aliases = events
        .iter()
        .filter(|(event, _)| event.is_alias())
        .collect::<Vec<_>>();

    assert_eq!(
        aliases
            .iter()
            .map(|(event, _)| event.clone())
            .collect::<Vec<_>>(),
        [
            Event::UnresolvedAlias("x".into()),
            Event::UnresolvedAlias("x".into()),
            Event::Alias(23),
            Event::Alias(24),
        ],
    );
    assert_ne!(aliases[0].1.start, aliases[1].1.start);
    assert_eq!(aliases[2].0.alias_id(), Some(23));
    assert!(aliases[2].0.is_node());
    assert_eq!(parser.anchor_offset(), 25);
}

#[test]
fn document_boundaries_clear_names_and_self_references_still_resolve() {
    let yaml = "---\n[&x first, *x]\n---\n[*x, &x [*x], *x]\n";
    let events = Parser::new_from_str_with_options(yaml, tooling_options())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        events
            .into_iter()
            .map(|(event, _)| event)
            .filter(Event::is_alias)
            .collect::<Vec<_>>(),
        [
            Event::Alias(1),
            Event::UnresolvedAlias("x".into()),
            Event::Alias(2),
            Event::Alias(2),
        ],
    );
}

#[derive(Default)]
struct Events<'input>(Vec<(Event<'input>, Span)>);

impl<'input> SpannedEventReceiver<'input> for Events<'input> {
    fn on_event(&mut self, event: Event<'input>, span: Span) {
        self.0.push((event, span));
    }
}

#[test]
fn input_backends_peek_and_load_preserve_the_event_stream() {
    let yaml = "# before\nfirst: *missing # alias\nsecond: [one, two]\n";
    for emit_comments in [false, true] {
        let options = granit_parser::options! {
            emit_unresolved_aliases: true,
            emit_comments: emit_comments,
        };
        let expected = Parser::new_from_str_with_options(yaml, options.clone())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for events in [
            Parser::new_from_iter_with_options(yaml.chars(), options.clone())
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            Parser::new_from_fallible_iter_with_options(yaml.chars().map(Ok), options.clone())
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        ] {
            assert_eq!(events, expected);
            let (event, span) = events.iter().find(|(event, _)| event.is_alias()).unwrap();
            assert!(matches!(event, Event::UnresolvedAlias(Cow::Owned(name)) if name == "missing"));
            assert_eq!((span.start.line(), span.start.col()), (2, 7));
        }

        let mut parser = Parser::new_from_str_with_options(yaml, options.clone());
        let mut prefix = Vec::new();
        loop {
            let peeked = parser.peek().unwrap().unwrap().clone();
            assert_eq!(parser.peek().unwrap().unwrap(), &peeked);
            if peeked.0.is_alias() {
                break;
            }
            prefix.push(parser.next().unwrap().unwrap());
        }
        let mut sink = Events::default();
        parser.load(&mut sink, true).unwrap();
        prefix.extend(sink.0);
        assert_eq!(
            prefix, expected,
            "load must include the peeked unresolved alias once"
        );
        assert!(parser.next().is_none());

        let mut parser = Parser::new_from_str_with_options(yaml, options);
        let mut through_peek = Vec::new();
        while let Some(result) = parser.peek() {
            let peeked = result.unwrap().clone();
            assert_eq!(parser.peek().unwrap().unwrap(), &peeked);
            assert_eq!(parser.next().unwrap().unwrap(), peeked);
            through_peek.push(peeked);
        }
        assert_eq!(through_peek, expected);
    }
}

#[test]
fn scanner_clean_input_can_still_have_a_terminal_grammar_error_after_an_alias() {
    let yaml = "[*missing value]\n";
    Scanner::new(StrInput::new(yaml))
        .collect::<Result<Vec<_>, _>>()
        .expect("this regression must exercise grammar validation, not a scanner error");
    let mut parser = Parser::new_from_str_with_options(yaml, tooling_options());
    let mut saw_alias = false;
    let error = loop {
        match parser
            .peek()
            .expect("missing sequence separator must be reported")
        {
            Ok((event, _)) => saw_alias |= matches!(event, Event::UnresolvedAlias(_)),
            Err(error) => break error,
        }
        parser.next().unwrap().unwrap();
    };
    assert!(saw_alias);
    assert_eq!(error.kind(), &ErrorKind::ExpectedFlowSequenceSeparator);
    assert_eq!(parser.peek().unwrap().unwrap_err(), error);
    assert_eq!(parser.next().unwrap().unwrap_err(), error);
    assert!(parser.next().is_none());
    assert!(parser.peek().is_none());
}

#[test]
fn later_scanner_errors_remain_terminal() {
    // Separate lines let scanner lookahead expose the alias before reaching the invalid escape.
    let yaml = "first: *missing\nsecond: one\nthird: \"\\q\"\n";
    let mut parser = Parser::new_from_str_with_options(yaml, tooling_options());
    let mut saw_alias = false;
    let error = loop {
        match parser.next().expect("invalid escape must be reported") {
            Ok((event, _)) => saw_alias |= matches!(event, Event::UnresolvedAlias(_)),
            Err(error) => break error,
        }
    };
    assert!(saw_alias);
    assert_eq!(error.kind(), &ErrorKind::UnknownQuotedScalarEscape);
    assert!(parser.next().is_none());
    assert!(parser.peek().is_none());

    let mut sink = Events::default();
    let load_error = Parser::new_from_str_with_options(yaml, tooling_options())
        .load(&mut sink, true)
        .unwrap_err();
    assert_eq!(load_error, error);
    assert!(sink.0.iter().any(|(event, _)| event.is_alias()));
    assert!(!sink
        .0
        .iter()
        .any(|(event, _)| matches!(event, Event::StreamEnd)));
}

#[test]
fn alias_syntax_and_input_errors_are_not_suppressed() {
    let error = Parser::new_from_str_with_options("[*missing, *]\n", tooling_options())
        .find_map(Result::err)
        .expect("an alias still needs a name");
    assert_eq!(error.kind(), &ErrorKind::MissingAnchorOrAliasName);

    let source_error = ErrorKind::InputDecoding {
        message: "truncated input".into(),
    };
    let input = "[*missing, one, "
        .chars()
        .map(Ok)
        .chain(std::iter::once(Err(source_error.clone())))
        .chain(std::iter::from_fn(|| {
            panic!("input polled after source error")
        }));
    let mut parser = Parser::new_from_fallible_iter_with_options(input, tooling_options());
    let error = parser
        .by_ref()
        .find_map(Result::err)
        .expect("source error must be retained");
    assert_eq!(error.kind(), &source_error);
    assert!(parser.next().is_none());
}

#[test]
fn include_options_preserve_unresolved_events_through_replay() {
    const INCLUDED: &str = "first: *missing\nsecond: &x later\nthird: *x\n";
    for borrowed in [false, true] {
        let mut stack: ParserStack<'_> = ParserStack::with_options(tooling_options());
        if borrowed {
            stack.set_borrowed_resolver(|_| Ok(INCLUDED));
        } else {
            stack.set_resolver(|_| Ok(INCLUDED.into()));
        }
        stack.push_include("included.yaml").unwrap();
        let events = stack.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            events
                .iter()
                .map(|(event, _)| event.clone())
                .filter(Event::is_alias)
                .collect::<Vec<_>>(),
            [Event::UnresolvedAlias("missing".into()), Event::Alias(1)],
        );
        assert!(matches!(events.last(), Some((Event::StreamEnd, _))));
    }

    let mut strict: ParserStack<'_> = ParserStack::new();
    strict.set_borrowed_resolver(|_| Ok(INCLUDED));
    assert_eq!(
        strict.push_include("included.yaml").unwrap_err().kind(),
        &ErrorKind::UnknownAnchor
    );
}
