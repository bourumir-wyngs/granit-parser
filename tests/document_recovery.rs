use granit_parser::{
    options, BorrowedInput, ErrorKind, Event, EventReceiver, Parser, ScalarStyle, ScanError, Span,
    StructureStyle, YamlVersion,
};

fn first_error<'input, I: BorrowedInput<'input>>(parser: &mut Parser<'input, I>) -> ScanError {
    parser
        .by_ref()
        .find_map(Result::err)
        .expect("the abandoned document must contain an error")
}

fn remaining_events<'input, I: BorrowedInput<'input>>(
    parser: &mut Parser<'input, I>,
) -> Vec<Event<'input>> {
    parser
        .by_ref()
        .map(|event| event.expect("recovered document should parse").0)
        .collect()
}

fn recover<'input, I: BorrowedInput<'input>>(
    mut parser: Parser<'input, I>,
) -> Vec<(Event<'input>, Span)> {
    first_error(&mut parser);
    parser.proceed_next_document().unwrap();
    parser.collect::<Result<Vec<_>, _>>().unwrap()
}

fn scalar_document(value: &str, explicit: bool) -> Vec<Event<'_>> {
    vec![
        Event::DocumentStart(explicit, None),
        Event::Scalar(value.into(), ScalarStyle::Plain, 0, None),
        Event::DocumentEnd,
        Event::StreamEnd,
    ]
}

#[test]
fn recovers_broken_flow_and_quoted_scalars_on_all_input_backends() {
    for broken in [
        "broken: [1, 2\n",
        "broken: {key: [1, 2}\n",
        "broken: \"unfinished\n",
        "broken: 'unfinished\n",
        "broken: \"\\q\"\n",
        "broken: *missing\n",
    ] {
        let source = format!("{broken}---\nhealthy:\n  enabled: true\n");
        let borrowed = recover(Parser::new_from_str(&source));
        let iterated = recover(Parser::new_from_iter(source.chars()));
        let fallible = recover(Parser::new_from_fallible_iter(
            source.chars().map(Ok::<_, ErrorKind>),
        ));
        assert_eq!(borrowed, iterated, "source: {source:?}");
        assert_eq!(borrowed, fallible, "source: {source:?}");

        let expected: Vec<_> = Parser::new_from_str("---\nhealthy:\n  enabled: true\n")
            .skip(1)
            .map(|event| event.unwrap().0)
            .collect();
        let actual: Vec<_> = borrowed.into_iter().map(|(event, _)| event).collect();
        assert_eq!(actual, expected, "source: {source:?}");
    }
}

#[test]
fn recovers_nested_mapping_after_unclosed_flow_sequence() {
    let source = "broken: [1, 2\n---\nhealthy:\n  enabled: true\n";
    let mut parser = Parser::new_from_str(source);
    let mut diagnostics = 0;
    let mut recovered = Vec::new();

    while let Some(next) = parser.next_event() {
        match next {
            Ok(event) if diagnostics > 0 => recovered.push(event),
            Ok(_) => {}
            Err(error) => {
                assert_eq!(diagnostics, 0, "the healthy document must not fail");
                assert_eq!(
                    error.kind(),
                    &ErrorKind::UnclosedFlowCollection { open: '[' }
                );
                diagnostics += 1;
                parser.proceed_next_document().unwrap();
            }
        }
    }

    assert_eq!(diagnostics, 1);
    assert_eq!(
        recovered
            .iter()
            .map(|(event, _)| event.clone())
            .collect::<Vec<_>>(),
        vec![
            Event::DocumentStart(true, None),
            Event::MappingStart(StructureStyle::Block, 0, None),
            Event::Scalar("healthy".into(), ScalarStyle::Plain, 0, None),
            Event::MappingStart(StructureStyle::Block, 0, None),
            Event::Scalar("enabled".into(), ScalarStyle::Plain, 0, None),
            Event::Scalar("true".into(), ScalarStyle::Plain, 0, None),
            Event::MappingEnd,
            Event::MappingEnd,
            Event::DocumentEnd,
            Event::StreamEnd,
        ],
    );

    let scalars: Vec<_> = recovered
        .iter()
        .filter_map(|(event, span)| match event {
            Event::Scalar(..) => Some((span.slice(source), span.start.line(), span.start.col())),
            _ => None,
        })
        .collect();
    assert_eq!(
        scalars,
        vec![
            (Some("healthy"), 3, 0),
            (Some("enabled"), 4, 2),
            (Some("true"), 4, 11),
        ],
    );
}

