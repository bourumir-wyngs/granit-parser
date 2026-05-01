extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::iter::Empty;
use granit_parser::{
    parser_stack::{ParserStack, ReplayParser},
    BorrowedInput, Event, Marker, Parser, ParserTrait, ScalarStyle, Span, SpannedEventReceiver,
    StrInput,
};

type MyStack<'a> = ParserStack<'a, Empty<char>, StrInput<'a>>;

fn test_span() -> Span {
    Span::empty(Marker::new(0, 1, 0))
}

fn plain_scalar(value: &'static str, anchor_id: usize) -> Event<'static> {
    Event::Scalar(value.into(), ScalarStyle::Plain, anchor_id, None)
}

fn collect_events<'a, I, T>(stack: &mut ParserStack<'a, I, T>) -> Result<Vec<Event<'a>>, String>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'a>,
{
    let mut events = Vec::new();
    loop {
        match stack.next_event() {
            Some(Ok((ev, _))) => {
                let is_end = matches!(ev, Event::StreamEnd);
                events.push(ev);
                if is_end {
                    break;
                }
            }
            Some(Err(e)) => return Err(e.to_string()),
            None => break,
        }
    }
    Ok(events)
}

fn find_anchor_id(events: &[Event], value: &str) -> Option<usize> {
    events.iter().find_map(|event| {
        if let Event::Scalar(scalar, _, anchor_id, _) = event {
            (scalar.as_ref() == value).then_some(*anchor_id)
        } else {
            None
        }
    })
}

fn format_events(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .map(|e| match e {
            Event::StreamStart => "StreamStart".to_string(),
            Event::StreamEnd => "StreamEnd".to_string(),
            Event::DocumentStart(_) => "DocStart".to_string(),
            Event::DocumentEnd => "DocEnd".to_string(),
            Event::Scalar(val, _, _, _) => alloc::format!("Scalar({})", val.as_ref()),
            Event::MappingStart(_, _) => "MapStart".to_string(),
            Event::MappingEnd => "MapEnd".to_string(),
            Event::SequenceStart(_, _) => "SeqStart".to_string(),
            Event::SequenceEnd => "SeqEnd".to_string(),
            _ => "Other".to_string(),
        })
        .collect()
}

#[test]
fn test_single_parser() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: b"), "p1".to_string());

    let events = collect_events(&mut stack).unwrap();
    let names = format_events(&events);

    assert_eq!(
        names,
        vec![
            "StreamStart",
            "DocStart",
            "MapStart",
            "Scalar(a)",
            "Scalar(b)",
            "MapEnd",
            "DocEnd",
            "StreamEnd"
        ]
    );
}

#[test]
fn test_two_parsers_switching() {
    let mut stack: MyStack = ParserStack::new();
    // pushed first, so it's at the bottom (yields last)
    stack.push_str_parser(Parser::new_from_str("a: 1"), "p1".to_string());
    // pushed second, so it's at the top (yields first)
    stack.push_str_parser(Parser::new_from_str("b: 2"), "p2".to_string());

    let events = collect_events(&mut stack).unwrap();
    let names = format_events(&events);

    assert_eq!(
        names,
        vec![
            "MapStart",
            "Scalar(b)",
            "Scalar(2)",
            "MapEnd",
            "StreamStart",
            "DocStart",
            "MapStart",
            "Scalar(a)",
            "Scalar(1)",
            "MapEnd",
            "DocEnd",
            "StreamEnd"
        ]
    );
}

#[test]
fn test_two_parsers_second_has_two_docs_error() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: 1"), "p1".to_string());
    // p2 is top. it has two documents. this should fail.
    stack.push_str_parser(Parser::new_from_str("b: 2\n---\nc: 3"), "p2".to_string());

    let res = collect_events(&mut stack);
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .contains("multiple documents not supported here"));
}

