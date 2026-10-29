// TEMPORARY review probe. Delete after use.
use granit_parser::{options, Event, Options, Parser, ParserStack, ParserTrait, ReplayParser};

fn dump(label: &str, source: &str, options: Options, offset: Option<usize>) -> Vec<String> {
    let mut parser = Parser::new_from_str_with_options(source, options);
    if let Some(offset) = offset {
        parser.set_anchor_offset(offset);
    }
    let mut out = Vec::new();
    for item in parser.by_ref() {
        match item {
            Ok((event, span)) => out.push(format!("{event:?} @{:?}", span.slice(source))),
            Err(error) => {
                out.push(format!("ERR {:?}", error.kind()));
                break;
            }
        }
    }
    println!("=== {label} :: {source:?} offset={offset:?}");
    for line in &out {
        println!("    {line}");
    }
    out
}

fn anchor_opts(comments: bool) -> Options {
    options! { emit_anchor_events: true, emit_comments: comments }
}

#[test]
fn probe_offset_zero_comment_between_anchor_and_second_anchor() {
    // Expect: debug_assert in register_anchor may fire (double registration while pending).
    dump("offset0 anchor comment anchor", "&a # c\n&b x\n", anchor_opts(true), Some(0));
}

#[test]
fn probe_offset_zero_comment_then_tag_then_anchor() {
    dump("offset0 anchor comment tag anchor", "&a # c\n!t &b x\n", anchor_opts(true), Some(0));
}

#[test]
fn probe_offset_zero_duplicate_anchor_without_comment() {
    dump("offset0 dup anchors", "&a &b x\n", anchor_opts(true), Some(0));
    dump("offset0 anchor tag anchor", "&a !t &b x\n", anchor_opts(true), Some(0));
    dump("offset0 anchor newline anchor", "&a\n&b x\n", anchor_opts(false), Some(0));
}

#[test]
fn probe_offset_zero_single_pair_and_indentless() {
    dump("offset0 single pair", "[&a key: value]\n", anchor_opts(true), Some(0));
    dump("offset0 indentless", "key: &a # c\n- # c2\n- x\n", anchor_opts(true), Some(0));
    dump("offset0 empty anchored key", "? &a # c\n: &b v\n", anchor_opts(true), Some(0));
    dump("offset0 seq entries", "- &a # c\n- &b x\n", anchor_opts(true), Some(0));
    dump("offset0 anchored empty then more", "- &a\n- &b\n", anchor_opts(true), Some(0));
}

#[test]
fn probe_default_offset_duplicates_and_orderings() {
    for comments in [false, true] {
        dump("dup anchors", "&a &b x\n", anchor_opts(comments), None);
        dump("anchor tag anchor", "&a !t &b x\n", anchor_opts(comments), None);
        dump("anchor newline anchor", "&a\n&b x\n", anchor_opts(comments), None);
        dump("anchor comment anchor", "&a # c\n&b x\n", anchor_opts(comments), None);
        dump("anchor comment tag anchor", "&a # c\n!t &b x\n", anchor_opts(comments), None);
        dump("seq entries", "- &a # c\n- &b x\n", anchor_opts(comments), None);
        dump("explicit key", "? &a # c\n: &b v\n", anchor_opts(comments), None);
        dump("flow map", "{&a: &b}\n", anchor_opts(comments), None);
        dump("flow seq", "[&a, &b]\n", anchor_opts(comments), None);
        dump("anchored key value", "&a: &b v\n", anchor_opts(comments), None);
        dump("indentless", "key: &a # c\n- # c2\n- x\n", anchor_opts(comments), None);
        dump("single pair", "[&a key: value]\n", anchor_opts(comments), None);
        dump("anchor before doc end", "&a # c\n...\n--- &b x\n", anchor_opts(comments), None);
        dump("anchor empty doc", "--- &a\n--- &b x\n", anchor_opts(comments), None);
        dump("anchor comment eof", "&a # c", anchor_opts(comments), None);
        dump("anchor then error", "&a # c\n- [\n", anchor_opts(comments), None);
        dump("tag anchor comment seq", "key: !t &a # c\n- x\n", anchor_opts(comments), None);
        dump("anchored key with comment before value", "&a key: # c\n  &b v\n", anchor_opts(comments), None);
        dump("block seq nested", "- &a\n  - &b x\n", anchor_opts(comments), None);
        dump("anchor alias same line", "&a [*a]\n", anchor_opts(comments), None);
        dump("merge", "<<: &a {x: 1}\n", anchor_opts(comments), None);
    }
}

#[test]
fn probe_stack_offset_zero_include() {
    let mut stack: ParserStack<'_> = ParserStack::with_options(options! { emit_anchor_events: true });
    stack.set_borrowed_resolver(|_| Ok("&inc [*inc]\n"));
    stack.push_replay_parser(ReplayParser::new(Vec::new(), 0), "bottom".into());
    stack.push_include("inc").unwrap();
    let events: Vec<_> = stack.collect::<Result<Vec<_>, _>>().unwrap();
    println!("=== stack offset0 include");
    for (event, _) in &events {
        println!("    {event:?} anchor_id={:?}", event.anchor_id());
    }
}

#[test]
fn probe_proceed_next_document_after_last_anchored_node() {
    let source = "--- &a x\n--- &b y\n";
    let mut parser = Parser::new_from_str_with_options(source, anchor_opts(true));
    loop {
        let (event, _) = parser.next().unwrap().unwrap();
        if matches!(event, Event::Scalar(..)) {
            break;
        }
    }
    parser.proceed_next_document().unwrap();
    let rest: Vec<_> = parser.map(|r| format!("{:?}", r.unwrap().0)).collect();
    println!("=== proceed after last anchored node");
    for line in &rest {
        println!("    {line}");
    }
}

#[test]
fn probe_try_load_single_doc_twice() {
    #[derive(Default)]
    struct Sink(Vec<String>);
    impl<'input> granit_parser::EventReceiver<'input> for Sink {
        fn on_event(&mut self, ev: Event<'input>) {
            self.0.push(format!("{ev:?}"));
        }
    }
    let source = "--- &a x\n--- &b y\n";
    let mut parser = Parser::new_from_str_with_options(source, anchor_opts(true));
    let mut sink = Sink::default();
    parser.load(&mut sink, false).unwrap();
    println!("=== load(false) #1: {:?}", sink.0);
    sink.0.clear();
    parser.load(&mut sink, false).unwrap();
    println!("=== load(false) #2: {:?}", sink.0);
    sink.0.clear();
    parser.load(&mut sink, true).unwrap();
    println!("=== load(true) #3: {:?}", sink.0);
}

#[test]
fn probe_peek_anchor_then_load() {
    #[derive(Default)]
    struct Sink(Vec<String>);
    impl<'input> granit_parser::EventReceiver<'input> for Sink {
        fn on_event(&mut self, ev: Event<'input>) {
            self.0.push(format!("{ev:?}"));
        }
    }
    let source = "&a x\n";
    let mut parser = Parser::new_from_str_with_options(source, anchor_opts(true));
    // Consume through the scalar, peek the anchor, then load the rest.
    loop {
        let (event, _) = parser.next().unwrap().unwrap();
        if matches!(event, Event::Scalar(..)) {
            break;
        }
    }
    let peeked = format!("{:?}", parser.peek().unwrap().unwrap().0);
    let mut sink = Sink::default();
    parser.load(&mut sink, true).unwrap();
    println!("=== peeked {peeked} then load: {:?}", sink.0);
}
