use granit_parser as saphyr_parser;
use saphyr_parser::{Event, Parser, ScanError, Span};

/// Run the parser through the string.
///
/// The parser is run through both the `StrInput` and `BufferedInput` variants. The resulting
/// events are then compared and must match.
///
/// # Returns
/// This function returns the events and associated spans if parsing succeeds, the error the parser returned otherwise.
///
/// # Panics
/// This function panics if there is a mismatch between the 2 parser invocations with the different
/// input traits.
fn run_parser_with_span(input: &str) -> Result<Vec<(Event<'_>, Span)>, ScanError> {
    let mut str_events = vec![];
    let mut str_error = None;
    let mut iter_events = vec![];
    let mut iter_error = None;

    for x in Parser::new_from_str(input) {
        match x {
            Ok(event) => str_events.push(event),
            Err(e) => {
                str_error = Some(e);
                break;
            }
        }
    }
    for x in Parser::new_from_iter(input.chars()) {
        match x {
            Ok(event) => iter_events.push(event),
            Err(e) => {
                iter_error = Some(e);
                break;
            }
        }
    }

    assert_eq!(str_events, iter_events);
    assert_eq!(str_error, iter_error);

    if let Some(err) = str_error {
        Err(err)
    } else {
        Ok(str_events)
    }
}

#[test]
fn test_document_end_emitted_immediately() {
    // Test that DocumentEnd event is emitted immediately after the document end marker (...)
    // without reading more content ahead.
    // The span of DocumentEnd should end right after the "..." marker.
    let s = "foo\n...\nbar";
    //       0123 456 789...
    //       foo\n = 0-3 (4 chars)
    //       ... = 4-6 (3 chars)
    //       \n = 7
    //       bar = 8-10

    let events = run_parser_with_span(s).unwrap();

    // Find the DocumentEnd event and check its span
    let doc_end_event = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::DocumentEnd))
        .expect("DocumentEnd event should exist");

    // The DocumentEnd span should start at position 4 (start of "...")
    // and end at position 7 (right after "...")
    assert_eq!(
        doc_end_event.1.start.index(),
        4,
        "DocumentEnd should start at the '...' marker"
    );
    assert_eq!(
        doc_end_event.1.end.index(),
        7,
        "DocumentEnd should end right after the '...' marker"
    );
}

#[test]
fn test_document_start_emitted_immediately() {
    // Test that DocumentStart event is emitted immediately after the document start marker (---)
    // without reading more content ahead.
    let s = "---\nfoo";
    //       0123 456
    //       --- = 0-2 (3 chars)
    //       \n = 3
    //       foo = 4-6

    let events = run_parser_with_span(s).unwrap();

    // Find the DocumentStart event and check its span
    let doc_start_event = events
        .iter()
        .find(|(ev, _)| matches!(ev, Event::DocumentStart(true)))
        .expect("DocumentStart(true) event should exist");

    // The DocumentStart span should start at position 0 (start of "---")
    // and end at position 3 (right after "---")
    assert_eq!(
        doc_start_event.1.start.index(),
        0,
        "DocumentStart should start at the '---' marker"
    );
    assert_eq!(
        doc_start_event.1.end.index(),
        3,
        "DocumentStart should end right after the '---' marker"
    );
}

#[test]
fn test_document_end_emitted_immediately_on_next_document_start_marker() {
    // Test that DocumentEnd event is emitted immediately when the parser encounters a new
    // document start marker ("---") at the beginning of a line.
    //
    // In YAML, a `---` marker can implicitly terminate the previous document.
    // The `DocumentEnd` span should therefore be located at the `---` marker and must be
    // emitted before the subsequent `DocumentStart(true)`.
    let s = "foo\n---\nbar";
    //       0123 456 789...
    //       foo\n = 0-3 (4 chars)
    //       --- = 4-6 (3 chars)
    //       \n = 7
    //       bar = 8-10

    let events = run_parser_with_span(s).unwrap();

    // Find the index of the DocumentEnd event.
    let doc_end_idx = events
        .iter()
        .position(|(ev, _)| matches!(ev, Event::DocumentEnd))
        .expect("DocumentEnd event should exist");

    // The next event must be DocumentStart(true) for the second document.
    let (next_ev, next_span) = events
        .get(doc_end_idx + 1)
        .expect("DocumentStart(true) should follow DocumentEnd");
    assert!(
        matches!(next_ev, Event::DocumentStart(true)),
        "DocumentStart(true) should immediately follow DocumentEnd"
    );

    // DocumentEnd should be located at the `---` marker.
    let doc_end_span = events[doc_end_idx].1;
    assert_eq!(
        doc_end_span.start.index(),
        4,
        "DocumentEnd should start at the '---' marker"
    );
    assert_eq!(
        doc_end_span.end.index(),
        7,
        "DocumentEnd should end right after the '---' marker"
    );

    // Sanity: the DocumentStart marker span should match.
    assert_eq!(
        next_span.start.index(),
        4,
        "DocumentStart should start at the '---' marker"
    );
    assert_eq!(
        next_span.end.index(),
        7,
        "DocumentStart should end right after the '---' marker"
    );
}