#[test]
fn test_two_parsers_first_has_multiple_docs_fine() {
    let mut stack: MyStack = ParserStack::new();
    // p1 is bottom. It can have multiple documents.
    stack.push_str_parser(Parser::new_from_str("a: 1\n---\nc: 3"), "p1".to_string());
    // p2 is top. Single document.
    stack.push_str_parser(Parser::new_from_str("b: 2"), "p2".to_string());

    let events = collect_events(&mut stack).unwrap();
    let names = format_events(&events);

    assert_eq!(
        names,
        vec![
            // p2
            "MapStart",
            "Scalar(b)",
            "Scalar(2)",
            "MapEnd",
            // p1 doc 1
            "StreamStart",
            "DocStart",
            "MapStart",
            "Scalar(a)",
            "Scalar(1)",
            "MapEnd",
            "DocEnd",
            // p1 doc 2
            "DocStart",
            "MapStart",
            "Scalar(c)",
            "Scalar(3)",
            "MapEnd",
            "DocEnd",
            "StreamEnd"
        ]
    );
}

#[test]
fn test_three_parsers_dynamic_adding() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("p1: 1"), "p1".to_string());

    // Fetch first event from top parser (p1)
    let ev1 = stack.next_event().unwrap().unwrap().0;
    assert!(matches!(ev1, Event::StreamStart));

    // Now push middle parser
    stack.push_str_parser(Parser::new_from_str("p2: 2"), "p2".to_string());

    // Fetch first event from middle parser (p2)
    let ev2 = stack.next_event().unwrap().unwrap().0;
    assert!(matches!(ev2, Event::MappingStart(..)));

    // Now push third parser
    stack.push_str_parser(Parser::new_from_str("p3: 3"), "p3".to_string());

    // Consume the rest
    let events = collect_events(&mut stack).unwrap();
    let names = format_events(&events);

    // p3 content:
    let mut expected = vec!["MapStart", "Scalar(p3)", "Scalar(3)", "MapEnd"];
    // p2 rest (already yielded MapStart, so now it's Scalar(p2)):
    expected.extend(vec!["Scalar(p2)", "Scalar(2)", "MapEnd"]);
    // p1 rest (already yielded StreamStart):
    expected.extend(vec![
        "DocStart",
        "MapStart",
        "Scalar(p1)",
        "Scalar(1)",
        "MapEnd",
        "DocEnd",
        "StreamEnd",
    ]);

    let expected_names: Vec<String> = expected.into_iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected_names);
}

#[test]
fn test_anchor_id_propagation() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(
        Parser::new_from_str("k1: &a v1\nk3: &c v3"),
        "p1".to_string(),
    );

    let mut events = Vec::new();

    // Read until v1 is consumed
    loop {
        let ev = stack.next_event().unwrap().unwrap().0;
        let is_v1 = if let Event::Scalar(val, _, anchor_id, _) = &ev {
            if val.as_ref() == "v1" {
                assert_eq!(*anchor_id, 1, "First anchor should have ID 1");
                true
            } else {
                false
            }
        } else {
            false
        };

        events.push(ev);
        if is_v1 {
            break;
        }
    }

    // Push inner parser after consuming first anchor event
    stack.push_str_parser(Parser::new_from_str("k2: &b v2"), "p2".to_string());

    // Consume the rest
    loop {
        match stack.next_event() {
            Some(Ok((ev, _))) => {
                let is_end = matches!(ev, Event::StreamEnd);
                events.push(ev);
                if is_end {
                    break;
                }
            }
            Some(Err(e)) => panic!("Parse error: {}", e),
            None => break,
        }
    }

    // Verify anchor IDs for v2 and v3
    let v2_ev = events
        .iter()
        .find(|e| matches!(e, Event::Scalar(v, _, _, _) if v.as_ref() == "v2"))
        .unwrap();
    if let Event::Scalar(_, _, id, _) = v2_ev {
        assert_eq!(*id, 2, "Second anchor (from inner parser) should have ID 2");
    }

    let v3_ev = events
        .iter()
        .find(|e| matches!(e, Event::Scalar(v, _, _, _) if v.as_ref() == "v3"))
        .unwrap();
    if let Event::Scalar(_, _, id, _) = v3_ev {
        assert_eq!(
            *id, 3,
            "Third anchor (from parent parser after inner) should have ID 3"
        );
    }
}

struct TestReceiver<'input> {
    events: Vec<Event<'input>>,
}

impl<'input> SpannedEventReceiver<'input> for TestReceiver<'input> {
    fn on_event(&mut self, ev: Event<'input>, _span: Span) {
        self.events.push(ev);
    }
}