#[test]
fn default_error_contract_remains_terminal_until_recovery_is_requested() {
    let mut parser = Parser::new_from_str("broken: [1, 2\n---\nhealthy\n");
    first_error(&mut parser);
    assert!(parser.next_event().is_none());
    assert!(parser.peek().is_none());
    parser.proceed_next_document().unwrap();
    assert_eq!(
        remaining_events(&mut parser),
        scalar_document("healthy", true)
    );
}

#[test]
fn recovery_discards_a_peeked_error() {
    let mut parser = Parser::new_from_str("broken: *missing\n---\nhealthy\n");
    loop {
        match parser.peek().expect("expected syntax error") {
            Ok(_) => {
                parser.next_event().unwrap().unwrap();
            }
            Err(error) => {
                assert_eq!(error.kind(), &ErrorKind::UnknownAnchor);
                break;
            }
        }
    }
    parser.proceed_next_document().unwrap();
    assert!(matches!(
        parser.peek().unwrap().unwrap().0,
        Event::DocumentStart(true, None)
    ));
    assert_eq!(
        remaining_events(&mut parser),
        scalar_document("healthy", true)
    );
}

#[derive(Default)]
struct Events<'input>(Vec<Event<'input>>);

impl<'input> EventReceiver<'input> for Events<'input> {
    fn on_event(&mut self, event: Event<'input>) {
        self.0.push(event);
    }
}

#[test]
fn load_can_resume_without_synthetic_ends_for_the_abandoned_document() {
    let mut parser = Parser::new_from_str("broken: [1, 2\n---\nhealthy\n");
    let mut partial = Events::default();
    parser.load(&mut partial, true).unwrap_err();
    assert!(partial
        .0
        .iter()
        .any(|event| matches!(event, Event::SequenceStart(..))));
    assert!(!partial.0.iter().any(|event| matches!(
        event,
        Event::SequenceEnd | Event::MappingEnd | Event::DocumentEnd | Event::StreamEnd
    )));

    parser.proceed_next_document().unwrap();
    let mut recovered = Events::default();
    parser.load(&mut recovered, true).unwrap();
    assert_eq!(recovered.0, scalar_document("healthy", true));
    assert!(parser.next_event().is_none());
}

#[test]
fn only_separated_column_zero_markers_are_recovery_points() {
    let source = concat!(
        "broken: \"\\q\"\n",
        "  ---\n",
        "inline --- value\n",
        "---suffix\n",
        "---#not-separated\n",
        "----\n",
        "...suffix\n",
        "... trailing-content\n",
        "---\thealthy\n",
    );
    let events = recover(Parser::new_from_str(source));
    assert_eq!(
        events
            .into_iter()
            .map(|(event, _)| event)
            .collect::<Vec<_>>(),
        scalar_document("healthy", true),
    );
}

#[test]
fn literal_nul_is_not_eof_or_marker_separation() {
    let source = "broken: \"\\q\"\n\0\n---\0\n...\0\n---\nhealthy\n";
    for events in [
        recover(Parser::new_from_str(source)),
        recover(Parser::new_from_iter(source.chars())),
        recover(Parser::new_from_fallible_iter(
            source.chars().map(Ok::<_, ErrorKind>),
        )),
    ] {
        assert_eq!(
            events
                .into_iter()
                .map(|(event, _)| event)
                .collect::<Vec<_>>(),
            scalar_document("healthy", true),
        );
    }
}

#[test]
fn a_queued_end_marker_with_invalid_trailing_content_is_not_a_boundary() {
    let source = "broken: [1, 2\n... trailing-content\nnot-a-new-document\n---\nhealthy\n";
    for events in [
        recover(Parser::new_from_str(source)),
        recover(Parser::new_from_iter(source.chars())),
    ] {
        assert_eq!(
            events
                .into_iter()
                .map(|(event, _)| event)
                .collect::<Vec<_>>(),
            scalar_document("healthy", true),
        );
    }
}

