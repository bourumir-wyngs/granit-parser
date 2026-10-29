use granit_parser::{options, Event, Marker, Parser, ParserStack, ParserTrait, ReplayParser, Span};
use std::borrow::Cow;

#[test]
fn replay_tracks_anchor_metadata_ids_without_reassigning_them() {
    let span = Span::empty(Marker::new(0, 1, 0));
    // A replay can start after the defining node was consumed elsewhere.
    let events = vec![
        (Event::Anchor(7, "name".into()), span),
        (Event::Alias(99), span),
        (Event::Anchor(0, "unassigned".into()), span),
    ];
    let mut replay = ReplayParser::new(events.clone(), 2);

    assert_eq!(replay.peek().unwrap().unwrap(), &events[0]);
    assert_eq!(replay.anchor_offset(), 2);
    assert_eq!(replay.next().unwrap().unwrap(), events[0]);
    assert_eq!(replay.anchor_offset(), 8);
    assert_eq!(replay.next().unwrap().unwrap(), events[1]);
    assert_eq!(replay.next().unwrap().unwrap(), events[2]);
    assert_eq!(replay.anchor_offset(), 8);
    assert!(replay.next().is_none());
}

#[test]
fn stack_preserves_caller_parser_and_replay_anchor_events() {
    let source = "&name value";
    let events = Parser::new_from_str_with_options(source, options! { emit_anchor_events: true })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(events
        .iter()
        .any(|(event, _)| matches!(event, Event::Anchor(1, name) if name == "name")));

    let mut stack: ParserStack<'_> = ParserStack::new();
    stack.push_str_parser(
        Parser::new_from_str_with_options(source, options! { emit_anchor_events: true }),
        "caller.yaml".into(),
    );
    assert_eq!(stack.collect::<Result<Vec<_>, _>>().unwrap(), events);

    let mut replay_stack: ParserStack<'_> = ParserStack::new();
    replay_stack.push_replay_parser(ReplayParser::new(events.clone(), 1), "replay.yaml".into());
    assert_eq!(replay_stack.collect::<Result<Vec<_>, _>>().unwrap(), events);
}

#[test]
fn stack_does_not_enable_metadata_on_caller_supplied_parsers() {
    let mut stack: ParserStack<'_> =
        ParserStack::with_options(options! { emit_anchor_events: true });
    stack.push_str_parser(Parser::new_from_str("&name value"), "caller.yaml".into());
    let events = stack.collect::<Result<Vec<_>, _>>().unwrap();
    assert!(!events
        .iter()
        .any(|(event, _)| matches!(event, Event::Anchor(..))));
}

#[test]
fn included_anchor_events_follow_options_with_local_ranges_and_inherited_ids() {
    const INCLUDED: &str = "&inc [*inc]";

    for borrowed in [false, true] {
        for enabled in [false, true] {
            let mut stack: ParserStack<'_> =
                ParserStack::with_options(options! { emit_anchor_events: enabled });
            if borrowed {
                stack.set_borrowed_resolver(|_| Ok(INCLUDED));
            } else {
                stack.set_resolver(|_| Ok(INCLUDED.to_owned()));
            }
            stack.push_str_parser(
                Parser::new_from_str("before: &parent value\nafter: &after value\n"),
                "parent.yaml".into(),
            );
            loop {
                let (event, _) = stack.next().unwrap().unwrap();
                if event.anchor_id() == Some(1) {
                    break;
                }
            }

            stack.push_include("included.yaml").unwrap();
            let events = stack.collect::<Result<Vec<_>, _>>().unwrap();
            let anchors: Vec<_> = events
                .iter()
                .filter_map(|(event, span)| match event {
                    Event::Anchor(id, name) => Some((*id, name, span)),
                    _ => None,
                })
                .collect();
            assert_eq!(anchors.len(), usize::from(enabled));
            if enabled {
                let (id, name, span) = anchors[0];
                assert_eq!(id, 2);
                assert_eq!(name.as_ref(), "inc");
                assert_eq!(matches!(name, Cow::Borrowed(_)), borrowed);
                assert_eq!((span.start.index(), span.end.index()), (0, 4));
                if borrowed {
                    assert_eq!(span.slice(INCLUDED), Some("&inc"));
                }
            }
            assert!(events
                .iter()
                .any(|(event, _)| matches!(event, Event::Alias(2))));
            assert!(events
                .iter()
                .any(|(event, _)| matches!(event, Event::Scalar(_, _, 3, _))));
        }
    }
}
