use granit_parser::{ErrorKind, Event, Parser, ScanError, Scanner, StrInput};

fn events_with_inputs(
    yaml: &str,
    strict_indentation: bool,
) -> Vec<Result<Vec<Event<'_>>, ScanError>> {
    let options = granit_parser::options! { strict_indentation: strict_indentation };
    [
        Parser::with_options(StrInput::new(yaml), options.clone()).collect::<Result<Vec<_>, _>>(),
        Parser::new_from_str_with_options(yaml, options.clone()).collect::<Result<Vec<_>, _>>(),
        Parser::new_from_iter_with_options(yaml.chars(), options.clone())
            .collect::<Result<Vec<_>, _>>(),
        Parser::new_from_fallible_iter_with_options(yaml.chars().map(Ok), options)
            .collect::<Result<Vec<_>, _>>(),
    ]
    .into_iter()
    .map(|result| result.map(|events| events.into_iter().map(|(event, _)| event).collect()))
    .collect()
}

fn assert_relaxed_only(yaml: &str, reference: &str, line: usize, col: usize) {
    let expected = Parser::new_from_str(reference)
        .map(|result| result.expect("valid reference").0)
        .collect::<Vec<_>>();
    let default = Parser::new_from_str(yaml)
        .map(|result| {
            result
                .unwrap_or_else(|error| panic!("default rejected {yaml:?}: {error}"))
                .0
        })
        .collect::<Vec<_>>();
    assert_eq!(default, expected, "default changed events for {yaml:?}");

    for result in events_with_inputs(yaml, false) {
        assert_eq!(
            result.unwrap_or_else(|error| panic!("relaxed mode rejected {yaml:?}: {error}")),
            expected,
            "relaxed mode changed events for {yaml:?}"
        );
    }
    Scanner::new(StrInput::new(yaml))
        .collect::<Result<Vec<_>, _>>()
        .expect("default scanner should accept relaxed indentation");
    Scanner::with_options(
        StrInput::new(yaml),
        granit_parser::options! { strict_indentation: false },
    )
    .collect::<Result<Vec<_>, _>>()
    .expect("explicit relaxed scanner should accept relaxed indentation");

    for result in events_with_inputs(yaml, true) {
        assert_indent_error(
            &result.expect_err("strict parser accepted relaxed indentation"),
            yaml,
            line,
            col,
        );
    }
    let error = Scanner::with_options(
        StrInput::new(yaml),
        granit_parser::options! { strict_indentation: true },
    )
    .find_map(Result::err)
    .expect("strict scanner accepted relaxed indentation");
    assert_indent_error(&error, yaml, line, col);

    // The corresponding conforming spelling must also work through every input backend.
    for result in events_with_inputs(reference, true) {
        assert_eq!(
            result.unwrap_or_else(|error| panic!(
                "strict mode rejected reference {reference:?}: {error}"
            )),
            expected
        );
    }
}

fn assert_indent_error(error: &ScanError, yaml: &str, line: usize, col: usize) {
    assert_eq!(
        error.kind(),
        &ErrorKind::InvalidIndentation,
        "input: {yaml:?}"
    );
    assert_eq!(
        (error.marker().line(), error.marker().col()),
        (line, col),
        "input: {yaml:?}"
    );
}

#[test]
fn strict_rejects_under_indented_flow_delimiters() {
    for (yaml, reference, line, col) in [
        ("key: [\n]\n", "key: [\n ]\n", 2, 0),
        ("key: {\n}\n", "key: {\n }\n", 2, 0),
        ("key: [\n a\n, b\n ]\n", "key: [\n a\n , b\n ]\n", 3, 0),
        ("outer:\n  key: [\n  ]\n", "outer:\n  key: [\n   ]\n", 3, 2),
        ("outer:\n  key: {\n }\n", "outer:\n  key: {\n   }\n", 3, 1),
        (
            "outer:\n  key: [one\n  , two]\n",
            "outer:\n  key: [one\n   , two]\n",
            3,
            2,
        ),
    ] {
        assert_relaxed_only(yaml, reference, line, col);
    }
}

