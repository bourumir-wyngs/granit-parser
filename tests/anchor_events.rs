use std::borrow::Cow;

use granit_parser::{
    BorrowedInput, ErrorKind, Event, Options, Parser, ScalarStyle, Span, SpannedEventReceiver,
    StructureStyle, TryLoadError, TrySpannedEventReceiver,
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
            let node_id = match &events[index - 1].0 {
                Event::Scalar(_, _, id, _)
                | Event::SequenceStart(_, id, _)
                | Event::MappingStart(_, id, _) => *id,
                event => panic!("anchor metadata followed {event:?}"),
            };
            assert_eq!(node_id, *id);
            assert_eq!(span.indent, None);
            assert_eq!(span.tag_span(), None);
            assert!(!event.is_node());
            assert!(!event.is_alias());
            assert_eq!(event.anchor_id(), (*id != 0).then_some(*id));
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

#[derive(Debug, PartialEq)]
enum AnchoredNode<'a> {
    Scalar(&'a str),
    Sequence(StructureStyle),
    Mapping(StructureStyle),
}

fn assert_anchored_nodes(
    source: &str,
    events: &[(Event<'_>, Span)],
    expected_nodes: &[AnchoredNode<'_>],
    offset: usize,
) {
    use AnchoredNode::{Mapping, Scalar, Sequence};
    assert_anchor_follows_node(events);
    // These fixtures use `*` only for alias tokens, all of which must be emitted.
    assert_eq!(
        events.iter().filter(|(event, _)| event.is_alias()).count(),
        source.matches('*').count(),
        "{source:?}"
    );
    let mut nodes = Vec::new();
    for (index, (event, span)) in events.iter().enumerate() {
        if let Event::Anchor(id, name) = event {
            assert_eq!(*id, offset + nodes.len(), "{source:?}");
            let expected_name = ["a", "b", "c"][nodes.len()];
            assert_eq!(name, expected_name, "{source:?}");
            let token = format!("&{expected_name}");
            let start = source.find(&token).unwrap();
            assert_eq!(span.slice(source), Some(token.as_str()));
            assert_eq!(span.byte_range(), Some(start..start + token.len()));
            let (node, node_span) = &events[index - 1];
            nodes.push(match node {
                Event::Scalar(value, ScalarStyle::Plain, _, _) => Scalar(value),
                Event::SequenceStart(style, _, _) => Sequence(*style),
                Event::MappingStart(style, _, _) => Mapping(*style),
                other => panic!("unexpected anchored node {other:?}"),
            });
            if source.contains("!t") {
                assert_eq!(node.tag().unwrap().original(), "!t");
                assert_eq!(node_span.tag_span().unwrap().slice(source), Some("!t"));
            }
        } else if let Event::Alias(id) = event {
            let name = span.slice(source).unwrap().strip_prefix('*').unwrap();
            let index = ["a", "b", "c"]
                .iter()
                .position(|item| *item == name)
                .unwrap();
            assert_eq!(*id, offset + index);
        }
    }
    assert_eq!(nodes, expected_nodes, "{source:?}, offset {offset}");
}

#[test]
fn every_anchored_node_kind_emits_metadata_immediately_after_its_node_event() {
    use AnchoredNode::{Mapping, Scalar, Sequence};
    use StructureStyle::{Block, Flow};

    let cases: &[(&str, &[AnchoredNode<'_>])] = &[
        ("&a value\n", &[Scalar("value")]),
        ("&a\n", &[Scalar("~")]),
        ("[&a, next]\n", &[Scalar("~")]),
        ("key: &a\nnext: value\n", &[Scalar("~")]),
        ("&a []\n", &[Sequence(Flow)]),
        ("&a {}\n", &[Mapping(Flow)]),
        ("&a [value]\n", &[Sequence(Flow)]),
        ("&a {key: value}\n", &[Mapping(Flow)]),
        ("&a\n- value\n", &[Sequence(Block)]),
        ("&a\nkey: value\n", &[Mapping(Block)]),
        ("key: &a\n- value\n", &[Sequence(Block)]),
        ("key: &a\n-\nnext: value\n", &[Sequence(Block)]),
        ("? &a key\n: value\n", &[Scalar("key")]),
        ("[&a key: value]\n", &[Scalar("key")]),
        (
            "&a [*a, &b {key: &c value}, *b, *c]\n",
            &[Sequence(Flow), Mapping(Flow), Scalar("value")],
        ),
        ("key: &a # c\n- # c2\n- x\n", &[Sequence(Block)]),
        ("? &a # c\n: &b v\n", &[Scalar("~"), Scalar("v")]),
        ("- &a # c\n- &b x\n", &[Scalar("~"), Scalar("x")]),
        ("- &a\n- &b\n", &[Scalar("~"), Scalar("~")]),
        ("[&a, &b]\n", &[Scalar("~"), Scalar("~")]),
        ("&a # c\n...\n--- &b x\n", &[Scalar("~"), Scalar("x")]),
        ("--- &a\n--- &b x\n", &[Scalar("~"), Scalar("x")]),
        ("&a # c", &[Scalar("~")]),
        ("key: !t &a # c\n- x\n", &[Sequence(Block)]),
        ("&a key: # c\n  &b v\n", &[Scalar("key"), Scalar("v")]),
        ("- &a\n  - &b x\n", &[Sequence(Block), Scalar("x")]),
        ("&a [*a]\n", &[Sequence(Flow)]),
        ("<<: &a {x: 1}\n", &[Mapping(Flow)]),
    ];
    for &(source, expected_nodes) in cases {
        for offset in [0, 1] {
            for emit_comments in [false, true] {
                let options = granit_parser::options! {
                    emit_anchor_events: true,
                    emit_comments: emit_comments,
                };
                let mut parser = Parser::new_from_str_with_options(source, options);
                if offset == 0 {
                    parser.set_anchor_offset(0);
                }
                let events = parser
                    .by_ref()
                    .collect::<Result<Vec<_>, _>>()
                    .expect(source);
                assert_anchored_nodes(source, &events, expected_nodes, offset);
                assert_eq!(parser.anchor_offset(), offset + expected_nodes.len());
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
fn anchor_offsets_reject_duplicate_properties_and_properties_on_aliases() {
    for (source, expected_error) in [
        ("&a # comment\n&b value", ErrorKind::ExpectedDocumentStart),
        (
            "&a # anchor\n!note &b value\n",
            ErrorKind::ExpectedDocumentStart,
        ),
        (
            "!note &a # properties\n&b value\n",
            ErrorKind::ExpectedDocumentStart,
        ),
        (
            "!note # tag\n&a # anchor\n&b value\n",
            ErrorKind::ExpectedDocumentStart,
        ),
        ("&a # anchor\n*a\n", ErrorKind::ExpectedDocumentStart),
        ("&a &b x\n", ErrorKind::ExpectedDocumentStart),
        ("&a !t &b x\n", ErrorKind::ExpectedDocumentStart),
        ("&a\n&b x\n", ErrorKind::ExpectedDocumentStart),
        // The colon belongs to the anchor name, so these also contain two properties on one node.
        ("&a: &b v\n", ErrorKind::ExpectedDocumentStart),
        ("{&a: &b}\n", ErrorKind::ExpectedFlowMappingSeparator),
    ] {
        for offset in [0, 1] {
            for emit_anchor_events in [true, false] {
                for emit_comments in [true, false] {
                    let options = granit_parser::options! {
                        emit_anchor_events: emit_anchor_events,
                        emit_comments: emit_comments,
                    };
                    let mut parser = Parser::new_from_str_with_options(source, options.clone());
                    let mut streamed = Parser::new_from_iter_with_options(source.chars(), options);
                    if offset == 0 {
                        parser.set_anchor_offset(0);
                        streamed.set_anchor_offset(0);
                    }
                    for result in [
                        parser.by_ref().collect::<Result<Vec<_>, _>>(),
                        streamed.by_ref().collect::<Result<Vec<_>, _>>(),
                    ] {
                        let error = result.expect_err(source);
                        assert_eq!(error.kind(), &expected_error, "{source:?}");
                    }
                    assert!(parser.next().is_none());
                    assert!(streamed.next().is_none());
                }
            }
        }
    }
}

#[test]
fn zero_anchor_offset_preserves_empty_scalars_and_tags_across_comments() {
    for (source, value, tagged) in [
        ("&a\n", "~", false),
        ("&a # empty\n", "~", false),
        ("[&a, *a]\n", "~", false),
        ("&a # anchor\n!note # empty\n", "", true),
        ("!note # tag\n&a # empty\n", "", true),
        ("&a # anchor\n!note # tag\nvalue\n", "value", true),
        ("!note # tag\n&a # anchor\nvalue\n", "value", true),
    ] {
        for emit_anchor_events in [false, true] {
            for emit_comments in [false, true] {
                let options = granit_parser::options! {
                    emit_anchor_events: emit_anchor_events,
                    emit_comments: emit_comments,
                };
                let mut parser = Parser::new_from_str_with_options(source, options.clone());
                parser.set_anchor_offset(0);
                let mut streamed = Parser::new_from_iter_with_options(source.chars(), options);
                streamed.set_anchor_offset(0);
                let expected = parser.collect::<Result<Vec<_>, _>>().expect(source);
                let actual = streamed.collect::<Result<Vec<_>, _>>().expect(source);
                assert_eq!(actual, expected, "{source:?}");
                let index = expected
                    .iter()
                    .position(|(event, _)| matches!(event, Event::Scalar(..)))
                    .unwrap();
                let (Event::Scalar(actual, _, id, tag), span) = &expected[index] else {
                    unreachable!()
                };
                assert_eq!(actual, value, "{source:?}");
                assert_eq!(*id, 0);
                assert_eq!(
                    tag.as_ref().map(|tag| tag.original()),
                    tagged.then(|| "!note".to_owned())
                );
                assert_eq!(
                    span.tag_span().and_then(|span| span.slice(source)),
                    tagged.then_some("!note")
                );
                let anchors: Vec<_> = expected
                    .iter()
                    .enumerate()
                    .filter(|(_, (event, _))| matches!(event, Event::Anchor(..)))
                    .collect();
                if emit_anchor_events {
                    assert_eq!(anchors.len(), 1);
                    let (anchor_index, (event, span)) = anchors[0];
                    assert_eq!(anchor_index, index + 1);
                    assert_eq!(*event, Event::Anchor(0, "a".into()));
                    assert_eq!(span.slice(source), Some("&a"));
                } else {
                    assert_eq!(anchors, []);
                }
                if source.contains("*a") {
                    assert!(expected.iter().any(|(event, _)| *event == Event::Alias(0)));
                }
            }
        }
    }
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

#[test]
fn loading_documents_separately_delivers_each_anchor_once() {
    let source = "--- &a x\n--- &b y\n";
    for offset in [0, 1] {
        let mut parser = Parser::new_from_str_with_options(source, anchor_options());
        parser.set_anchor_offset(offset);
        let mut sink = Events::default();
        for (index, (name, value)) in [("a", "x"), ("b", "y")].into_iter().enumerate() {
            parser.load(&mut sink, false).unwrap();
            let id = offset + index;
            let mut expected = vec![
                Event::DocumentStart(true, None),
                Event::Scalar(value.into(), ScalarStyle::Plain, id, None),
                Event::Anchor(id, name.into()),
                Event::DocumentEnd,
            ];
            if index == 0 {
                expected.insert(0, Event::StreamStart);
            }
            assert_eq!(
                sink.0.iter().map(|(event, _)| event).collect::<Vec<_>>(),
                expected.iter().collect::<Vec<_>>()
            );
            assert_anchor_follows_node(&sink.0);
            let (_, span) = sink
                .0
                .iter()
                .find(|(event, _)| matches!(event, Event::Anchor(..)))
                .unwrap();
            assert_eq!(span.slice(source), Some(format!("&{name}").as_str()));
            sink.0.clear();
        }
        parser.load(&mut sink, true).unwrap();
        assert_eq!(sink.0.len(), 1);
        assert_eq!(sink.0[0].0, Event::StreamEnd);
        assert!(parser.next().is_none());
    }
}

#[test]
fn loading_after_peeking_anchor_delivers_the_metadata_once() {
    let source = "&a x\n";
    for offset in [0, 1] {
        let mut parser = Parser::new_from_str_with_options(source, anchor_options());
        parser.set_anchor_offset(offset);
        loop {
            let (event, _) = parser.next().unwrap().unwrap();
            if matches!(event, Event::Scalar(..)) {
                assert_eq!(
                    event,
                    Event::Scalar("x".into(), ScalarStyle::Plain, offset, None)
                );
                break;
            }
        }
        let (event, span) = parser.peek().unwrap().unwrap();
        assert_eq!(*event, Event::Anchor(offset, "a".into()));
        assert_eq!(span.slice(source), Some("&a"));
        let mut sink = Events::default();
        parser.load(&mut sink, true).unwrap();
        assert_eq!(
            sink.0
                .iter()
                .map(|(event, _)| event.clone())
                .collect::<Vec<_>>(),
            [
                Event::Anchor(offset, "a".into()),
                Event::DocumentEnd,
                Event::StreamEnd
            ]
        );
        assert_eq!(sink.0[0].1.slice(source), Some("&a"));
        assert!(parser.next().is_none());
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
fn skipping_a_document_discards_pending_or_peeked_anchor_metadata() {
    let source = "--- &skipped first\n--- &kept second\n";
    for offset in [0, 1] {
        for peek_metadata in [false, true] {
            let mut parser = Parser::new_from_str_with_options(source, anchor_options());
            parser.set_anchor_offset(offset);
            loop {
                if matches!(parser.next().unwrap().unwrap().0, Event::Scalar(..)) {
                    break;
                }
            }
            if peek_metadata {
                assert_eq!(
                    parser.peek().unwrap().unwrap().0,
                    Event::Anchor(offset, "skipped".into())
                );
            }
            parser.proceed_next_document().unwrap();
            let events = parser.collect::<Result<Vec<_>, _>>().unwrap();
            assert_anchor_follows_node(&events);
            assert_eq!(
                events
                    .iter()
                    .map(|(event, _)| event.clone())
                    .collect::<Vec<_>>(),
                [
                    Event::DocumentStart(true, None),
                    Event::Scalar("second".into(), ScalarStyle::Plain, offset + 1, None),
                    Event::Anchor(offset + 1, "kept".into()),
                    Event::DocumentEnd,
                    Event::StreamEnd,
                ]
            );
            assert_eq!(events[2].1.slice(source), Some("&kept"));
        }
    }
}

#[test]
fn anchored_collection_emits_metadata_before_an_unclosed_child_error() {
    let source = "&a # c\n- [\n";
    for offset in [0, 1] {
        for emit_comments in [false, true] {
            let options = granit_parser::options! {
                emit_anchor_events: true,
                emit_comments: emit_comments,
            };
            let mut parser = Parser::new_from_str_with_options(source, options);
            parser.set_anchor_offset(offset);
            let mut events = Vec::new();
            loop {
                match parser.next().expect("unclosed child must produce an error") {
                    Ok(event) => events.push(event),
                    Err(error) => {
                        assert_eq!(
                            error.kind(),
                            &ErrorKind::UnclosedFlowCollection { open: '[' }
                        );
                        break;
                    }
                }
            }
            assert_anchored_nodes(
                source,
                &events,
                &[AnchoredNode::Sequence(StructureStyle::Block)],
                offset,
            );
            assert_eq!(events.last().unwrap().0, Event::Anchor(offset, "a".into()));
            assert!(parser.next().is_none());
        }
    }
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