#[test]
fn explicit_end_can_recover_into_an_implicit_document() {
    for marker in ["...\n", "...  # abandoned document ends\n", "...\r\n"] {
        let source = format!("broken: [1, 2\n{marker}healthy\n");
        let events = recover(Parser::new_from_str(&source));
        let events: Vec<_> = events
            .into_iter()
            .map(|(event, _)| event)
            .filter(|event| {
                #[cfg(feature = "comments")]
                if matches!(event, Event::Comment(..)) {
                    return false;
                }
                let _ = event;
                true
            })
            .collect();
        assert_eq!(events, scalar_document("healthy", false), "{source:?}");
    }
}

#[test]
fn recovery_preserves_original_unicode_and_crlf_positions() {
    let source = "café: 漢字\r\nbroken: [1, 2\r\n---\r\nhealthy: λ\r\n";
    let events = recover(Parser::new_from_str(source));
    let start = events.first().unwrap();
    let byte_offset = source.find("---").unwrap();
    assert_eq!(start.0, Event::DocumentStart(true, None));
    assert_eq!(start.1.start.index(), source[..byte_offset].chars().count());
    assert_eq!(start.1.start.byte_offset(), Some(byte_offset));
    assert_eq!((start.1.start.line(), start.1.start.col()), (3, 0));
    assert_eq!(start.1.slice(source), Some("---"));
    let (_, scalar) = events
        .iter()
        .find(|(event, _)| matches!(event, Event::Scalar(value, ..) if value == "λ"))
        .unwrap();
    assert_eq!((scalar.start.line(), scalar.start.col()), (4, 9));
    assert_eq!(scalar.slice(source), Some("λ"));
    assert_eq!(
        scalar.byte_range(),
        Some(source.find('λ').unwrap()..source.find('λ').unwrap() + 2)
    );
}

#[test]
fn healthy_documents_can_be_skipped_from_fresh_or_partially_consumed_parsers() {
    for source in ["first\n---\nsecond\n", "---\nfirst\n---\nsecond\n"] {
        for consumed in 0..=3 {
            let mut parser = Parser::new_from_str(source);
            for _ in 0..consumed {
                parser.next_event().unwrap().unwrap();
            }
            parser.proceed_next_document().unwrap();
            assert_eq!(
                remaining_events(&mut parser),
                scalar_document("second", true),
                "source={source:?}, consumed={consumed}",
            );
        }
    }
}

fn skip_at_event_offset<'input, I: BorrowedInput<'input>>(
    mut parser: Parser<'input, I>,
    consumed: usize,
    peek: bool,
) {
    for _ in 0..consumed {
        parser.next_event().unwrap().unwrap();
    }
    if peek {
        parser.peek().unwrap().unwrap();
    }
    parser.proceed_next_document().unwrap();
    assert_eq!(
        remaining_events(&mut parser),
        scalar_document("second", true),
        "consumed={consumed}, peek={peek}",
    );
}

#[test]
fn a_healthy_document_can_be_skipped_at_every_event_offset() {
    for boundary in ["---\n", "...\n---\n"] {
        let source =
            format!("---\nfirst:\n  - [one, two]\n  - {{nested: value}}\n{boundary}second\n");
        let document_end = Parser::new_from_str(&source)
            .position(|event| event.unwrap().0 == Event::DocumentEnd)
            .unwrap();
        for consumed in 0..=document_end {
            for peek in [false, true] {
                skip_at_event_offset(Parser::new_from_str(&source), consumed, peek);
                skip_at_event_offset(Parser::new_from_iter(source.chars()), consumed, peek);
                skip_at_event_offset(
                    Parser::new_from_fallible_iter(source.chars().map(Ok::<_, ErrorKind>)),
                    consumed,
                    peek,
                );
            }
        }
    }
}

#[test]
fn skipping_discards_a_peeked_node_and_pending_collection_state() {
    let mut parser = Parser::new_from_str("first: [one, two]\n---\nsecond\n");
    loop {
        match parser.peek().unwrap().unwrap().0 {
            Event::SequenceStart(..) => break,
            _ => {
                parser.next_event().unwrap().unwrap();
            }
        }
    }
    parser.proceed_next_document().unwrap();
    assert_eq!(
        remaining_events(&mut parser),
        scalar_document("second", true)
    );
}

