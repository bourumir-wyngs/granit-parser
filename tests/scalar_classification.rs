#[cfg(feature = "extended_details")]
use granit_parser::ScalarStyle;
use granit_parser::{Parser, ScalarKind};

#[test]
fn scalar_predicates_classify_parsed_notation_independently_of_metadata() {
    for (source, expected_kind) in [
        ("plain", ScalarKind::Plain),
        ("missing:", ScalarKind::Plain),
        ("'quoted'", ScalarKind::SingleQuoted),
        ("\"quoted\"", ScalarKind::DoubleQuoted),
        ("|2-\n  block\n", ScalarKind::Literal),
        (">+2\n  folded\n", ScalarKind::Folded),
    ] {
        let parsed = Parser::new_from_str(source)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let (_, style) = parsed
            .iter()
            .rev()
            .find_map(|(event, _)| event.scalar())
            .unwrap();
        assert_eq!(style.kind(), expected_kind, "source: {source:?}");
        #[cfg(feature = "extended_details")]
        assert_eq!(
            matches!(style, ScalarStyle::Plain { implicit: true, .. }),
            source == "missing:",
            "source: {source:?}"
        );
        for (kind, applies) in [
            (ScalarKind::Plain, style.is_plain()),
            (ScalarKind::SingleQuoted, style.is_single_quoted()),
            (ScalarKind::DoubleQuoted, style.is_double_quoted()),
            (ScalarKind::Literal, style.is_literal()),
            (ScalarKind::Folded, style.is_folded()),
        ] {
            assert_eq!(applies, kind == expected_kind, "{kind:?}: {source:?}");
        }
    }
}
