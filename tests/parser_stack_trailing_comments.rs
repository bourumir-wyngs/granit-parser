use granit_parser::{
    ErrorKind, Event, Options, Parser, ParserStack, ParserTrait, ReplayParser, ScanError, Span,
    StrInput,
};

type Stack = ParserStack<'static, std::vec::IntoIter<char>, StrInput<'static>>;

const CHILD: &str = "child\n... # trailing\n# after\n";
const TWO_DOCUMENTS: &str = "child\n... # trailing\n# after\n---\nsecond\n";
const PARENT: &str = "parent\n";

#[derive(Clone, Copy, Debug)]
enum Backend {
    String,
    Iterator,
    Custom,
    Replay,
    ReplayWithoutStreamEnd,
    OwnedInclude,
    BorrowedInclude,
}

const BACKENDS: [Backend; 7] = [
    Backend::String,
    Backend::Iterator,
    Backend::Custom,
    Backend::Replay,
    Backend::ReplayWithoutStreamEnd,
    Backend::OwnedInclude,
    Backend::BorrowedInclude,
];

fn ordinary_events(yaml: &'static str) -> Vec<(Event<'static>, Span)> {
    Parser::new_from_str(yaml)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn nested_events(yaml: &'static str) -> Vec<(Event<'static>, Span)> {
    let mut document_starts = 0;
    ordinary_events(yaml)
        .into_iter()
        .take_while(|(event, _)| {
            if matches!(event, Event::DocumentStart(..)) {
                document_starts += 1;
            }
            document_starts < 2
        })
        .filter(|(event, _)| {
            !matches!(
                event,
                Event::StreamStart
                    | Event::StreamEnd
                    | Event::DocumentStart(..)
                    | Event::DocumentEnd
            )
        })
        .collect()
}

fn stack_with_child(backend: Backend, yaml: &'static str, options: Options) -> Stack {
    let mut stack = Stack::with_options(options);
    stack.push_str_parser(Parser::new_from_str(PARENT), "parent.yaml".to_owned());
    match backend {
        Backend::String => {
            stack.push_str_parser(Parser::new_from_str(yaml), "child.yaml".to_owned());
        }
        Backend::Iterator => {
            let chars = yaml.chars().collect::<Vec<_>>().into_iter();
            stack.push_iter_parser(Parser::new_from_iter(chars), "child.yaml".to_owned());
        }
        Backend::Custom => {
            stack.push_custom_parser(Parser::new(StrInput::new(yaml)), "child.yaml".to_owned());
        }
        Backend::Replay | Backend::ReplayWithoutStreamEnd => {
            let mut events = ordinary_events(yaml);
            if matches!(backend, Backend::ReplayWithoutStreamEnd) {
                assert!(matches!(events.pop().unwrap().0, Event::StreamEnd));
            }
            stack.push_replay_parser(ReplayParser::new(events, 1), "child.yaml".to_owned());
        }
        Backend::OwnedInclude => {
            stack.set_resolver(move |_| Ok(yaml.to_owned()));
            stack.push_include("child.yaml").unwrap();
        }
        Backend::BorrowedInclude => {
            stack.set_borrowed_resolver(move |_| Ok(yaml));
            stack.push_include("child.yaml").unwrap();
        }
    }
    stack
}

fn next_with_repeated_peek(stack: &mut Stack) -> Option<Result<(Event<'static>, Span), ScanError>> {
    let peeked = stack.peek().map(Result::<&_, _>::cloned);
    assert_eq!(stack.peek().map(Result::<&_, _>::cloned), peeked);
    let next = stack.next_event();
    assert_eq!(next, peeked);
    next
}

fn first_document_end(yaml: &'static str) -> Span {
    ordinary_events(yaml)
        .into_iter()
        .find(|(event, _)| matches!(event, Event::DocumentEnd))
        .unwrap()
        .1
}

fn assert_multiple_documents_error(stack: &mut Stack, error: &ScanError) {
    assert_eq!(error.kind(), &ErrorKind::MultipleDocumentsUnsupported);
    assert_eq!(*error.marker(), first_document_end(TWO_DOCUMENTS).start);
    assert_eq!(error.source_stack(), ["parent.yaml", "child.yaml"]);
    assert_eq!(stack.stack(), ["parent.yaml"]);
    assert!(stack.next_event().is_none());
    assert!(stack.peek().is_none());
}

#[test]
fn included_trailing_comments_are_emitted_before_parent_resumes() {
    let mut expected = nested_events(CHILD);
    expected.extend(ordinary_events(PARENT));

    for backend in BACKENDS {
        let mut stack = stack_with_child(backend, CHILD, Options::default());
        let mut actual = Vec::new();
        while let Some(event) = next_with_repeated_peek(&mut stack) {
            actual.push(event.unwrap());
        }
        assert_eq!(actual, expected, "{backend:?}");
        assert!(stack.stack().is_empty());
    }
}

#[test]
fn trailing_comments_do_not_hide_a_second_included_document() {
    for backend in BACKENDS {
        for emit_comments in [true, false] {
            let options = granit_parser::options! { emit_comments: emit_comments };
            let mut stack = stack_with_child(backend, TWO_DOCUMENTS, options);
            let mut actual = Vec::new();
            let error = loop {
                match next_with_repeated_peek(&mut stack).expect("second document must fail") {
                    Ok(event) => actual.push(event),
                    Err(error) => break error,
                }
            };
            let expected = nested_events(TWO_DOCUMENTS)
                .into_iter()
                .filter(|(event, _)| emit_comments || !matches!(event, Event::Comment(..)))
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "{backend:?}, emit_comments={emit_comments}"
            );
            assert_multiple_documents_error(&mut stack, &error);
        }
    }
}

