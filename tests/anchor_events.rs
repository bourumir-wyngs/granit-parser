use std::borrow::Cow;

use granit_parser::{
    BorrowedInput, ErrorKind, Event, Options, Parser, Span, SpannedEventReceiver, TryLoadError,
    TrySpannedEventReceiver,
};

fn anchor_options() -> Options {
    granit_parser::options! { emit_anchor_events: true }
}

fn parse(source: &str, options: Options) -> Vec<(Event<'_>, Span)> {
    Parser::new_from_str_with_options(source, options)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("failed to parse {source:?}: {error:?}"))
}

fn assert_anchor_follows_node(events: &[(Event<'_>, Span)]) {
    for (index, (event, span)) in events.iter().enumerate() {
        if let Event::Anchor(id, _) = event {
            assert!(index > 0);
            assert!(events[index - 1].0.is_node());
            assert_eq!(events[index - 1].0.anchor_id(), Some(*id));
            assert_eq!(span.indent, None);
            assert_eq!(span.tag_span(), None);
            assert!(!event.is_node());
            assert!(!event.is_alias());
            assert_eq!(event.anchor_id(), Some(*id));
            assert_eq!(event.alias_id(), None);
            assert_eq!(event.tag(), None);
            assert_eq!(event.scalar(), None);
        } else if event.is_node() {
            if let Some(id) = event.anchor_id() {
                assert!(
                    matches!(events.get(index + 1), Some((Event::Anchor(next_id, _), _)) if *next_id == id)
                );
            }
        }
    }
}

#[test]
fn anchor_name_id_and_exact_token_span_connect_nodes_and_aliases() {
    let source = "message: &msg !note hello\ncopy: *msg\nexample: \"&msg is just text\"\n# &fake !fake *fake\n";
    let events = parse(source, anchor_options());
    assert_anchor_follows_node(&events);
    let anchors: Vec<_> = events
        .iter()
        .filter(|(event, _)| matches!(event, Event::Anchor(..)))
        .collect();
    assert_eq!(anchors.len(), 1);
    assert!(matches!(
        anchors[0].0,
        Event::Anchor(1, Cow::Borrowed("msg"))
    ));
    assert_eq!(anchors[0].1.slice(source), Some("&msg"));
    assert_eq!(anchors[0].1.byte_range(), Some(9..13));
    let (alias, alias_span) = events.iter().find(|(event, _)| event.is_alias()).unwrap();
    assert_eq!(alias.alias_id(), Some(1));
    assert_eq!(alias_span.slice(source), Some("*msg"));
    let (_, node_span) = events
        .iter()
        .find(|(event, _)| event.tag().is_some())
        .unwrap();
    assert_eq!(node_span.tag_span().unwrap().slice(source), Some("!note"));
}

#[test]
fn emission_is_opt_in_and_preserves_all_existing_events_and_spans() {
    assert!(!Options::default().emit_anchor_events);
    for source in [
        "&a value # tail\n",
        "[&a first, *a, &b [*b]]\n",
        "key: &a # before sequence\n- # empty entry\n- value\nnext: *a\n",
        "key: &a # empty scalar\nnext: value\n",
        "? &key name\n: &value data\n",
        "--- &a first\n--- &b second\n",
    ] {
        for emit_comments in [false, true] {
            let options = granit_parser::options! { emit_comments: emit_comments };
            let baseline = parse(source, options.clone());
            let explicit_false = parse(
                source,
                granit_parser::options! {
                    emit_comments: emit_comments,
                    emit_anchor_events: false,
                },
            );
            assert_eq!(baseline, explicit_false, "{source:?}");
            let mut enabled = options;
            enabled.emit_anchor_events = true;
            let events = parse(source, enabled);
            assert_anchor_follows_node(&events);
            let filtered: Vec<_> = events
                .into_iter()
                .filter(|(event, _)| !matches!(event, Event::Anchor(..)))
                .collect();
            assert_eq!(filtered, baseline, "{source:?}");
        }
    }
}

#[test]
fn every_anchored_node_kind_emits_metadata_immediately_after_its_node_event() {
    for source in [
        "&a value\n",
        "&a\n",
        "[&a, next]\n",
        "key: &a\nnext: value\n",
        "&a []\n",
        "&a {}\n",
        "&a [value]\n",
        "&a {key: value}\n",
        "&a\n- value\n",
        "&a\nkey: value\n",
        "key: &a\n- value\n",
        "key: &a\n-\nnext: value\n",
        "? &a key\n: value\n",
        "[&a key: value]\n",
        "&a [*a, &b {key: &c value}, *b, *c]\n",
    ] {
        let events = parse(source, anchor_options());
        assert_anchor_follows_node(&events);
        assert!(events
            .iter()
            .any(|(event, _)| matches!(event, Event::Anchor(..))));
        for (event, span) in &events {
            if let Event::Anchor(_, name) = event {
                assert_eq!(span.slice(source), Some(format!("&{name}").as_str()));
            }
        }
    }
}

#[test]
fn anchor_and_raw_tag_ranges_survive_property_order_and_comments() {
    for source in [
        "&a !note value\n",
        "!note &a value\n",
        "&a # anchor\n!note value\n",
        "!note # tag\n&a value\n",
        "&a # anchor\n!note # tag\nvalue\n",
        "!note # tag\n&a # anchor\nvalue\n",
        "key: !note &a # empty value\nnext: value\n",
        "key: &a !note # mapping\n  nested: value\n",
        "key: !note &a # sequence\n  - value\n",
        "key: &a !note # indentless sequence\n- # empty entry\n- value\n",
    ] {
        for emit_comments in [false, true] {
            let events = parse(
                source,
                granit_parser::options! {
                    emit_anchor_events: true,
                    emit_comments: emit_comments,
                },
            );
            assert_anchor_follows_node(&events);
            let index = events
                .iter()
                .position(|(event, _)| matches!(event, Event::Anchor(..)))
                .unwrap();
            assert_eq!(events[index].1.slice(source), Some("&a"));
            assert_eq!(events[index - 1].0.tag().unwrap().original(), "!note");
            assert_eq!(
                events[index - 1].1.tag_span().unwrap().slice(source),
                Some("!note")
            );
        }
    }
}

#[test]
fn unicode_anchor_names_and_escaped_tags_retain_source_spelling() {
    for (source, raw_tag) in [
        (
            "clé: &mémo !snow%E2%98%83 value\ncopy: *mémo\n",
            "!snow%E2%98%83",
        ),
        (
            "clé: !<tag:example.com,2026:%E2%98%83> &mémo value\ncopy: *mémo\n",
            "!<tag:example.com,2026:%E2%98%83>",
        ),
        (
            "%TAG !e! tag:example.com,2026:\n---\nclé: &mémo !e!snow%E2%98%83 value\ncopy: *mémo\n",
            "!e!snow%E2%98%83",
        ),
    ] {
        let events = parse(source, anchor_options());
        assert_anchor_follows_node(&events);
        let index = events
            .iter()
            .position(|(event, _)| matches!(event, Event::Anchor(..)))
            .unwrap();
        assert!(matches!(
            events[index].0,
            Event::Anchor(1, Cow::Borrowed("mémo"))
        ));
        let span = events[index].1;
        let start = source.find("&mémo").unwrap();
        let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
        assert_eq!(span.slice(source), Some("&mémo"));
        assert_eq!(span.byte_range(), Some(start..start + "&mémo".len()));
        assert_eq!(span.start.index(), source[..start].chars().count());
        assert_eq!(span.end.index(), span.start.index() + 5);
        assert_eq!(span.start.col(), source[line_start..start].chars().count());
        assert_eq!(span.end.col(), span.start.col() + 5);
        assert_eq!(
            events[index - 1].1.tag_span().unwrap().slice(source),
            Some(raw_tag)
        );
        assert_eq!(
            events
                .iter()
                .find(|(event, _)| event.is_alias())
                .unwrap()
                .1
                .slice(source),
            Some("*mémo")
        );
    }
}

#[test]
fn reused_names_resolve_to_the_latest_definition_with_custom_ids() {
    let source = "[&same first, *same, &same [*same], *same]\n";
    let mut parser = Parser::new_from_str_with_options(source, anchor_options());
    parser.set_anchor_offset(41);
    let events = parser.by_ref().collect::<Result<Vec<_>, _>>().unwrap();
    assert_anchor_follows_node(&events);
    let metadata: Vec<_> = events
        .iter()
        .filter_map(|(event, span)| match event {
            Event::Anchor(id, name) => Some((*id, name.as_ref(), span.byte_range().unwrap())),
            _ => None,
        })
        .collect();
    assert_eq!(metadata, [(41, "same", 1..6), (42, "same", 21..26)]);
    assert_eq!(
        events
            .iter()
            .filter_map(|(event, _)| event.alias_id())
            .collect::<Vec<_>>(),
        [41, 42, 42]
    );
    assert_eq!(parser.anchor_offset(), 43);
}

#[test]
fn zero_anchor_offset_still_emits_metadata_and_allows_later_definitions() {
    let source = "[&zero first, *zero, &next second, *next]\n";
    let mut parser = Parser::new_from_str_with_options(source, anchor_options());
    parser.set_anchor_offset(0);
    let events = parser.by_ref().collect::<Result<Vec<_>, _>>().unwrap();
    let definitions: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(index, (event, span))| match event {
            Event::Anchor(id, name) => {
                assert!(
                    matches!(&events[index - 1].0, Event::Scalar(_, _, node_id, _) if node_id == id)
                );
                Some((*id, name.as_ref(), span.slice(source)))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        definitions,
        [(0, "zero", Some("&zero")), (1, "next", Some("&next"))]
    );
    assert_eq!(
        events
            .iter()
            .filter_map(|(event, _)| event.alias_id())
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(parser.anchor_offset(), 2);
}

#[test]
fn document_boundaries_clear_names_but_preserve_assigned_id_identity() {
    let source = "---\n[&same first, *same]\n---\n[*same, &same second, *same]\n";
    let events = parse(
        source,
        granit_parser::options! {
            emit_anchor_events: true,
            emit_unresolved_aliases: true,
        },
    );
    assert_anchor_follows_node(&events);
    let references: Vec<_> = events
        .iter()
        .filter(|(event, _)| event.is_alias() || matches!(event, Event::Anchor(..)))
        .map(|(event, _)| event.clone())
        .collect();
    assert_eq!(
        references,
        [
            Event::Anchor(1, "same".into()),
            Event::Alias(1),
            Event::UnresolvedAlias("same".into()),
            Event::Anchor(2, "same".into()),
            Event::Alias(2),
        ]
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
fn streaming_peek_and_load_preserve_anchor_metadata_and_order() {
    let source = "clé: &mémo # anchor\n  !note [*mémo, &nested value]\n--- &next end\n";
    for emit_comments in [false, true] {
        let options = granit_parser::options! {
            emit_anchor_events: true,
            emit_comments: emit_comments,
        };
        let expected = parse(source, options.clone());
        for streamed in [
            Parser::new_from_iter_with_options(source.chars(), options.clone())
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            Parser::new_from_fallible_iter_with_options(
                source.chars().map(Ok::<_, ErrorKind>),
                options.clone(),
            )
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        ] {
            assert_eq!(streamed, expected);
            assert_anchor_follows_node(&streamed);
            for (event, span) in &streamed {
                if let Event::Anchor(_, name) = event {
                    assert!(matches!(name, Cow::Owned(_)));
                    assert_eq!(span.byte_range(), None);
                }
            }
        }

        let mut parser = Parser::new_from_str_with_options(source, options.clone());
        let mut peeked = Vec::new();
        while let Some(event) = parser.peek() {
            let event = event.unwrap().clone();
            assert_eq!(parser.peek().unwrap().unwrap(), &event);
            assert_eq!(parser.next().unwrap().unwrap(), event);
            peeked.push(event);
        }
        assert_eq!(peeked, expected);

        let mut parser = Parser::new_from_str_with_options(source, options);
        let mut loaded = Events::default();
        parser.load(&mut loaded, false).unwrap();
        assert!(matches!(loaded.0.last(), Some((Event::DocumentEnd, _))));
        parser.load(&mut loaded, true).unwrap();
        assert_eq!(loaded.0, expected);
    }
}

struct StopAfter<'input> {
    events: Vec<(Event<'input>, Span)>,
    metadata: bool,
}

impl<'input> TrySpannedEventReceiver<'input> for StopAfter<'input> {
    type Error = ();

    fn on_event(&mut self, event: Event<'input>, span: Span) -> Result<(), Self::Error> {
        let stop = if self.metadata {
            matches!(event, Event::Anchor(..))
        } else {
            event.is_node() && event.anchor_id().is_some()
        };
        self.events.push((event, span));
        if stop {
            Err(())
        } else {
            Ok(())
        }
    }
}

#[test]
fn receiver_failure_can_resume_before_or_after_anchor_metadata() {
    let source = "key: &a [*a, value]\n";
    let expected = parse(source, anchor_options());
    for metadata in [false, true] {
        let mut parser = Parser::new_from_str_with_options(source, anchor_options());
        let mut sink = StopAfter {
            events: Vec::new(),
            metadata,
        };
        assert_eq!(
            parser.try_load(&mut sink, true),
            Err(TryLoadError::Receiver(()))
        );
        assert_eq!(
            matches!(parser.peek().unwrap().unwrap().0, Event::Anchor(..)),
            !metadata
        );
        sink.events
            .extend(parser.collect::<Result<Vec<_>, _>>().unwrap());
        assert_eq!(sink.events, expected);
    }
}

#[test]
fn recovery_discards_metadata_for_a_node_that_failed_before_emission() {
    let source = "&abandoned !missing!tag value\n---\n[*abandoned, &kept healthy, *kept]\n";
    let options = granit_parser::options! {
        emit_anchor_events: true,
        emit_unresolved_aliases: true,
    };
    let mut parser = Parser::new_from_str_with_options(source, options);
    let mut failed = false;
    for event in parser.by_ref() {
        match event {
            Ok((Event::Anchor(..), _)) => panic!("failed node emitted anchor metadata"),
            Ok(_) => {}
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    assert!(failed);
    assert!(parser.next().is_none());
    parser.proceed_next_document().unwrap();
    let events = parser.collect::<Result<Vec<_>, _>>().unwrap();
    assert_anchor_follows_node(&events);
    let metadata: Vec<_> = events
        .iter()
        .filter(|(event, _)| matches!(event, Event::Anchor(..)))
        .collect();
    assert_eq!(metadata.len(), 1);
    assert!(matches!(&metadata[0].0, Event::Anchor(_, name) if name == "kept"));
    assert_eq!(metadata[0].1.slice(source), Some("&kept"));
    assert!(events
        .iter()
        .any(|(event, _)| matches!(event, Event::UnresolvedAlias(name) if name == "abandoned")));
}

#[test]
fn skipping_a_document_discards_a_peeked_anchor_event() {
    let source = "--- &skipped first\n--- &kept second\n";
    let mut parser = Parser::new_from_str_with_options(source, anchor_options());
    loop {
        if matches!(parser.peek().unwrap().unwrap().0, Event::Anchor(..)) {
            break;
        }
        parser.next().unwrap().unwrap();
    }
    parser.proceed_next_document().unwrap();
    let events = parser.collect::<Result<Vec<_>, _>>().unwrap();
    assert_anchor_follows_node(&events);
    let names: Vec<_> = events
        .iter()
        .filter_map(|(event, _)| match event {
            Event::Anchor(_, name) => Some(name.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["kept"]);
}

fn assert_anchor_precedes_late_error<'input, I: BorrowedInput<'input>>(
    mut parser: Parser<'input, I>,
) {
    let mut events = Vec::new();
    loop {
        match parser.next().expect("invalid tag must report an error") {
            Ok(event) => events.push(event),
            Err(error) => {
                assert_eq!(error.kind(), &ErrorKind::InvalidTagEscape);
                break;
            }
        }
    }
    assert_anchor_follows_node(&events);
    assert!(matches!(events.last(), Some((Event::Anchor(1, name), _)) if name == "a"));
    assert!(matches!(
        events[events.len() - 2].0,
        Event::SequenceStart(_, 1, _)
    ));
    assert!(parser.next().is_none());
    assert!(parser.peek().is_none());
}

#[test]
fn anchored_indentless_sequence_emits_metadata_before_a_later_scanner_error() {
    let source = "key: &a\n- !%";
    let options = granit_parser::options! {
        emit_anchor_events: true,
        emit_comments: true,
    };
    assert_anchor_precedes_late_error(Parser::new_from_str_with_options(source, options.clone()));
    assert_anchor_precedes_late_error(Parser::new_from_iter_with_options(
        source.chars(),
        options.clone(),
    ));
    assert_anchor_precedes_late_error(Parser::new_from_fallible_iter_with_options(
        source.chars().map(Ok::<_, ErrorKind>),
        options,
    ));
}
