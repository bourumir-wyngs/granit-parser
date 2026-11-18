use granit_parser::{
    Event, Marker, Parser, ParserStack, ParserTrait, ReplayParser, ScalarStyle, SourceId, Span,
    StrInput,
};

#[test]
fn source_queries_borrow_names_and_preserve_distinct_keys_for_equal_names() {
    let mut stack: ParserStack<'_> = ParserStack::new();
    stack.push_str_parser_with_source_id(
        Parser::new_from_str("parent"),
        "same".into(),
        SourceId(1),
    );
    stack.push_str_parser_with_source_id(Parser::new_from_str("child"), "same".into(), SourceId(2));
    assert_eq!(stack.depth(), 2);
    assert_eq!(stack.current_source_id(), Some(SourceId(2)));
    let mut names = stack.source_names();
    assert_eq!(names.len(), 2);
    assert_eq!(names.next(), Some("same"));
    assert_eq!(names.next_back(), Some("same"));
    assert_eq!(names.len(), 0);
    assert_eq!(names.next_back(), None);
    assert_eq!(
        stack.source_ids().collect::<Vec<_>>(),
        [Some(SourceId(1)), Some(SourceId(2))]
    );
    assert_eq!(
        stack.source_ids().rev().collect::<Vec<_>>(),
        [Some(SourceId(2)), Some(SourceId(1))]
    );

    let mut scalars = Vec::new();
    let mut final_source = None;
    while let Some(sourced) = stack.next_sourced() {
        let (event, _) = sourced.value.unwrap();
        if let Event::Scalar { value, .. } = event {
            scalars.push((value.into_owned(), sourced.source_id));
        } else if matches!(event, Event::StreamEnd { .. }) {
            final_source = sourced.source_id;
        }
    }
    assert_eq!(
        scalars,
        [
            ("child".into(), Some(SourceId(2))),
            ("parent".into(), Some(SourceId(1)))
        ]
    );
    assert_eq!(final_source, Some(SourceId(1)));
    assert_eq!(stack.depth(), 0);
    assert_eq!(stack.current_source_id(), None);
    assert_eq!(stack.source_names().len(), 0);
}

#[test]
fn peeked_parent_event_keeps_its_key_when_another_source_is_pushed() {
    let mut stack: ParserStack<'_> = ParserStack::new();
    stack.push_str_parser_with_source_id(
        Parser::new_from_str("parent"),
        "parent".into(),
        SourceId(1),
    );
    assert!(matches!(
        stack.peek().unwrap().unwrap().0,
        Event::StreamStart { .. }
    ));
    stack.push_str_parser_with_source_id(
        Parser::new_from_str("child"),
        "child".into(),
        SourceId(2),
    );
    let first = stack.next_sourced().unwrap();
    assert_eq!(first.source_id, Some(SourceId(1)));
    assert!(matches!(first.value.unwrap().0, Event::StreamStart { .. }));
    let child = stack.next_sourced().unwrap();
    assert_eq!(child.source_id, Some(SourceId(2)));
    assert!(matches!(child.value.unwrap().0, Event::Scalar { value, .. } if value == "child"));
}

#[test]
fn peeked_error_keeps_failing_source_after_pop_and_push() {
    let mut stack: ParserStack<'_> = ParserStack::new();
    stack.push_str_parser_with_source_id(
        Parser::new_from_str("parent"),
        "same".into(),
        SourceId(1),
    );
    stack.push_str_parser_with_source_id(Parser::new_from_str("["), "same".into(), SourceId(2));
    while stack.peek().unwrap().is_ok() {
        stack.next().unwrap().unwrap();
    }
    assert_eq!(stack.depth(), 1);
    assert_eq!(stack.current_source_id(), Some(SourceId(1)));
    // An explicit current event must not overwrite the provenance of a cached error.
    stack.push_custom_parser_with_current_and_source_id(
        Parser::new(StrInput::new("new")),
        "replacement".into(),
        (
            Event::new_scalar("primed".into(), ScalarStyle::plain(), 0, None),
            Span::empty(Marker::new(0, 1, 0)),
        ),
        SourceId(3),
    );
    let failed = stack.next_sourced().unwrap();
    assert_eq!(failed.source_id, Some(SourceId(2)));
    let error = failed.value.unwrap_err();
    assert_eq!(error.source_stack(), ["same", "same"]);
}

#[test]
fn custom_current_event_and_subsequent_events_share_the_supplied_key() {
    let mut stack: ParserStack<'_> = ParserStack::new();
    let mut parser = Parser::new(StrInput::new("value"));
    let current = parser.next().unwrap().unwrap();
    stack.push_custom_parser_with_current_and_source_id(
        parser,
        "custom".into(),
        current,
        SourceId(8),
    );
    assert!(stack.peek().unwrap().is_ok());
    let mut events = 0;
    while let Some(sourced) = stack.next_sourced() {
        assert_eq!(sourced.source_id, Some(SourceId(8)));
        sourced.value.unwrap();
        events += 1;
    }
    assert_eq!(events, 5);
}

#[test]
fn replay_without_end_marker_keeps_key_for_synthesized_stream_end() {
    let mut stack: ParserStack<'_> = ParserStack::new();
    stack.push_replay_parser_with_source_id(
        ReplayParser::new(
            vec![(
                Event::new_scalar("value".into(), ScalarStyle::plain(), 0, None),
                Span::empty(Marker::new(0, 1, 0)),
            )],
            0,
        ),
        "replay".into(),
        SourceId(9),
    );
    assert_eq!(stack.next_sourced().unwrap().source_id, Some(SourceId(9)));
    let end = stack.next_sourced().unwrap();
    assert_eq!(end.source_id, Some(SourceId(9)));
    assert!(matches!(end.value.unwrap().0, Event::StreamEnd { .. }));
    assert_eq!(stack.depth(), 0);
    assert!(stack.next_sourced().is_none());
}

#[test]
fn iterator_parser_results_keep_their_key() {
    let text = String::from("value");
    let mut stack: ParserStack<'_, _> = ParserStack::new();
    stack.push_iter_parser_with_source_id(
        Parser::new_from_iter(text.chars()),
        "iter".into(),
        SourceId(10),
    );
    while let Some(sourced) = stack.next_sourced() {
        assert_eq!(sourced.source_id, Some(SourceId(10)));
        sourced.value.unwrap();
    }
}

#[test]
fn resolver_results_and_eager_errors_keep_supplied_keys() {
    let mut stack: ParserStack<'_> = ParserStack::new();
    stack.set_resolver(|name| Ok(if name == "bad" { "[" } else { "value" }.to_owned()));
    let error = stack
        .push_include_with_source_id("bad", SourceId(11))
        .unwrap_err();
    assert_eq!(error.source_id, Some(SourceId(11)));
    assert_eq!(error.value.source_stack(), ["bad"]);
    assert_eq!(stack.depth(), 0);
    stack
        .push_include_with_source_id("good", SourceId(12))
        .unwrap();
    while let Some(sourced) = stack.next_sourced() {
        assert_eq!(sourced.source_id, Some(SourceId(12)));
        sourced.value.unwrap();
    }
}

#[test]
fn ordinary_pushes_keep_legacy_results_and_have_no_source_key() {
    let mut stack: ParserStack<'_> = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("value"), "legacy".into());
    assert_eq!(stack.source_ids().collect::<Vec<_>>(), [None]);
    assert!(matches!(
        stack.next().unwrap().unwrap().0,
        Event::StreamStart { .. }
    ));
    while let Some(sourced) = stack.next_sourced() {
        assert_eq!(sourced.source_id, None);
        sourced.value.unwrap();
    }
}