#[test]
fn strict_rejects_under_indented_flow_entries_in_all_scalar_styles() {
    // The flow indentation depends on its enclosing block, including a compact mapping
    // inside a sequence. Equal indentation is insufficient in strict YAML.
    for (prefix, parent_indent, suffix) in [
        ("key: [", 0, "]\nafter: done\n"),
        ("- targets: [", 2, "]\n  after: done\n- next\n"),
        ("outer:\n  inner: [", 2, "]\n  after: done\nlast: end\n"),
        ("key:\n  [", 0, "]\nafter: done\n"),
    ] {
        for quote in ["", "'", "\""] {
            let value = format!("{quote}192.168.1.1:9100{quote}");
            let reference = format!("{prefix}{value}{suffix}");
            let line = prefix.lines().count() + 1;
            let closing_indent = " ".repeat(parent_indent + 1);
            for indent in 0..=parent_indent {
                let spaces = " ".repeat(indent);
                let yaml = format!("{prefix}\n{spaces}{value}\n{closing_indent}{suffix}");
                assert_relaxed_only(&yaml, &reference, line, indent);
            }
        }
    }
}

#[test]
fn strict_rejects_under_indented_mapping_entries_and_nested_collections() {
    for (yaml, reference, line, col) in [
        ("key: {\none: value\n }\n", "key: {one: value}\n", 2, 0),
        (
            "outer:\n  key: {\n  \"one\": value\n   }\n",
            "outer:\n  key: {\"one\": value}\n",
            3,
            2,
        ),
        ("key: {one:\nvalue\n }\n", "key: {one: value}\n", 2, 0),
        ("key: [\n[nested]\n ]\n", "key: [[nested]]\n", 2, 0),
        (
            "key: [\n{nested: value}\n ]\n",
            "key: [{nested: value}]\n",
            2,
            0,
        ),
        (
            "key: {\n? name: value\n }\n",
            "key: {? name: value}\n",
            2,
            0,
        ),
        (
            "key: { ? name\n:\n value\n }\n",
            "key: {? name: value}\n",
            2,
            0,
        ),
    ] {
        assert_relaxed_only(yaml, reference, line, col);
    }
}

#[test]
fn strict_rejects_under_indented_node_properties_and_aliases() {
    for properties in ["&a", "!custom", "&a !custom", "!custom &a"] {
        let yaml = format!("key: [\n{properties} value\n ]\n");
        let reference = format!("key: [{properties} value]\n");
        assert_relaxed_only(&yaml, &reference, 2, 0);

        let yaml = format!("key: [ {properties}\nvalue\n ]\n");
        assert_relaxed_only(&yaml, &reference, 2, 0);
    }
    assert_relaxed_only("key: [&a value,\n*a\n ]\n", "key: [&a value, *a]\n", 2, 0);
}

#[test]
fn strict_rejects_under_indented_multiline_scalar_content() {
    for (yaml, reference) in [
        ("key: [first\nsecond]\n", "key: [first second]\n"),
        ("key: ['first\nsecond']\n", "key: ['first second']\n"),
        ("key: [\"first\nsecond\"]\n", "key: [\"first second\"]\n"),
        ("key: [\"first\\\nsecond\"]\n", "key: [\"firstsecond\"]\n"),
        ("key: ['first\n']\n", "key: ['first ']\n"),
        ("key: [\"first\n\"]\n", "key: [\"first \"]\n"),
        ("key: [\"first\\\n\"]\n", "key: [\"first\"]\n"),
    ] {
        assert_relaxed_only(yaml, reference, 2, 0);
    }
}

