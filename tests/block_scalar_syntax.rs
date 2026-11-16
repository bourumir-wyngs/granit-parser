use granit_parser::{
    BlockScalarSyntax, Chomping, ErrorKind, Event, Marker, Options, Parser, ScalarStyle, Scanner,
    Span, StrInput, TokenType,
};
use std::collections::{BTreeSet, HashSet};

fn events(source: &str) -> Vec<(Event<'_>, Span)> {
    Parser::new_from_str(source)
        .collect::<Result<Vec<_>, _>>()
        .expect("valid YAML")
}

fn block_scalar<'events>(
    events: &'events [(Event<'_>, Span)],
) -> (&'events str, ScalarStyle, BlockScalarSyntax, Span) {
    events
        .iter()
        .find_map(|(event, span)| match event {
            Event::Scalar {
                value,
                style:
                    style @ (ScalarStyle::Literal { syntax, .. }
                    | ScalarStyle::Folded { syntax, .. }),
                ..
            } => Some((value.as_ref(), *style, *syntax, *span)),
            _ => None,
        })
        .expect("a block scalar")
}

#[test]
fn headers_preserve_style_chomping_digits_and_both_indicator_orders() {
    for indicator in ['|', '>'] {
        for (suffix, chomping, digit, chomp_offset, indent_offset, tail) in [
            ("", Chomping::clip(), None, None, None, "\n"),
            ("-", Chomping::strip(), None, Some(1), None, ""),
            ("+", Chomping::keep(), None, Some(1), None, "\n\n"),
            ("2", Chomping::clip(), Some(2), None, Some(1), "\n"),
            ("2-", Chomping::strip(), Some(2), Some(2), Some(1), ""),
            ("-2", Chomping::strip(), Some(2), Some(1), Some(2), ""),
            ("2+", Chomping::keep(), Some(2), Some(2), Some(1), "\n\n"),
            ("+2", Chomping::keep(), Some(2), Some(1), Some(2), "\n\n"),
        ] {
            let header = format!("{indicator}{suffix}");
            let source =
                format!("message: {header} # explanation\n  first\n  second\n\nnext: end\n");
            let parsed = events(&source);
            let (value, style, syntax, span) = block_scalar(&parsed);
            let separator = if indicator == '|' { "\n" } else { " " };
            assert_eq!(value, format!("first{separator}second{tail}"), "{header}");
            assert_eq!(style.is_literal(), indicator == '|');
            assert_eq!(style.is_folded(), indicator == '>');
            assert_eq!(syntax.chomping, chomping);
            assert_eq!(syntax.indentation_indicator, digit);
            assert_eq!(syntax.chomping_offset, chomp_offset);
            assert_eq!(syntax.indentation_offset, indent_offset);
            assert_eq!(syntax.content_indent, 2);

            let start = source.find(&header).unwrap();
            assert_eq!(
                syntax.header_char_range(&span),
                Some(start..start + header.len())
            );
            assert_eq!(
                syntax.header_byte_range(&span),
                Some(start..start + header.len())
            );
            assert_eq!(&source[syntax.header_byte_range(&span).unwrap()], header);
            assert_eq!(syntax.header_slice(&span, &source), Some(header.as_str()));
            for (offset, character_range, byte_range) in [
                (
                    chomp_offset,
                    syntax.chomping_char_range(&span),
                    syntax.chomping_byte_range(&span),
                ),
                (
                    indent_offset,
                    syntax.indentation_char_range(&span),
                    syntax.indentation_byte_range(&span),
                ),
            ] {
                let expected = offset.map(|offset| std::ops::Range {
                    start: start + usize::from(offset),
                    end: start + usize::from(offset) + 1,
                });
                assert_eq!(character_range, expected);
                assert_eq!(byte_range, expected);
            }

            // Retain the existing body span, independently of the new header range.
            let body_start = source.find("first").unwrap();
            assert_eq!(span.start.index(), body_start);
            assert_eq!(span.start.byte_offset(), Some(body_start));
            assert_eq!(span.start.line(), 2);
            assert_eq!(span.start.col(), 2);
            assert_eq!(span.end.byte_offset(), source.find("next:"));
            assert_eq!(syntax.header_chars_before_span, body_start - start);
            assert_eq!(syntax.header_bytes_before_span, body_start - start);
        }
    }
}

