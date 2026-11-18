use granit_parser::{
    BorrowedInput, BufferedInput, ErrorKind, Event, FallibleBufferedInput, Parser, ParserStack,
};
use std::borrow::Cow;
use std::iter::Empty;

fn collect_scalars<'input, T: BorrowedInput<'input>>(
    root: &'input str,
    streamed: T,
) -> Vec<Cow<'input, str>> {
    let mut stack: ParserStack<'input, Empty<char>, T> = ParserStack::new();
    stack.push_str_parser(Parser::new_from_str(root), "root.yaml".into());
    stack.push_custom_parser(Parser::new(streamed), "streamed.yaml".into());
    stack
        .filter_map(
            |result| match result.expect("stacked input should parse").0 {
                Event::Scalar { value, .. } => Some(value),
                _ => None,
            },
        )
        .collect()
}

fn assert_mixed_ownership(values: &[Cow<'_, str>], root: &str) {
    let [Cow::Owned(streamed), Cow::Borrowed(borrowed)] = values else {
        panic!("expected owned streamed text followed by borrowed root text: {values:?}");
    };
    assert_eq!(streamed, "streamed");
    assert_eq!(*borrowed, root);
    assert_eq!(borrowed.as_ptr(), root.as_ptr());
}

#[test]
fn buffered_input_shares_a_stack_with_nonstatic_borrowed_input() {
    let root = String::from("borrowed");
    let values = {
        // The streamed source can disappear before the resulting events are used.
        let source = String::from("streamed");
        collect_scalars(&root, BufferedInput::new(source.chars()))
    };
    assert_mixed_ownership(&values, &root);
}

#[test]
fn fallible_buffered_input_shares_a_stack_with_nonstatic_borrowed_input() {
    let root = String::from("borrowed");
    let values = {
        let source = String::from("streamed");
        collect_scalars(
            &root,
            FallibleBufferedInput::new(source.chars().map(Ok::<_, ErrorKind>)),
        )
    };
    assert_mixed_ownership(&values, &root);
}
