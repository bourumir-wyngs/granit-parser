use granit_parser::{options, ErrorKind, Options, Parser, ScanError};

/// Drive the parser to completion and return the first error, if any.
fn first_error(yaml: &str, options: Options) -> Option<ScanError> {
    Parser::new_from_str_with_options(yaml, options).find_map(Result::err)
}

/// Drive an iterator-backed parser to completion and return the first error, if any.
fn first_iter_error(yaml: &str, options: Options) -> Option<ScanError> {
    Parser::new_from_iter_with_options(yaml.chars(), options).find_map(Result::err)
}

// ZYU8: Directive variants
// In YAML 1.2, a directive name is any non\u{2011}space, non\u{2011}line\u{2011}break sequence of characters
// Saphyr expects only alphabetic characters in a directive name, dot . triggers the error.
#[test]
fn yaml_zyu8_directive_variant_yaml11_null_document() {
    let yaml = "%YAML1.1\n---\n";
    let mut got_err: Option<ScanError> = None;

    for item in Parser::new_from_str(yaml) {
        match item {
            Ok((_event, _span)) => {}
            Err(e) => {
                got_err = Some(e);
                break;
            }
        }
    }
    assert!(got_err.is_none(), "Error: {}", got_err.unwrap().info());
}

#[test]
fn yaml_reserved_directive_stars() {
    let yaml = "%***\n---\n";
    let mut got_err: Option<ScanError> = None;

    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            got_err = Some(e);
            break;
        }
    }
    assert!(got_err.is_none(), "Error: {}", got_err.unwrap().info());
}

#[test]
fn yaml_bad_yaml_directive() {
    let yaml = "%YAML 1.1 1.2\n---\n";
    let mut got_err: Option<ScanError> = None;

    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            got_err = Some(e);
            break;
        }
    }
    // This should fail because "YAML" is a defined directive and it has too many parameters.
    assert!(got_err.is_some());
    assert!(got_err
        .unwrap()
        .info()
        .contains("did not find expected comment or line break"));
}

#[test]
fn yaml_reserved_directive_with_params() {
    let yaml = "%FOO bar baz\n---\n";
    let mut got_err: Option<ScanError> = None;

    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            got_err = Some(e);
            break;
        }
    }
    assert!(got_err.is_none(), "Error: {}", got_err.unwrap().info());
}

#[test]
fn yaml_reserved_directive_at_eof() {
    let yaml = "%FOO";
    let mut got_err: Option<ScanError> = None;

    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            got_err = Some(e);
            break;
        }
    }
    assert!(got_err.is_some(), "Expected an error");
    assert!(got_err
        .unwrap()
        .info()
        .contains("did not find expected <document start>"));
}

#[test]
fn yaml_reserved_directive_with_param_at_eof() {
    let yaml = "%FOO bar";
    let mut got_err: Option<ScanError> = None;

    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            got_err = Some(e);
            break;
        }
    }
    assert!(got_err.is_some(), "Expected an error");
    assert!(got_err
        .unwrap()
        .info()
        .contains("did not find expected <document start>"));
}

// The parser ignores reserved directives, but the scanner still has to materialize one `String`
// per parameter to build the token. Without a limit, `%X` followed by two-byte ` a` parameters
// buys an allocation and a vector slot for every two input bytes, all inside a single token and
// therefore before any event-level budget can see it.
#[test]
fn reserved_directive_params_are_capped_by_default() {
    let mut yaml = String::from("%X");
    for _ in 0..100_000 {
        yaml.push_str(" a");
    }
    yaml.push_str("\n---\n");

    let err = first_error(&yaml, Options::default()).expect("expected a limit error");
    assert!(
        err.info().contains("reserved directive exceeds"),
        "unexpected error: {}",
        err.info()
    );
}

#[test]
fn reserved_directive_param_count_limit_is_configurable() {
    let options = options! { max_reserved_directive_params: 2 };

    assert!(first_error("%X a b\n---\n", options.clone()).is_none());

    let err = first_error("%X a b c\n---\n", options).expect("expected a limit error");
    assert!(
        err.info()
            .contains("reserved directive exceeds the configured limit of 2 parameters"),
        "unexpected error: {}",
        err.info()
    );
}