#[test]
fn unicode_comments_and_crlf_use_distinct_byte_and_character_distances() {
    let source = "clé: >-2 # žąsis 🦀\r\n  日本\r\n  été\r\nnext: end\r\n";
    let parsed = events(source);
    let (value, _, syntax, span) = block_scalar(&parsed);
    assert_eq!(value, "日本 été");
    let header_byte = source.find('>').unwrap();
    let header_char = source[..header_byte].chars().count();
    let body_byte = source.find("日本").unwrap();
    let body_char = source[..body_byte].chars().count();
    assert_eq!(
        syntax.header_byte_range(&span),
        Some(header_byte..header_byte + 3)
    );
    assert_eq!(
        syntax.header_char_range(&span),
        Some(header_char..header_char + 3)
    );
    assert_eq!(syntax.header_bytes_before_span, body_byte - header_byte);
    assert_eq!(syntax.header_chars_before_span, body_char - header_char);
    assert!(syntax.header_bytes_before_span > syntax.header_chars_before_span);
    assert_eq!(span.start.line(), 2);
    assert_eq!(span.start.col(), 2);
    assert_eq!(
        syntax.chomping_byte_range(&span),
        Some(header_byte + 1..header_byte + 2)
    );
    assert_eq!(
        syntax.indentation_char_range(&span),
        Some(header_char + 2..header_char + 3)
    );

    for streamed in [
        Parser::new_from_iter(source.chars())
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        Parser::new_from_fallible_iter(source.chars().map(Ok::<_, ErrorKind>))
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
    ] {
        let (stream_value, stream_style, stream_syntax, stream_span) = block_scalar(&streamed);
        assert_eq!(stream_value, value);
        assert!(stream_style.is_folded());
        assert_eq!(stream_syntax, syntax);
        assert_eq!(stream_span, span);
        assert_eq!(
            stream_syntax.header_char_range(&stream_span),
            syntax.header_char_range(&span)
        );
        assert_eq!(stream_syntax.header_byte_range(&stream_span), None);
        assert_eq!(stream_syntax.chomping_byte_range(&stream_span), None);
        assert_eq!(stream_syntax.indentation_byte_range(&stream_span), None);
    }
}

#[test]
fn explicit_indent_is_relative_to_the_parent_not_the_header_column() {
    for header in ["|2-", "|-2", "|-"] {
        let source = format!("outer:\n  a_long_key: {header}\n    value\n  next: end\n");
        let parsed = events(&source);
        let (value, _, syntax, span) = block_scalar(&parsed);
        assert_eq!(value, "value");
        assert_eq!(syntax.content_indent, 4);
        assert_eq!(
            syntax.indentation_indicator,
            header.contains('2').then_some(2)
        );
        assert_eq!(&source[syntax.header_byte_range(&span).unwrap()], header);
        // The next mapping key remains a separate source node.
        let next = parsed
            .iter()
            .find_map(|(event, span)| match event {
                Event::Scalar { value, .. } if value == "next" => Some(span),
                _ => None,
            })
            .unwrap();
        assert!(syntax.header_byte_range(&span).unwrap().end <= next.start.byte_offset().unwrap());
        assert_eq!(next.slice(&source), Some("next"));
    }
}

#[test]
fn empty_and_blank_only_scalars_keep_header_locations_at_eof() {
    for (source, header, value) in [
        ("value: |", "|", ""),
        ("value: >-2", ">-2", ""),
        ("value: |+\n", "|+", ""),
        ("value: |+\n\n", "|+", "\n"),
        ("value: >-\n  \n", ">-", ""),
        ("value: |+ # žąsis\r\n\r\n", "|+", "\n"),
    ] {
        let parsed = events(source);
        let (actual, _, syntax, span) = block_scalar(&parsed);
        assert_eq!(actual, value, "{source:?}");
        let start = source.find(header).unwrap();
        // The existing EOF-only empty-scalar branch returns a span at the header.
        assert_eq!(span.start.byte_offset(), Some(start), "{source:?}");
        assert_eq!(syntax.header_chars_before_span, 0);
        assert_eq!(syntax.header_bytes_before_span, 0);
        assert_eq!(
            syntax.header_byte_range(&span),
            Some(start..start + header.len())
        );
        assert_eq!(&source[syntax.header_byte_range(&span).unwrap()], header);
    }
}

#[test]
fn empty_scalars_before_other_nodes_retain_the_correct_header() {
    for source in [
        "value: |\nnext: end\n",
        "value: >-2\n\nnext: end\n",
        "|2-\n---\nnext\n",
        ">+\n\n...\n",
    ] {
        let parsed = events(source);
        let (_, _, syntax, span) = block_scalar(&parsed);
        let start = source.find(['|', '>']).unwrap();
        let end = source[start..].find('\n').unwrap() + start;
        assert_eq!(
            syntax.header_byte_range(&span),
            Some(start..end),
            "{source:?}"
        );
        assert_eq!(
            syntax.header_char_range(&span),
            Some(start..end),
            "{source:?}"
        );
    }
}

