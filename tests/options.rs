use granit_parser::{ErrorKind, Event, Options, Parser, StrInput};

fn assert_comment_free_value(events: &[(Event<'_>, granit_parser::Span)]) {
    assert!(events
        .iter()
        .all(|(event, _)| !matches!(event, Event::Comment(..))));
    assert!(events
        .iter()
        .any(|(event, _)| { matches!(event, Event::Scalar(value, ..) if value == "value") }));
}

#[test]
fn options_macro_starts_with_defaults_and_applies_fields() {
    let defaults = Options::default();
    assert!(defaults.emit_comments);
    assert_eq!(defaults.max_buffered_comment_events, 32);
    assert_eq!(defaults.simple_key_max_lookahead, 1024);
    assert_eq!(defaults.flow_nesting_limit, 255);
    assert_eq!(granit_parser::options! {}, defaults);

    let options = granit_parser::options! {
        emit_comments: false,
        max_buffered_comment_events: 7,
        simple_key_max_lookahead: 11,
        flow_nesting_limit: 13,
    };

    assert!(!options.emit_comments);
    assert_eq!(options.max_buffered_comment_events, 7);
    assert_eq!(options.simple_key_max_lookahead, 11);
    assert_eq!(options.flow_nesting_limit, 13);
}

#[test]
fn options_macro_configures_parser() {
    let options = granit_parser::options! {
        flow_nesting_limit: 1,
    };
    let error = Parser::with_options(StrInput::new("[[]]"), options)
        .find_map(Result::err)
        .expect("macro-configured flow limit should be enforced");

    assert_eq!(error.kind(), &ErrorKind::RecursionLimitExceeded);
}

#[test]
fn new_uses_default_options() {
    let yaml = "root: [a, {b: c}]\n";
    let from_new: Vec<_> = Parser::new(StrInput::new(yaml)).collect();
    let from_options: Vec<_> =
        Parser::with_options(StrInput::new(yaml), Options::default()).collect();

    assert_eq!(from_new, from_options);
}

#[test]
fn common_input_constructors_accept_options() {
    let yaml = String::from("# ignored\nvalue\n");
    let options = granit_parser::options! {
        emit_comments: false,
    };

    let string_events = Parser::new_from_str_with_options(&yaml, options.clone())
        .collect::<Result<Vec<_>, _>>()
        .expect("string constructor should parse with custom options");
    assert_comment_free_value(&string_events);

    let iterator_events = Parser::new_from_iter_with_options(yaml.chars(), options.clone())
        .collect::<Result<Vec<_>, _>>()
        .expect("iterator constructor should parse with custom options");
    assert_comment_free_value(&iterator_events);

    let fallible_events =
        Parser::new_from_fallible_iter_with_options(yaml.chars().map(Ok), options)
            .collect::<Result<Vec<_>, _>>()
            .expect("fallible iterator constructor should parse with custom options");
    assert_comment_free_value(&fallible_events);
}

#[test]
fn buffered_comment_limit_can_be_raised() {
    let mut yaml = String::from("key: # c0\n");
    for index in 1..33 {
        yaml.push_str("# c");
        yaml.push_str(&index.to_string());
        yaml.push('\n');
    }
    yaml.push_str("next: value\n");

    let error = Parser::new(StrInput::new(&yaml))
        .find_map(Result::err)
        .expect("default options should reject comment 33");
    assert_eq!(error.kind(), &ErrorKind::TooManyComments);

    let options = granit_parser::options! {
        max_buffered_comment_events: 33,
    };
    let events = Parser::with_options(StrInput::new(&yaml), options)
        .collect::<Result<Vec<_>, _>>()
        .expect("custom options should accept 33 buffered comments");

    assert_eq!(
        events
            .iter()
            .filter(|(event, _)| matches!(event, Event::Comment(..)))
            .count(),
        33
    );
}

#[test]
fn buffered_comment_limit_honors_lower_and_zero_boundaries() {
    let one_comment = "key: # one\nnext: value\n";
    let options = granit_parser::options! {
        max_buffered_comment_events: 1,
    };
    Parser::with_options(StrInput::new(one_comment), options)
        .collect::<Result<Vec<_>, _>>()
        .expect("one buffered comment should be accepted at limit one");

    let two_comments = "key: # one\n# two\nnext: value\n";
    let options = granit_parser::options! {
        max_buffered_comment_events: 1,
    };
    let error = Parser::with_options(StrInput::new(two_comments), options)
        .find_map(Result::err)
        .expect("the second buffered comment should exceed limit one");
    assert_eq!(error.kind(), &ErrorKind::TooManyComments);

    let options = granit_parser::options! {
        max_buffered_comment_events: 0,
    };
    let error = Parser::with_options(StrInput::new(one_comment), options)
        .find_map(Result::err)
        .expect("the first buffered comment should exceed limit zero");
    assert_eq!(error.kind(), &ErrorKind::TooManyComments);
}

#[test]
fn zero_comment_limit_still_precedes_indentless_sequence_start() {
    let options = granit_parser::options! {
        max_buffered_comment_events: 0,
    };
    let mut events = Vec::new();
    let error = Parser::with_options(StrInput::new("a:\n- # one\n"), options)
        .find_map(|result| match result {
            Ok((event, _)) => {
                events.push(event);
                None
            }
            Err(error) => Some(error),
        })
        .expect("the first buffered comment should exceed limit zero");

    assert_eq!(error.kind(), &ErrorKind::TooManyComments);
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, Event::SequenceStart(..))),
        "the resource-limit error must not be deferred behind the sequence start"
    );
}