#[test]
fn test_parser_stack_load() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: 1\n---\nc: 3"), "p1".to_string());
    stack.push_str_parser(Parser::new_from_str("b: 2"), "p2".to_string());

    let mut recv = TestReceiver { events: Vec::new() };

    // Load with multi = true
    stack.load(&mut recv, true).unwrap();

    let names = format_events(&recv.events);

    assert_eq!(
        names,
        vec![
            // p2
            "MapStart",
            "Scalar(b)",
            "Scalar(2)",
            "MapEnd",
            // p1 doc 1
            "StreamStart",
            "DocStart",
            "MapStart",
            "Scalar(a)",
            "Scalar(1)",
            "MapEnd",
            "DocEnd",
            // p1 doc 2
            "DocStart",
            "MapStart",
            "Scalar(c)",
            "Scalar(3)",
            "MapEnd",
            "DocEnd",
            "StreamEnd"
        ]
    );
}

#[test]
fn test_parser_stack_load_single() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: 1\n---\nc: 3"), "p1".to_string());
    stack.push_str_parser(Parser::new_from_str("b: 2"), "p2".to_string());

    let mut recv = TestReceiver { events: Vec::new() };

    // Load with multi = false
    stack.load(&mut recv, false).unwrap();

    let names = format_events(&recv.events);

    assert_eq!(
        names,
        vec![
            // Nested parsers are inlined as subtree events, so their DocumentEnd is suppressed.
            // With multi = false, loading stops at the first DocumentEnd emitted by the parent parser.
            "MapStart",
            "Scalar(b)",
            "Scalar(2)",
            "MapEnd",
            // p1 doc 1
            "StreamStart",
            "DocStart",
            "MapStart",
            "Scalar(a)",
            "Scalar(1)",
            "MapEnd",
            "DocEnd"
        ]
    );
}

#[test]
fn test_iterator_impl() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: b"), "p1".to_string());

    let mut events = Vec::new();
    for ev in stack {
        let (e, _) = ev.unwrap();
        events.push(e);
    }

    let names = format_events(&events);
    assert_eq!(
        names,
        vec![
            "StreamStart",
            "DocStart",
            "MapStart",
            "Scalar(a)",
            "Scalar(b)",
            "MapEnd",
            "DocEnd",
            "StreamEnd"
        ]
    );
}

#[test]
fn test_include_resolver() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: 1"), "p1".to_string());

    stack.set_resolver(|name| {
        if name == "inc1" {
            Ok("b: 2".to_string())
        } else {
            Err(granit_parser::ScanError::new(
                granit_parser::Marker::new(0, 1, 0),
                "Not found".to_string(),
            ))
        }
    });

    stack.resolve("inc1").unwrap();

    let events = collect_events(&mut stack).unwrap();
    let names = format_events(&events);

    assert_eq!(
        names,
        vec![
            "MapStart",
            "Scalar(b)",
            "Scalar(2)",
            "MapEnd",
            "StreamStart",
            "DocStart",
            "MapStart",
            "Scalar(a)",
            "Scalar(1)",
            "MapEnd",
            "DocEnd",
            "StreamEnd"
        ]
    );
}

#[test]
fn test_unexpected_eof_is_reported() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: [1, 2"), "p1".to_string());

    let res = collect_events(&mut stack);
    assert!(res.is_err(), "expected malformed input to return an error");
}

#[test]
fn test_replay_parser_updates_anchor_offset() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(
        Parser::new_from_str("k1: &a v1\nk3: &c v3"),
        "p1".to_string(),
    );

    loop {
        let ev = stack.next_event().unwrap().unwrap().0;
        if matches!(ev, Event::Scalar(ref val, _, _, _) if val.as_ref() == "v1") {
            break;
        }
    }

    let span = Span::empty(Marker::new(0, 1, 0));
    let replay_events = vec![
        (Event::StreamStart, span),
        (Event::DocumentStart(false), span),
        (Event::MappingStart(0, None), span),
        (
            Event::Scalar("k2".into(), granit_parser::ScalarStyle::Plain, 0, None),
            span,
        ),
        (
            Event::Scalar("v2".into(), granit_parser::ScalarStyle::Plain, 2, None),
            span,
        ),
        (Event::MappingEnd, span),
        (Event::DocumentEnd, span),
        (Event::StreamEnd, span),
    ];
    stack.push_replay_parser(ReplayParser::new(replay_events, 1), "replay".to_string());

    let events = collect_events(&mut stack).unwrap();
    let v3_ev = events
        .iter()
        .find(|e| matches!(e, Event::Scalar(v, _, _, _) if v.as_ref() == "v3"))
        .unwrap();

    if let Event::Scalar(_, _, id, _) = v3_ev {
        assert_eq!(
            *id, 3,
            "Parent parser should continue after replayed anchors"
        );
    }
}

