use granit_parser::{options, ErrorKind, Event, Options, Parser, ScanError};

/// Parse `yaml` with every parser input backend and return the scalar values or the first error.
fn scalars(yaml: &str, options: Options) -> [(&'static str, Result<Vec<String>, ScanError>); 3] {
    fn collect<'a>(
        events: impl Iterator<Item = Result<(Event<'a>, granit_parser::Span), ScanError>>,
    ) -> Result<Vec<String>, ScanError> {
        let mut values = Vec::new();
        for event in events {
            if let (Event::Scalar { value, .. }, _) = event? {
                values.push(value.into_owned());
            }
        }
        Ok(values)
    }

    [
        (
            "string parser",
            collect(Parser::new_from_str_with_options(yaml, options.clone())),
        ),
        (
            "iterator parser",
            collect(Parser::new_from_iter_with_options(
                yaml.chars(),
                options.clone(),
            )),
        ),
        (
            "fallible parser",
            collect(Parser::new_from_fallible_iter_with_options(
                yaml.chars().map(Ok),
                options,
            )),
        ),
    ]
}

const IN_DOCUMENT: [(&str, &[&str]); 6] = [
    ("---\n\u{feff}a: b\n", &["\u{feff}a", "b"]),
    ("a: 1\n\u{feff}b: 2\n", &["a", "1", "\u{feff}b", "2"]),
    ("key: \u{feff}value\n", &["key", "\u{feff}value"]),
    ("key: \u{feff}\n", &["key", "\u{feff}"]),
    ("- \u{feff}\n- x\n", &["\u{feff}", "x"]),
    ("[\u{feff}x, y]\n", &["\u{feff}x", "y"]),
];

#[test]
fn bom_inside_document_is_rejected_by_default() {
    assert!(!Options::default().bom_inside_document_as_content);
    for (yaml, _) in IN_DOCUMENT {
        for (backend, result) in scalars(yaml, Options::default()) {
            let error = result.expect_err(&format!("{backend} accepted {yaml:?}"));
            assert_eq!(
                error.kind(),
                &ErrorKind::BomInsideDocument,
                "{backend}: {yaml:?}"
            );
        }
    }
}

#[test]
fn bom_inside_document_can_be_read_as_content() {
    let options = options! { bom_inside_document_as_content: true };
    for (yaml, expected) in IN_DOCUMENT {
        for (backend, result) in scalars(yaml, options.clone()) {
            let values = result.unwrap_or_else(|error| panic!("{backend}: {yaml:?}: {error}"));
            assert_eq!(values, expected, "{backend}: {yaml:?}");
        }
    }
}

#[test]
fn bom_in_document_prefix_is_skipped_in_both_modes() {
    for options in [
        Options::default(),
        options! { bom_inside_document_as_content: true },
    ] {
        for (yaml, expected) in [
            ("\u{feff}a: b\n", &["a", "b"][..]),
            ("\u{feff}\u{feff}a: b\n", &["a", "b"]),
            ("---\nfirst\n...\n\u{feff}second\n", &["first", "second"]),
            // A BOM after other scalar characters is already content.
            ("key: va\u{feff}lue\n", &["key", "va\u{feff}lue"]),
            ("key: '\u{feff}value'\n", &["key", "\u{feff}value"]),
        ] {
            for (backend, result) in scalars(yaml, options.clone()) {
                let values = result.unwrap_or_else(|error| panic!("{backend}: {yaml:?}: {error}"));
                assert_eq!(values, expected, "{backend}: {yaml:?}");
            }
        }
    }
}