#[test]
fn simple_key_lookahead_can_be_raised() {
    let key = "k".repeat(1025);
    let yaml = format!("a: b\n{key}: value\n");

    let error = Parser::new(StrInput::new(&yaml))
        .find_map(Result::err)
        .expect("default options should reject a 1025-character simple key");
    assert_eq!(error.kind(), &ErrorKind::SimpleKeyExpected);

    let options = granit_parser::options! {
        simple_key_max_lookahead: 1025,
    };
    Parser::with_options(StrInput::new(&yaml), options)
        .collect::<Result<Vec<_>, _>>()
        .expect("custom options should accept a 1025-character simple key");
}

#[test]
fn simple_key_lookahead_honors_configured_boundary() {
    let yaml = "a: b\nlong: value\n";
    let options = granit_parser::options! {
        simple_key_max_lookahead: 4,
    };
    Parser::with_options(StrInput::new(yaml), options)
        .collect::<Result<Vec<_>, _>>()
        .expect("a simple key at the configured limit should be accepted");

    let options = granit_parser::options! {
        simple_key_max_lookahead: 3,
    };
    let error = Parser::with_options(StrInput::new(yaml), options)
        .find_map(Result::err)
        .expect("a simple key beyond the configured limit should be rejected");
    assert_eq!(error.kind(), &ErrorKind::SimpleKeyExpected);
}

#[test]
fn flow_nesting_limit_can_be_raised_above_previous_type_limit() {
    let yaml = format!("{}{}", "[".repeat(256), "]".repeat(256));

    let error = Parser::new(StrInput::new(&yaml))
        .find_map(Result::err)
        .expect("default options should reject flow nesting level 256");
    assert_eq!(error.kind(), &ErrorKind::RecursionLimitExceeded);

    let options = granit_parser::options! {
        flow_nesting_limit: 256,
    };
    let events = Parser::with_options(StrInput::new(&yaml), options)
        .collect::<Result<Vec<_>, _>>()
        .expect("custom options should accept flow nesting level 256");

    assert_eq!(
        events
            .iter()
            .filter(|(event, _)| matches!(event, Event::SequenceStart(..)))
            .count(),
        256
    );
}

#[test]
fn flow_nesting_limit_honors_lower_and_zero_boundaries() {
    let options = granit_parser::options! {
        flow_nesting_limit: 1,
    };
    Parser::with_options(StrInput::new("[]"), options)
        .collect::<Result<Vec<_>, _>>()
        .expect("one flow collection should be accepted at limit one");

    let options = granit_parser::options! {
        flow_nesting_limit: 1,
    };
    let error = Parser::with_options(StrInput::new("[[]]"), options)
        .find_map(Result::err)
        .expect("the second flow collection should exceed limit one");
    assert_eq!(error.kind(), &ErrorKind::RecursionLimitExceeded);

    let options = granit_parser::options! {
        flow_nesting_limit: 0,
    };
    let error = Parser::with_options(StrInput::new("[]"), options)
        .find_map(Result::err)
        .expect("the first flow collection should exceed limit zero");
    assert_eq!(error.kind(), &ErrorKind::RecursionLimitExceeded);
}