#[test]
fn test_replay_parser_without_anchors_does_not_regress_anchor_offset() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(
        Parser::new_from_str("k1: &a v1\nk2: &b v2"),
        "parent".to_string(),
    );

    loop {
        let ev = stack.next_event().unwrap().unwrap().0;
        if matches!(ev, Event::Scalar(ref val, _, _, _) if val.as_ref() == "v1") {
            break;
        }
    }

    let span = Span::empty(Marker::new(0, 1, 0));
    let replay_events = vec![
        (Event::StreamStart, span),
        (Event::DocumentStart(false), span),
        (Event::MappingStart(0, None), span),
        (
            Event::Scalar(
                "included".into(),
                granit_parser::ScalarStyle::Plain,
                0,
                None,
            ),
            span,
        ),
        (
            Event::Scalar("value".into(), granit_parser::ScalarStyle::Plain, 0, None),
            span,
        ),
        (Event::MappingEnd, span),
        (Event::DocumentEnd, span),
        (Event::StreamEnd, span),
    ];

    stack.push_replay_parser(ReplayParser::new(replay_events, 1), "replay".to_string());

    let events = collect_events(&mut stack).unwrap();
    let v2_ev = events
        .iter()
        .find(|e| matches!(e, Event::Scalar(v, _, _, _) if v.as_ref() == "v2"))
        .unwrap();

    if let Event::Scalar(_, _, id, _) = v2_ev {
        assert_eq!(
            *id, 2,
            "Parent parser should not reuse anchor IDs after replay without anchors"
        );
    }
}

#[test]
fn replay_parser_peek_next_and_load_track_collection_anchors() {
    let span = test_span();
    let replay_events = vec![
        (Event::StreamStart, span),
        (Event::DocumentStart(false), span),
        (Event::SequenceStart(4, None), span),
        (Event::SequenceEnd, span),
        (Event::MappingStart(7, None), span),
        (Event::MappingEnd, span),
        (Event::DocumentEnd, span),
        (Event::StreamEnd, span),
    ];
    let mut replay = ReplayParser::new(replay_events, 1);

    assert!(matches!(
        replay.peek().unwrap().unwrap().0,
        Event::StreamStart
    ));
    assert!(matches!(
        replay.next_event().unwrap().unwrap().0,
        Event::StreamStart
    ));

    let mut recv = TestReceiver { events: Vec::new() };
    replay.load(&mut recv, true).unwrap();

    assert_eq!(replay.get_anchor_offset(), 8);
    assert_eq!(
        format_events(&recv.events),
        vec![
            "DocStart",
            "SeqStart",
            "SeqEnd",
            "MapStart",
            "MapEnd",
            "DocEnd",
            "StreamEnd"
        ]
    );
}

#[test]
fn replay_parser_load_single_stops_at_document_end() {
    let span = test_span();
    let replay_events = vec![
        (Event::StreamStart, span),
        (Event::DocumentStart(false), span),
        (plain_scalar("first", 0), span),
        (Event::DocumentEnd, span),
        (Event::DocumentStart(false), span),
        (plain_scalar("second", 0), span),
        (Event::DocumentEnd, span),
        (Event::StreamEnd, span),
    ];
    let mut replay = ReplayParser::new(replay_events, 1);
    let mut recv = TestReceiver { events: Vec::new() };

    replay.load(&mut recv, false).unwrap();

    assert_eq!(
        format_events(&recv.events),
        vec!["StreamStart", "DocStart", "Scalar(first)", "DocEnd"]
    );
}

#[test]
fn default_empty_stack_peek_and_next_emit_stream_end_once() {
    let mut stack: MyStack = ParserStack::default();

    assert!(matches!(stack.peek().unwrap().unwrap().0, Event::StreamEnd));
    assert!(matches!(stack.peek().unwrap().unwrap().0, Event::StreamEnd));
    assert!(matches!(
        stack.next_event().unwrap().unwrap().0,
        Event::StreamEnd
    ));
    assert!(stack.next_event().is_none());
}