#[test]
fn pushing_during_a_peeked_trailing_comment_preserves_each_sources_validation() {
    const GRANDCHILD: &str = "grandchild\n... # grandchild tail\n";

    for child in [CHILD, TWO_DOCUMENTS] {
        let mut stack = stack_with_child(Backend::String, child, Options::default());
        let expected_child = nested_events(child);
        assert_eq!(stack.next_event().unwrap().unwrap(), expected_child[0]);

        assert_eq!(stack.peek().unwrap().unwrap(), &expected_child[1]);
        assert_eq!(stack.peek().unwrap().unwrap(), &expected_child[1]);
        stack.push_str_parser(
            Parser::new_from_str(GRANDCHILD),
            "grandchild.yaml".to_owned(),
        );
        assert_eq!(stack.next_event().unwrap().unwrap(), expected_child[1]);

        let mut expected = nested_events(GRANDCHILD);
        expected.extend(expected_child.into_iter().skip(2));
        for event in expected {
            assert_eq!(next_with_repeated_peek(&mut stack).unwrap().unwrap(), event);
        }

        if child == TWO_DOCUMENTS {
            let error = next_with_repeated_peek(&mut stack).unwrap().unwrap_err();
            assert_multiple_documents_error(&mut stack, &error);
        } else {
            let mut resumed = Vec::new();
            while let Some(event) = next_with_repeated_peek(&mut stack) {
                resumed.push(event.unwrap());
            }
            assert_eq!(resumed, ordinary_events(PARENT));
        }
    }
}

#[test]
fn scan_errors_after_trailing_comments_keep_their_kind_and_source() {
    const INVALID_TAIL: &str = "child\n... # trailing\n[\n";

    for backend in [Backend::String, Backend::Iterator, Backend::Custom] {
        let mut stack = stack_with_child(backend, INVALID_TAIL, Options::default());
        let expected = nested_events(CHILD);
        assert_eq!(stack.next_event().unwrap().unwrap(), expected[0]);
        assert_eq!(
            next_with_repeated_peek(&mut stack).unwrap().unwrap(),
            expected[1]
        );

        let error = next_with_repeated_peek(&mut stack).unwrap().unwrap_err();
        assert_eq!(
            error.kind(),
            &ErrorKind::UnclosedFlowCollection { open: '[' }
        );
        assert_eq!(error.source_stack(), ["parent.yaml", "child.yaml"]);
        assert_eq!(stack.stack(), ["parent.yaml"]);
        assert!(stack.next_event().is_none());
        assert!(stack.peek().is_none());
    }
}
