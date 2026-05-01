use saphyr_parser_bw::{
    parser_stack::ParserStack, Event, Parser, ParserTrait, StrInput,
};
use std::iter::Empty;

type MyStack<'a> = ParserStack<'a, Empty<char>, StrInput<'a>>;

fn collect_events<'a>(stack: &mut MyStack<'a>) -> Result<Vec<Event<'a>>, String> {
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

fn format_events(events: &[Event]) -> Vec<String> {
    events.iter().map(|e| match e {
        Event::StreamStart => "StreamStart".to_string(),
        Event::StreamEnd => "StreamEnd".to_string(),
        Event::DocumentStart(_) => "DocStart".to_string(),
        Event::DocumentEnd => "DocEnd".to_string(),
        Event::Scalar(val, _, _, _) => format!("Scalar({})", val),
        Event::MappingStart(_, _) => "MapStart".to_string(),
        Event::MappingEnd => "MapEnd".to_string(),
        Event::SequenceStart(_, _) => "SeqStart".to_string(),
        Event::SequenceEnd => "SeqEnd".to_string(),
        _ => "Other".to_string(),
    }).collect()
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
            "StreamStart", "DocStart", "MapStart", "Scalar(a)", "Scalar(b)", "MapEnd", "DocEnd", "StreamEnd"
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
            "StreamStart", "DocStart", "MapStart", "Scalar(b)", "Scalar(2)", "MapEnd", "DocEnd",
            "StreamStart", "DocStart", "MapStart", "Scalar(a)", "Scalar(1)", "MapEnd", "DocEnd", "StreamEnd"
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
    assert!(res.unwrap_err().contains("multiple documents not supported here"));
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
            "StreamStart", "DocStart", "MapStart", "Scalar(b)", "Scalar(2)", "MapEnd", "DocEnd",
            // p1 doc 1
            "StreamStart", "DocStart", "MapStart", "Scalar(a)", "Scalar(1)", "MapEnd", "DocEnd",
            // p1 doc 2
            "DocStart", "MapStart", "Scalar(c)", "Scalar(3)", "MapEnd", "DocEnd", "StreamEnd"
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
    assert!(matches!(ev2, Event::StreamStart));

    // Now push third parser
    stack.push_str_parser(Parser::new_from_str("p3: 3"), "p3".to_string());

    // Consume the rest
    let events = collect_events(&mut stack).unwrap();
    let names = format_events(&events);

    // p3 content:
    let mut expected = vec![
        "StreamStart", "DocStart", "MapStart", "Scalar(p3)", "Scalar(3)", "MapEnd", "DocEnd"
    ];
    // p2 rest (already yielded StreamStart):
    expected.extend(vec![
        "DocStart", "MapStart", "Scalar(p2)", "Scalar(2)", "MapEnd", "DocEnd"
    ]);
    // p1 rest (already yielded StreamStart):
    expected.extend(vec![
        "DocStart", "MapStart", "Scalar(p1)", "Scalar(1)", "MapEnd", "DocEnd", "StreamEnd"
    ]);

    let expected_names: Vec<String> = expected.into_iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected_names);
}