#[test]
fn tag_anchor_order_and_comment_emission_do_not_change_header_metadata() {
    for properties in ["&message !note", "!note &message"] {
        let source = format!("value: {properties} >2- # žąsis 🦀\n  first\n  second\nnext: end\n");
        let mut baseline = None;
        for emit_comments in [false, true] {
            let mut options = Options::default();
            options.emit_comments = emit_comments;
            let parsed = Parser::new_from_str_with_options(&source, options)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            let (value, _, syntax, span) = block_scalar(&parsed);
            assert_eq!(value, "first second");
            assert_eq!(&source[syntax.header_byte_range(&span).unwrap()], ">2-");
            assert_eq!(span.tag_span().unwrap().slice(&source), Some("!note"));
            let metadata = (syntax, span);
            if let Some(expected) = baseline {
                assert_eq!(metadata, expected);
            } else {
                baseline = Some(metadata);
            }
        }
    }
}

#[test]
fn scanner_and_parser_expose_identical_header_ranges() {
    let source = "outer:\n  value: &a !note |+2 # žąsis\n    content\n\n  next: end\n";
    let parsed = events(source);
    let (_, parser_style, parser_syntax, parser_span) = block_scalar(&parsed);
    let tokens = Scanner::new(StrInput::new(source))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let (scanner_style, scanner_syntax, scanner_span) = tokens
        .iter()
        .find_map(|token| match token.token_type() {
            TokenType::Scalar {
                style: style @ ScalarStyle::Literal { syntax, .. },
                ..
            } => Some((*style, *syntax, token.span())),
            _ => None,
        })
        .unwrap();
    assert_eq!(scanner_style, parser_style);
    assert_eq!(scanner_syntax, parser_syntax);
    assert_eq!(
        scanner_syntax.header_byte_range(&scanner_span),
        parser_syntax.header_byte_range(&parser_span)
    );
    assert_eq!(
        scanner_syntax.header_char_range(&scanner_span),
        parser_syntax.header_char_range(&parser_span)
    );
    assert_eq!(scanner_span.start, parser_span.start);
    assert_eq!(scanner_span.end, parser_span.end);
}

#[test]
fn checked_ranges_reject_underflow_and_overflow() {
    let mut syntax = BlockScalarSyntax::default();
    let zero = Span::empty(Marker::new(0, 1, 0).with_byte_offset(Some(0)));
    syntax.header_chars_before_span = 1;
    syntax.header_bytes_before_span = 1;
    assert_eq!(syntax.header_char_range(&zero), None);
    assert_eq!(syntax.header_byte_range(&zero), None);

    let syntax = BlockScalarSyntax::default();
    let maximum = Span::empty(Marker::new(usize::MAX, 1, 0).with_byte_offset(Some(usize::MAX)));
    assert_eq!(syntax.header_char_range(&maximum), None);
    assert_eq!(syntax.header_byte_range(&maximum), None);

    for offset in [0, 3, u8::MAX] {
        let mut syntax = BlockScalarSyntax::new(Chomping::strip(), None, 0).unwrap();
        syntax.chomping_offset = Some(offset);
        assert_eq!(syntax.chomping_char_range(&zero), None);
        assert_eq!(syntax.chomping_byte_range(&zero), None);

        let mut syntax = BlockScalarSyntax::new(Chomping::clip(), Some(2), 2).unwrap();
        syntax.indentation_offset = Some(offset);
        assert_eq!(syntax.indentation_char_range(&zero), None);
        assert_eq!(syntax.indentation_byte_range(&zero), None);
    }

    let syntax = BlockScalarSyntax::default();
    let inside_utf8 = Span::empty(Marker::new(0, 1, 0).with_byte_offset(Some(1)));
    assert_eq!(syntax.header_slice(&inside_utf8, "é"), None);
    assert_eq!(syntax.header_slice(&maximum, ""), None);
}