#[cfg(feature = "comments")]
#[test]
fn skipping_discards_a_deferred_scanner_error_after_a_peeked_event() {
    let mut parser = Parser::new_from_iter("a:\n-\nb\n---\nhealthy\n".chars());
    loop {
        match parser.peek().unwrap().unwrap().0 {
            Event::SequenceStart(..) => break,
            _ => {
                parser.next_event().unwrap().unwrap();
            }
        }
    }
    parser.proceed_next_document().unwrap();
    assert_eq!(
        remaining_events(&mut parser),
        scalar_document("healthy", true)
    );
}

#[test]
fn proceeding_after_document_end_preserves_the_next_document() {
    for source in ["first\n---\nsecond\n", "first\n...\nsecond\n"] {
        let mut parser = Parser::new_from_str(source);
        loop {
            if parser.next_event().unwrap().unwrap().0 == Event::DocumentEnd {
                break;
            }
        }
        parser.proceed_next_document().unwrap();
        assert_eq!(
            remaining_events(&mut parser),
            scalar_document("second", source.contains("---")),
        );
    }
}

#[test]
fn skipping_successive_documents_suppresses_skipped_syntax_errors() {
    let mut parser = Parser::new_from_str("first\n---\n[broken\n---\nthird\n");
    parser.proceed_next_document().unwrap();
    parser.proceed_next_document().unwrap();
    assert_eq!(
        remaining_events(&mut parser),
        scalar_document("third", true)
    );
}

#[test]
fn repeated_errors_make_progress_to_later_documents() {
    let mut parser = Parser::new_from_str(
        "broken: *one\n---\nbroken: \"\\q\"\n---\nbroken: [1, 2\n---\nhealthy\n",
    );
    let mut previous_error_index = None;
    for _ in 0..3 {
        let error = first_error(&mut parser);
        if let Some(previous) = previous_error_index {
            assert!(error.marker().index() > previous);
        }
        previous_error_index = Some(error.marker().index());
        parser.proceed_next_document().unwrap();
    }
    assert_eq!(
        remaining_events(&mut parser),
        scalar_document("healthy", true)
    );
}

#[test]
fn recovery_at_eof_emits_stream_end_once() {
    for source in ["broken: [1, 2", "broken: \"\\q\"\nonly discarded text\n"] {
        let mut parser = Parser::new_from_str(source);
        first_error(&mut parser);
        parser.proceed_next_document().unwrap();
        let (event, span) = parser.next_event().unwrap().unwrap();
        assert_eq!(event, Event::StreamEnd);
        assert_eq!(span.start.index(), source.chars().count());
        for _ in 0..3 {
            assert!(parser.next_event().is_none());
            parser.proceed_next_document().unwrap();
            assert!(parser.peek().is_none());
        }
    }
}

#[test]
fn repeated_proceed_calls_preserve_an_unread_stream_end() {
    for source in ["", "healthy\n", "broken: [1, 2\n"] {
        let mut parser = Parser::new_from_str(source);
        for _ in 0..4 {
            parser.proceed_next_document().unwrap();
        }
        assert_eq!(remaining_events(&mut parser), [Event::StreamEnd]);
        parser.proceed_next_document().unwrap();
        assert!(parser.next_event().is_none());
    }
}

#[test]
fn proceed_preserves_a_peeked_stream_end_until_it_is_consumed() {
    let mut parser = Parser::new_from_str("healthy\n");
    loop {
        if parser.peek().unwrap().unwrap().0 == Event::StreamEnd {
            break;
        }
        parser.next_event().unwrap().unwrap();
    }
    for _ in 0..3 {
        parser.proceed_next_document().unwrap();
        assert_eq!(parser.peek().unwrap().unwrap().0, Event::StreamEnd);
    }
    assert_eq!(remaining_events(&mut parser), [Event::StreamEnd]);
    parser.proceed_next_document().unwrap();
    assert!(parser.peek().is_none());
}

#[test]
fn skipping_an_empty_or_finished_stream_does_not_restart_it() {
    let mut empty = Parser::new_from_str("");
    empty.proceed_next_document().unwrap();
    assert_eq!(remaining_events(&mut empty), vec![Event::StreamEnd]);

    let mut finished = Parser::new_from_str("healthy\n");
    remaining_events(&mut finished);
    finished.proceed_next_document().unwrap();
    assert!(finished.next_event().is_none());
}

