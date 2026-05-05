use granit_parser::{Event, Parser, ScalarStyle, ScanError};

fn parse_events(input: &str) -> Result<Vec<Event<'_>>, ScanError> {
    Parser::new_from_str(input)
        .map(|event| event.map(|(event, _)| event))
        .collect()
}

fn first_error_info(input: &str) -> String {
    for event in Parser::new_from_str(input) {
        if let Err(error) = event {
            return error.info().to_owned();
        }
    }
    panic!("expected parser error");
}

fn scalar_values(events: &[Event<'_>]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| {
            if let Event::Scalar(value, ..) = event {
                Some(value.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn explicit_block_mapping_empty_key_is_null() {
    let events = parse_events("?\n: value\n").unwrap();

    assert_eq!(
        events,
        vec![
            Event::StreamStart,
            Event::DocumentStart(false),
            Event::MappingStart(0, None),
            Event::Scalar("~".into(), ScalarStyle::Plain, 0, None),
            Event::Scalar("value".into(), ScalarStyle::Plain, 0, None),
            Event::MappingEnd,
            Event::DocumentEnd,
            Event::StreamEnd,
        ]
    );
}

#[test]
fn flow_mapping_entry_without_colon_gets_null_value() {
    let events = parse_events("{foo}").unwrap();

    assert_eq!(scalar_values(&events), vec!["foo", "~"]);
    assert!(matches!(events.get(2), Some(Event::MappingStart(..))));
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::MappingEnd)));
}

#[test]
fn flow_sequence_explicit_mapping_can_omit_key_and_value() {
    let events = parse_events("[? ]").unwrap();

    assert_eq!(scalar_values(&events), vec!["~", "~"]);
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::SequenceStart(..))));
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::MappingStart(..))));
}

#[test]
fn flow_sequence_explicit_mapping_can_omit_value() {
    let events = parse_events("[? foo]").unwrap();

    assert_eq!(scalar_values(&events), vec!["foo", "~"]);
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::MappingEnd)));
}

#[test]
fn flow_sequence_entry_requires_comma_before_next_collection() {
    assert_eq!(
        first_error_info("[a [b]]"),
        "while parsing a flow sequence, expected ',' or ']'"
    );
}