#[test]
fn strict_rejects_tabs_used_instead_of_flow_indentation() {
    // A preceding plain scalar removes the scanner's temporary indentation level.
    // Tabs still cannot replace the spaces required by the enclosing block.
    for (yaml, kind, line) in [
        (
            "key: [first,\n\tsecond\n ]\n",
            ErrorKind::TabInBlockIndentation,
            2,
        ),
        ("key: [first,\n\t]\n", ErrorKind::TabInBlockIndentation, 2),
        (
            "key: {first: value,\n\t}\n",
            ErrorKind::TabInBlockIndentation,
            2,
        ),
        (
            "key: [first, \"second\"\n\t, third]\n",
            ErrorKind::TabInBlockIndentation,
            2,
        ),
        (
            "outer:\n  key: [first,\n  \tsecond\n   ]\n",
            ErrorKind::TabInBlockIndentation,
            3,
        ),
        (
            "key: [first, 'a\n\tb'\n ]\n",
            ErrorKind::TabInIndentation,
            2,
        ),
        (
            "key: [first, \"a\n\tb\"\n ]\n",
            ErrorKind::TabInIndentation,
            2,
        ),
        (
            "key: [first, \"a\\\n\tb\"\n ]\n",
            ErrorKind::TabInIndentation,
            2,
        ),
        ("key: [first, 'a\n\t']\n", ErrorKind::TabInIndentation, 2),
        (
            "key: [first, \"a\\\n\t\"]\n",
            ErrorKind::TabInIndentation,
            2,
        ),
    ] {
        let expected = Parser::new_from_str(yaml)
            .map(|result| {
                result
                    .expect("default mode should preserve relaxed tab handling")
                    .0
            })
            .collect::<Vec<_>>();
        for result in events_with_inputs(yaml, false) {
            assert_eq!(result.expect("relaxed mode should parse"), expected);
        }
        for result in events_with_inputs(yaml, true) {
            let error = result.expect_err("strict mode accepted tab indentation");
            assert_eq!(error.kind(), &kind, "input: {yaml:?}");
            assert_eq!(error.marker().line(), line, "input: {yaml:?}");
        }
        Scanner::new(StrInput::new(yaml))
            .collect::<Result<Vec<_>, _>>()
            .expect("default scanner should preserve relaxed tab handling");
        Scanner::with_options(
            StrInput::new(yaml),
            granit_parser::options! { strict_indentation: false },
        )
        .collect::<Result<Vec<_>, _>>()
        .expect("relaxed scanner should parse");
        let error = Scanner::with_options(
            StrInput::new(yaml),
            granit_parser::options! { strict_indentation: true },
        )
        .find_map(Result::err)
        .expect("strict scanner accepted tab indentation");
        assert_eq!(error.kind(), &kind, "scanner input: {yaml:?}");
        assert_eq!(error.marker().line(), line, "scanner input: {yaml:?}");
    }
}

#[test]
fn conforming_flow_indentation_has_identical_events_in_both_modes() {
    for yaml in [
        "[\nvalue,\n{key: value}\n]\n",
        "key: [\n 1, 2, 3,\n 4, 5, 6\n ]\n",
        "key:\n  [\n value\n ]\n",
        "key: {\n \"one\":\n 'value',\n other: [\n nested\n ]\n }\n",
        "outer:\n  key: [\n   &a !custom value,\n   *a,\n   {? nested: [one, two]}\n   ]\n  after: done\n",
        "- targets: [\n   value,\n   another\n   ]\n  after: done\n- next\n",
        "key: [first\n second, 'third\n fourth', \"fifth\\\n sixth\"]\n",
        "key: [ # start\n# comment indentation is independent\n value\n# before closing delimiter\n ]\n",
        "key: [\n\n value,\n\n ]\n",
        "key: [first,\n \tsecond,\n \t]\n",
        "key: [first, 'a\n \tb', \"c\\\n \td\"]\n",
        "[first,\n\tsecond,\n\t]\n",
        "[first, 'a\n\tb', \"c\\\n\td\"]\n",
    ] {
        let expected = Parser::new_from_str(yaml)
            .map(|result| result.expect("valid conforming YAML").0)
            .collect::<Vec<_>>();
        for strict_indentation in [false, true] {
            for result in events_with_inputs(yaml, strict_indentation) {
                assert_eq!(
                    result.unwrap_or_else(|error| panic!("strict_indentation={strict_indentation}, input {yaml:?}: {error}")),
                    expected
                );
            }
            Scanner::with_options(StrInput::new(yaml), granit_parser::options! { strict_indentation: strict_indentation })
                .collect::<Result<Vec<_>, _>>()
                .unwrap_or_else(|error| panic!("strict_indentation={strict_indentation}, scanner input {yaml:?}: {error}"));
        }
    }
}

#[test]
fn strict_option_preserves_existing_invalid_structure_checks() {
    for yaml in [
        "key: [\n value\n [nested]\n ]\n",
        "key: {\n one: value\n other: value\n }\n",
        "key: [\n value\n }\n",
        "key: {\nname\n : value\n }\n",
        "key: { &a\nname\n : value\n }\n",
    ] {
        for strict_indentation in [false, true] {
            for result in events_with_inputs(yaml, strict_indentation) {
                assert!(
                    result.is_err(),
                    "strict_indentation={strict_indentation} accepted invalid input {yaml:?}"
                );
            }
        }
    }
}