#[test]
fn parser_stack_resolve_without_resolver_reports_error() {
    let mut stack: MyStack = ParserStack::new();

    let err = stack.resolve("missing").unwrap_err();

    assert!(err
        .to_string()
        .contains("No include resolver set for parser stack."));
}

#[test]
fn iter_parser_inherits_anchor_offset_and_reports_stack() {
    let mut stack: ParserStack<'static, alloc::vec::IntoIter<char>, StrInput<'static>> =
        ParserStack::new();
    stack.push_str_parser(
        Parser::new_from_str("k1: &a v1\nk3: &c v3"),
        "parent".to_string(),
    );

    loop {
        let ev = stack.next_event().unwrap().unwrap().0;
        if matches!(ev, Event::Scalar(ref val, _, _, _) if val.as_ref() == "v1") {
            break;
        }
    }
    assert_eq!(stack.current_anchor_offset(), 2);

    let iter = "k2: &b v2".chars().collect::<Vec<_>>().into_iter();
    stack.push_iter_parser(Parser::new_from_iter(iter), "iter".to_string());

    assert_eq!(
        stack.stack(),
        vec!["parent".to_string(), "iter".to_string()]
    );
    assert_eq!(stack.current_anchor_offset(), 2);

    let events = collect_events(&mut stack).unwrap();
    assert_eq!(find_anchor_id(&events, "v2"), Some(2));
    assert_eq!(find_anchor_id(&events, "v3"), Some(3));
}

#[test]
fn custom_parser_inherits_anchor_offset_and_reports_stack() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(
        Parser::new_from_str("k1: &a v1\nk3: &c v3"),
        "parent".to_string(),
    );

    loop {
        let ev = stack.next_event().unwrap().unwrap().0;
        if matches!(ev, Event::Scalar(ref val, _, _, _) if val.as_ref() == "v1") {
            break;
        }
    }
    assert_eq!(stack.current_anchor_offset(), 2);

    stack.push_custom_parser(
        Parser::new(StrInput::new("k2: &b v2")),
        "custom".to_string(),
    );

    assert_eq!(
        stack.stack(),
        vec!["parent".to_string(), "custom".to_string()]
    );
    assert_eq!(stack.current_anchor_offset(), 2);

    let events = collect_events(&mut stack).unwrap();
    assert_eq!(find_anchor_id(&events, "v2"), Some(2));
    assert_eq!(find_anchor_id(&events, "v3"), Some(3));
}

#[test]
fn custom_parser_with_current_primes_next_event() {
    let mut stack: MyStack = ParserStack::new();
    let span = test_span();
    stack.push_custom_parser_with_current(
        Parser::new(StrInput::new("k: v")),
        "custom".to_string(),
        (plain_scalar("primed", 0), span),
    );

    assert_eq!(stack.stack(), vec!["custom".to_string()]);

    let (event, event_span) = stack.next_event().unwrap().unwrap();
    assert_eq!(event, plain_scalar("primed", 0));
    assert_eq!(event_span, span);
}

#[test]
fn replay_parser_without_stream_end_is_popped_at_eof() {
    let span = test_span();
    let mut stack: MyStack = ParserStack::new();

    stack.push_replay_parser(
        ReplayParser::new(vec![(plain_scalar("only", 0), span)], 1),
        "replay".to_string(),
    );

    assert!(matches!(
        stack.next_event().unwrap().unwrap().0,
        Event::Scalar(..)
    ));
    assert!(matches!(
        stack.next_event().unwrap().unwrap().0,
        Event::StreamEnd
    ));
    assert!(stack.next_event().is_none());
}

#[test]
fn parser_stack_peek_surfaces_parse_error() {
    let mut stack: MyStack = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str("a: [1, 2"), "bad".to_string());

    loop {
        match stack.peek() {
            Some(Ok(_)) => {
                stack.next_event().unwrap().unwrap();
            }
            Some(Err(err)) => {
                assert_eq!(err.info(), "unclosed bracket '['");
                break;
            }
            None => panic!("expected parse error before the stream ended"),
        }
    }
}