#[test]
fn synthetic_syntax_validates_digits_and_preserves_equality_contracts() {
    for invalid in [0, 10, u8::MAX] {
        assert_eq!(
            BlockScalarSyntax::new(Chomping::clip(), Some(invalid), 0),
            None
        );
    }
    for digit in [None, Some(1), Some(9)] {
        let syntax = BlockScalarSyntax::new(Chomping::keep(), digit, 9).unwrap();
        assert_eq!(syntax.indentation_indicator, digit);
        assert_eq!(syntax.chomping_offset, Some(1));
        assert_eq!(syntax.indentation_offset, digit.map(|_| 2));
        assert_eq!(syntax.header_len(), if digit.is_some() { 3 } else { 2 });
    }

    let original = BlockScalarSyntax::new(Chomping::strip(), Some(2), 4).unwrap();
    let mut other_bytes = original;
    other_bytes.header_bytes_before_span = 17;
    assert_eq!(original, other_bytes);
    assert_eq!(original.cmp(&other_bytes), std::cmp::Ordering::Equal);
    assert_eq!(HashSet::from([original, other_bytes]).len(), 1);
    assert_eq!(BTreeSet::from([original, other_bytes]).len(), 1);

    let mut other_characters = original;
    other_characters.header_chars_before_span = 17;
    assert_ne!(original, other_characters);
    assert_ne!(original.cmp(&other_characters), std::cmp::Ordering::Equal);
    assert_eq!(HashSet::from([original, other_characters]).len(), 2);
    assert_eq!(BTreeSet::from([original, other_characters]).len(), 2);

    let first_style = ScalarStyle::literal(original);
    let second_style = ScalarStyle::literal(other_characters);
    assert_ne!(first_style, second_style);
    assert_eq!(first_style.kind(), second_style.kind());
}

#[test]
fn implicit_empty_plain_scalars_are_distinct_from_written_text() {
    for (source, expected_value, expected_implicit) in [
        ("key:\n", "~", true),
        ("key: &a\n", "~", true),
        ("key: !note\n", "", true),
        ("key: ~\n", "~", false),
        ("key: null\n", "null", false),
    ] {
        let parsed = events(source);
        let (value, implicit) = parsed
            .iter()
            .rev()
            .find_map(|(event, _)| match event {
                Event::Scalar {
                    value,
                    style: ScalarStyle::Plain { implicit, .. },
                    ..
                } => Some((value.as_ref(), *implicit)),
                _ => None,
            })
            .unwrap();
        assert_eq!(value, expected_value, "{source:?}");
        assert_eq!(implicit, expected_implicit, "{source:?}");
    }
    let parsed = events("key: \"\"\n");
    assert!(parsed.iter().any(|(event, _)| matches!(event,
        Event::Scalar { value, style: ScalarStyle::DoubleQuoted { .. }, .. } if value.is_empty()
    )));
}

#[test]
fn document_end_metadata_reports_actual_markers_only() {
    for (source, expected) in [
        ("value", vec![false]),
        ("value\n...\n", vec![true]),
        ("---\n...\n", vec![true]),
        ("one\n---\ntwo\n", vec![false, false]),
        ("one\n...\n---\ntwo\n...\n", vec![true, true]),
        ("key: \"...\" # ...\n", vec![false]),
    ] {
        let parsed = events(source);
        let ends: Vec<_> = parsed
            .iter()
            .filter_map(|(event, span)| match event {
                Event::DocumentEnd { explicit, .. } => {
                    if *explicit {
                        assert_eq!(span.slice(source), Some("..."));
                    } else {
                        assert!(span.is_empty());
                    }
                    Some(*explicit)
                }
                _ => None,
            })
            .collect();
        assert_eq!(ends, expected, "{source:?}");
    }
}

#[test]
fn header_only_eof_uses_minimum_inferred_content_indent() {
    for indicator in ['|', '>'] {
        for (prefix, minimum_indent, explicit_indent) in
            [("", 0, 2), ("value: ", 1, 2), ("outer:\n  value: ", 3, 4)]
        {
            for comment in ["", " # explanation", " # žodis 🦀"] {
                for (digit, expected_indent) in [("", minimum_indent), ("2", explicit_indent)] {
                    let source = format!("{prefix}{indicator}{digit}{comment}");
                    let parsed = events(&source);
                    let (value, _, syntax, span) = block_scalar(&parsed);
                    assert_eq!(value, "", "{source:?}");
                    assert_eq!(syntax.content_indent, expected_indent, "{source:?}");
                    assert_eq!(syntax.header_chars_before_span, 0, "{source:?}");
                    assert_eq!(syntax.header_bytes_before_span, 0, "{source:?}");
                    assert_eq!(
                        syntax.header_slice(&span, &source),
                        Some(format!("{indicator}{digit}").as_str()),
                        "{source:?}",
                    );
                    let streaming = Parser::new_from_iter(source.chars())
                        .collect::<Result<Vec<_>, _>>()
                        .expect("valid header-only YAML");
                    assert_eq!(parsed, streaming, "{source:?}");
                }
            }
        }
    }
}