#[test]
fn proceed_after_load_consumes_a_recovered_eof_stays_exhausted() {
    for (source, failed) in [("%YAML bad\n", true), ("healthy\n", false)] {
        let mut parser = Parser::new_from_str(source);
        if failed {
            first_error(&mut parser);
        }
        parser.proceed_next_document().unwrap();
        let mut events = Events::default();
        parser.load(&mut events, true).unwrap();
        assert_eq!(events.0, [Event::StreamEnd]);
        parser.proceed_next_document().unwrap();
        assert!(parser.peek().is_none());
        assert!(parser.next_event().is_none());
    }
}

#[test]
fn anchor_names_are_cleared_and_anchor_ids_remain_unique() {
    let source = "first: &base one\nbroken: *missing\n---\nsecond: &base two\ncopy: *base\n";
    let mut parser = Parser::new_from_str(source);
    let mut first_id = None;
    loop {
        match parser.next_event().unwrap() {
            Ok((Event::Scalar(value, _, id, _), _)) if value == "one" => first_id = Some(id),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    parser.proceed_next_document().unwrap();
    let events = remaining_events(&mut parser);
    let second_id = events
        .iter()
        .find_map(|event| match event {
            Event::Scalar(value, _, id, _) if value == "two" => Some(*id),
            _ => None,
        })
        .unwrap();
    assert!(second_id > first_id.unwrap());
    assert!(events.contains(&Event::Alias(second_id)));

    let mut parser = Parser::new_from_str("first: &base one\nbroken: *missing\n---\n*base\n");
    first_error(&mut parser);
    parser.proceed_next_document().unwrap();
    assert_eq!(first_error(&mut parser).kind(), &ErrorKind::UnknownAnchor);
}

#[test]
fn versions_and_tag_directives_follow_normal_document_boundaries() {
    let source = "%YAML 1.2\n%TAG !e! tag:example.com,2026:\n---\nfirst: !e!thing one\nbroken: *missing\n---\n!e!thing two\n";
    let mut strict = Parser::new_from_str(source);
    first_error(&mut strict);
    strict.proceed_next_document().unwrap();
    assert_eq!(
        strict.next_event().unwrap().unwrap().0,
        Event::DocumentStart(true, None)
    );
    first_error(&mut strict);

    let events = recover(Parser::new_from_str(source).keep_tags(true));
    assert_eq!(events[0].0, Event::DocumentStart(true, None));
    assert!(events.iter().any(|(event, _)| matches!(event,
        Event::Scalar(value, _, _, Some(tag))
            if value == "two" && tag.handle() == "tag:example.com,2026:" && tag.suffix() == "thing"
    )));

    let source = "%YAML 1.2\n---\nbroken: *missing\n...\n%YAML 1.1\n---\nhealthy\n";
    let events = recover(Parser::new_from_str(source));
    assert_eq!(
        events[0].0,
        Event::DocumentStart(true, Some(YamlVersion::new(1, 1)))
    );
}

#[test]
fn retained_tags_survive_unsupported_version_headers() {
    check_retained_tags_after_header_error("%YAML 2.0\n", &ErrorKind::UnsupportedYamlMajorVersion);
}

#[test]
fn retained_tags_survive_duplicate_directive_headers() {
    check_retained_tags_after_header_error(
        "%YAML 1.2\n%YAML 1.1\n",
        &ErrorKind::DuplicateVersionDirective,
    );
    check_retained_tags_after_header_error(
        "%TAG !other! tag:first.example,2026:\n%TAG !other! tag:second.example,2026:\n",
        &ErrorKind::DuplicateTagDirective,
    );
}

#[test]
fn retained_tags_survive_scanner_errors_during_headers() {
    // A valid directive first enters parser_process_directives before its next scanner read fails.
    check_retained_tags_after_header_error(
        "%YAML 1.2\n%YAML bad\n",
        &ErrorKind::MissingYamlVersion,
    );
    check_retained_tags_after_header_error(
        "%TAG !other! tag:other.example,2026:\n%YAML bad\n",
        &ErrorKind::MissingYamlVersion,
    );
}

fn check_retained_tags_after_header_error(header: &str, expected_error: &ErrorKind) {
    for separator in ["\n", "\n# between directives\n"] {
        let header = header.replace('\n', separator);
        let source = format!(
            "%TAG !e! tag:example.com,2026:\n---\n!e!thing one\n...\n\
             {header}---\ndiscard\n---\n!e!thing two\n"
        );
        for emit_comments in [false, true] {
            for keep_tags in [false, true] {
                let options = options! { emit_comments: emit_comments };
                assert_header_error_tag_retention(
                    Parser::new_from_str_with_options(&source, options.clone())
                        .keep_tags(keep_tags),
                    expected_error,
                    keep_tags,
                );
                assert_header_error_tag_retention(
                    Parser::new_from_iter_with_options(source.chars(), options)
                        .keep_tags(keep_tags),
                    expected_error,
                    keep_tags,
                );
            }
        }
    }
}

fn assert_header_error_tag_retention<'input, I: BorrowedInput<'input>>(
    mut parser: Parser<'input, I>,
    expected_error: &ErrorKind,
    keep_tags: bool,
) {
    assert_eq!(first_error(&mut parser).kind(), expected_error);
    parser.proceed_next_document().unwrap();
    if keep_tags {
        let events = remaining_events(&mut parser);
        assert!(events.iter().any(|event| matches!(event,
            Event::Scalar(value, _, _, Some(tag))
                if value == "two" && tag.handle() == "tag:example.com,2026:" && tag.suffix() == "thing"
        )));
        assert!(!events
            .iter()
            .any(|event| matches!(event, Event::Scalar(value, ..) if value == "discard")));
        assert_eq!(events.last(), Some(&Event::StreamEnd));
    } else {
        assert_eq!(
            first_error(&mut parser).kind(),
            &ErrorKind::UndeclaredTagHandle
        );
    }
}

#[test]
fn failed_document_headers_and_their_bodies_are_abandoned_together() {
    for header in [
        "%YAML 2.0\n",
        "%YAML bad\n",
        "%YAML 1.2\n%YAML 1.1\n",
        "%TAG !e! tag:example.com,2026:\n%TAG !e! tag:other.example,2026:\n",
        "# before header\n%YAML bad\n# after header\n",
        "%YAML 1.2\n# between directives\n%YAML 1.1\n",
    ] {
        let source = format!("{header}--- # abandoned\nfailed-body\n---\nhealthy\n");
        for events in [
            recover(Parser::new_from_str(&source)),
            recover(Parser::new_from_iter(source.chars())),
            recover(Parser::new_from_fallible_iter(
                source.chars().map(Ok::<_, ErrorKind>),
            )),
        ] {
            assert_eq!(
                events
                    .into_iter()
                    .map(|(event, _)| event)
                    .collect::<Vec<_>>(),
                scalar_document("healthy", true),
                "source: {source:?}",
            );
        }
    }
}

#[test]
fn a_disallowed_directive_also_abandons_its_document_body() {
    for directive in ["%YAML 1.2", "%TAG !e! tag:example.com,2026:", "%FOO value"] {
        let source = format!("{directive}\n---\nfailed-body\n---\nhealthy\n");
        let options = options! { disallow_directives: true };
        for events in [
            recover(Parser::new_from_str_with_options(&source, options.clone())),
            recover(Parser::new_from_iter_with_options(
                source.chars(),
                options.clone(),
            )),
            recover(Parser::new_from_fallible_iter_with_options(
                source.chars().map(Ok::<_, ErrorKind>),
                options,
            )),
        ] {
            assert_eq!(
                events
                    .into_iter()
                    .map(|(event, _)| event)
                    .collect::<Vec<_>>(),
                scalar_document("healthy", true),
                "source: {source:?}",
            );
        }
    }
}

#[test]
fn ordinary_content_errors_before_a_start_marker_do_not_skip_the_next_document() {
    for prefix in [
        "@bad\n",
        "%YAML 1.2\nmissing-start\n",
        "%YAML 1.2\n@bad\n",
        "%TAG !e! tag:example.com,2026:\n@bad\n",
    ] {
        let source = format!("{prefix}---\nhealthy\n");
        for events in [
            recover(Parser::new_from_str(&source)),
            recover(Parser::new_from_iter(source.chars())),
            recover(Parser::new_from_fallible_iter(
                source.chars().map(Ok::<_, ErrorKind>),
            )),
        ] {
            assert_eq!(
                events
                    .into_iter()
                    .map(|(event, _)| event)
                    .collect::<Vec<_>>(),
                scalar_document("healthy", true),
                "source: {source:?}",
            );
        }
    }
}

#[test]
fn failed_headers_can_recover_after_an_explicit_end_or_reach_eof() {
    for header in ["%YAML 2.0\n", "%YAML bad\n"] {
        for body in ["", "---\nfailed-body\n"] {
            let source = format!("{header}{body}...\nhealthy\n");
            let events = recover(Parser::new_from_str(&source));
            assert_eq!(
                events
                    .into_iter()
                    .map(|(event, _)| event)
                    .collect::<Vec<_>>(),
                scalar_document("healthy", false),
                "source: {source:?}",
            );
        }
        let source = format!("{header}---\nfailed-body\n");
        let events = recover(Parser::new_from_str(&source));
        assert_eq!(
            events
                .into_iter()
                .map(|(event, _)| event)
                .collect::<Vec<_>>(),
            [Event::StreamEnd],
            "source: {source:?}",
        );
    }
}

#[test]
fn keep_tags_does_not_retain_redefined_primary_or_builtin_handles_after_recovery() {
    for (handle, expected_prefix) in [("!", "!"), ("!!", "tag:yaml.org,2002:")] {
        let source = format!(
            "%TAG {handle} tag:example.com,2026:\n---\nfirst: {handle}int one\nbroken: *missing\n---\n{handle}int two\n"
        );
        let events = recover(Parser::new_from_str(&source).keep_tags(true));
        let tag = events
            .iter()
            .find_map(|(event, _)| match event {
                Event::Scalar(value, _, _, Some(tag)) if value == "two" => Some(tag),
                _ => None,
            })
            .unwrap();
        assert_eq!(tag.handle(), expected_prefix);
        assert_eq!(tag.suffix(), "int");
    }
}

#[test]
fn recovery_retains_options_and_resets_nesting_limits() {
    let source = "[[one]]\n---\n[*missing]\n";
    let mut parser = Parser::new_from_str_with_options(
        source,
        options! {
            flow_nesting_limit: 1,
            emit_unresolved_aliases: true,
            emit_comments: false,
        },
    );
    first_error(&mut parser);
    parser.proceed_next_document().unwrap();
    let events = remaining_events(&mut parser);
    assert!(events.contains(&Event::UnresolvedAlias("missing".into())));
    assert_eq!(events.first(), Some(&Event::DocumentStart(true, None)));
    assert_eq!(events.last(), Some(&Event::StreamEnd));
}

#[cfg(feature = "comments")]
#[test]
fn abandoned_comments_do_not_leak_into_the_recovered_document() {
    let source = "broken: \"\\q\"\n# discarded\n--- # retained\nhealthy # also retained\n";
    let events = recover(Parser::new_from_str(source));
    let comments: Vec<_> = events
        .iter()
        .filter_map(|(event, _)| match event {
            Event::Comment(text, _) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" retained", " also retained"]);

    let events = recover(Parser::new_from_str_with_options(
        source,
        options! { emit_comments: false },
    ));
    assert!(!events
        .iter()
        .any(|(event, _)| matches!(event, Event::Comment(..))));
}

#[test]
fn source_failures_remain_terminal_during_or_before_recovery() {
    for first_report_failure in [false, true] {
        let failure = ErrorKind::InputDecoding {
            message: "truncated UTF-8".into(),
        };
        let input = "first\n"
            .chars()
            .map(Ok)
            .chain(std::iter::once(Err(failure.clone())))
            .chain(std::iter::from_fn(|| panic!("source polled after failure")));
        let mut parser = Parser::new_from_fallible_iter(input);
        if first_report_failure {
            assert_eq!(first_error(&mut parser).kind(), &failure);
        }
        assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
        assert!(parser.next_event().is_none());
        assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
    }
}

#[test]
fn a_source_failure_while_skipping_is_not_reported_as_stream_end() {
    let failure = ErrorKind::InputDecoding {
        message: "interrupted recovery".into(),
    };
    let input = "broken: *missing\nmore: discarded\n"
        .chars()
        .map(Ok)
        .chain(std::iter::once(Err(failure.clone())))
        .chain(std::iter::from_fn(|| panic!("source polled after failure")));
    let mut parser = Parser::new_from_fallible_iter(input);
    assert_eq!(first_error(&mut parser).kind(), &ErrorKind::UnknownAnchor);
    assert_eq!(parser.proceed_next_document().unwrap_err().kind(), &failure);
    assert!(parser.next_event().is_none());
}