// A single unbounded parameter is a weaker amplification than many small ones, but the scanner
// would still retain a `String` the size of the input for a token the parser discards.
#[test]
fn oversized_reserved_directive_param_is_rejected() {
    let mut yaml = String::from("%X ");
    yaml.push_str(&"a".repeat(100_000));
    yaml.push_str("\n---\n");

    let err = first_error(&yaml, Options::default()).expect("expected a limit error");
    assert!(
        err.info()
            .contains("directive exceeds the configured limit of 1024 bytes"),
        "unexpected error: {}",
        err.info()
    );
}

#[test]
fn oversized_directive_name_is_rejected() {
    let yaml = format!("%{}\n---\n", "A".repeat(100_000));

    let err = first_error(&yaml, Options::default()).expect("expected a limit error");
    assert!(
        err.info()
            .contains("directive exceeds the configured limit of 1024 bytes"),
        "unexpected error: {}",
        err.info()
    );
}

// The byte limit spans the whole directive, so parameters that individually fit still cannot add
// up to an unbounded token.
#[test]
fn reserved_directive_bytes_are_capped_across_params() {
    let options = options! { max_reserved_directive_params: usize::MAX };
    let mut yaml = String::from("%X");
    for _ in 0..100_000 {
        yaml.push_str(" aaaaaaaa");
    }
    yaml.push_str("\n---\n");

    let err = first_error(&yaml, options).expect("expected a limit error");
    assert!(
        err.info()
            .contains("directive exceeds the configured limit of 1024 bytes"),
        "unexpected error: {}",
        err.info()
    );
}

#[test]
fn directive_byte_limit_is_configurable_and_counts_multibyte_chars() {
    let options = options! { max_directive_bytes: 8 };

    // The name, the separating blank and three two-byte characters exactly fill the budget.
    assert!(first_error("%X ÿÿÿ\n---\n", options.clone()).is_none());

    let err = first_error("%X ÿÿÿÿ\n---\n", options).expect("expected a limit error");
    assert!(
        err.info()
            .contains("directive exceeds the configured limit of 8 bytes"),
        "unexpected error: {}",
        err.info()
    );
}

// Real directives are short; the defaults must not disturb them.
#[test]
fn ordinary_directives_are_unaffected_by_the_limits() {
    let yaml = "%YAML 1.2\n%TAG !e! tag:example.com,2000:app/\n%FOO bar baz\n---\nkey: value\n";

    assert!(first_error(yaml, Options::default()).is_none());
}

#[test]
fn tag_directive_handle_and_prefix_are_capped_for_borrowed_and_streaming_inputs() {
    let oversized = [
        format!("%TAG !{}! x\n---\n", "a".repeat(100_000)),
        format!("%TAG !e! tag:{}\n---\n", "a".repeat(100_000)),
    ];

    for yaml in &oversized {
        for error in [
            first_error(yaml, Options::default()),
            first_iter_error(yaml, Options::default()),
        ] {
            let error = error.expect("expected a directive byte-limit error");
            assert_eq!(
                error.kind(),
                &ErrorKind::DirectiveByteLimitExceeded { limit: 1024 }
            );
        }
    }
}

#[test]
fn tag_directive_byte_limit_counts_separators_and_raw_escape_bytes() {
    let yaml = "%TAG !e! %C3%BF\n---\n";
    let exact = options! { max_directive_bytes: 14 };

    assert!(first_error(yaml, exact.clone()).is_none());
    assert!(first_iter_error(yaml, exact).is_none());

    let too_small = options! { max_directive_bytes: 13 };
    for error in [
        first_error(yaml, too_small.clone()),
        first_iter_error(yaml, too_small),
    ] {
        let error = error.expect("expected a directive byte-limit error");
        assert_eq!(
            error.kind(),
            &ErrorKind::DirectiveByteLimitExceeded { limit: 13 }
        );
    }
}
