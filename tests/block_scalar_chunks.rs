use granit_parser::{ErrorKind, Event, InputIoError, Marker, Parser, ScalarStyle, ScanError, Span};

/// Compare scalar values and character-based spans with both streaming backends.
/// String input additionally exposes byte offsets, checked separately below.
fn parse_all_inputs(source: &str) -> Vec<(Event<'_>, Span)> {
    let from_str = Parser::new_from_str(source)
        .collect::<Result<Vec<_>, _>>()
        .expect("string input should parse");
    let from_iter = Parser::new_from_iter(source.chars())
        .collect::<Result<Vec<_>, _>>()
        .expect("iterator input should parse");
    let from_fallible = Parser::new_from_fallible_iter(source.chars().map(Ok::<_, ErrorKind>))
        .collect::<Result<Vec<_>, _>>()
        .expect("fallible input should parse");
    assert_eq!(from_str, from_iter);
    assert_eq!(from_str, from_fallible);
    from_str
}

fn block_scalar<'events>(
    events: &'events [(Event<'_>, Span)],
) -> (&'events str, ScalarStyle, Span) {
    events
        .iter()
        .find_map(|(event, span)| match event {
            Event::Scalar(value, style @ (ScalarStyle::Literal | ScalarStyle::Folded), ..) => {
                Some((value.as_ref(), *style, *span))
            }
            _ => None,
        })
        .expect("expected a block scalar")
}

fn terminal_error<'input>(
    mut parser: impl Iterator<Item = Result<(Event<'input>, Span), ScanError>>,
) -> ScanError {
    let error = parser
        .by_ref()
        .find_map(Result::err)
        .expect("expected an error");
    assert!(
        parser.next().is_none(),
        "an error must terminate the parser"
    );
    assert!(parser.next().is_none(), "the parser must stay fused");
    error
}

fn error_all_inputs(source: &str) -> ScanError {
    let from_str = terminal_error(Parser::new_from_str(source));
    let from_iter = terminal_error(Parser::new_from_iter(source.chars()));
    let from_fallible = terminal_error(Parser::new_from_fallible_iter(
        source.chars().map(Ok::<_, ErrorKind>),
    ));
    for other in [&from_iter, &from_fallible] {
        assert_eq!(from_str.kind(), other.kind());
        assert_eq!(from_str.marker(), other.marker());
    }
    from_str
}

#[test]
fn long_lines_preserve_folding_chomping_indentation_and_line_breaks() {
    let run = "abcdefgh01234567".repeat(256);
    for (indicator, style, separator) in [
        ('|', ScalarStyle::Literal, "\n"),
        ('>', ScalarStyle::Folded, " "),
    ] {
        for newline in ["\n", "\r", "\r\n"] {
            for indent in ["", "2"] {
                for (chomp, tail) in [("-", ""), ("", "\n"), ("+", "\n\n")] {
                    let source = format!(
                        "{indicator}{indent}{chomp}{newline}  {run}{newline}  {run}{newline}{newline}---{newline}tail{newline}"
                    );
                    let events = parse_all_inputs(&source);
                    let (value, actual_style, _) = block_scalar(&events);
                    assert_eq!(value, format!("{run}{separator}{run}{tail}"));
                    assert_eq!(actual_style, style);
                    assert!(events.iter().any(|(event, _)| matches!(
                        event,
                        Event::Scalar(value, ScalarStyle::Plain, ..) if value == "tail"
                    )));
                }
            }
        }
    }
}

#[test]
fn unicode_lines_keep_character_markers_and_byte_offsets_distinct() {
    let contents = format!(
        "{}é\t字\u{85}\u{a0}\u{2028}\u{2029}🦀{}",
        "a".repeat(4096),
        "z".repeat(4096)
    );
    for indicator in ['|', '>'] {
        let prefix = format!("clé: {indicator}-\r\n  ");
        let source = format!("{prefix}{contents}\r\nfin: done\r\n");
        let end_byte = prefix.len() + contents.len() + 2;
        let events = parse_all_inputs(&source);
        let (value, _, span) = block_scalar(&events);
        assert_eq!(value, contents);
        assert_eq!(span.start, Marker::new(prefix.chars().count(), 2, 2));
        assert_eq!(
            span.end,
            Marker::new(source[..end_byte].chars().count(), 3, 0)
        );
        assert_eq!(span.byte_range(), Some(prefix.len()..end_byte));
        assert_eq!(span.slice(&source), Some(&source[prefix.len()..end_byte]));
        let (_, next_span) = events
            .iter()
            .find(|(event, _)| matches!(event, Event::Scalar(value, ..) if value == "fin"))
            .expect("mapping continues after the block scalar");
        assert_eq!(next_span.start, span.end);
        assert_eq!(next_span.start.byte_offset(), Some(end_byte));
    }
}

#[test]
fn more_indented_lines_blank_lines_and_tabs_preserve_folding_boundaries() {
    let run = "long line ".repeat(256);
    for (indicator, separator) in [('|', "\n\n"), ('>', "\n")] {
        let source =
            format!("{indicator}-\n  {run}\n    indented\t{run}\n\n  {run}\n\n  last\tline\n");
        let events = parse_all_inputs(&source);
        let (value, _, _) = block_scalar(&events);
        assert_eq!(
            value,
            format!("{run}\n  indented\t{run}\n\n{run}{separator}last\tline")
        );
    }
}

#[test]
fn unterminated_final_lines_preserve_short_boundaries_and_chomping() {
    for length in [1, 7, 8, 15, 16, 31, 32, 127, 128, 4096] {
        let contents = format!("{}é🦀", "x".repeat(length));
        for indicator in ['|', '>'] {
            for (chomp, tail) in [("-", ""), ("", "\n"), ("+", "\n")] {
                let prefix = format!("{indicator}{chomp}\n  ");
                let source = format!("{prefix}{contents}");
                let events = parse_all_inputs(&source);
                let (value, _, span) = block_scalar(&events);
                assert_eq!(value, format!("{contents}{tail}"));
                assert_eq!(
                    span.end,
                    Marker::new(source.chars().count(), 2, 2 + contents.chars().count())
                );
                assert_eq!(span.byte_range(), Some(prefix.len()..source.len()));
            }
        }
    }
}

#[test]
fn root_document_markers_end_content_but_marker_prefixes_remain_content() {
    let run = "x".repeat(4096);
    for indicator in ['|', '>'] {
        for marker in ["---", "..."] {
            let separator = if indicator == '|' { "\n" } else { " " };
            let source = format!("{indicator}-\n{run}\n{marker}suffix\n{marker}\n");
            let events = parse_all_inputs(&source);
            let (value, _, _) = block_scalar(&events);
            assert_eq!(value, format!("{run}{separator}{marker}suffix"));
        }
    }
}

#[test]
fn forbidden_controls_after_long_lines_keep_error_kinds_and_markers() {
    let run = "x".repeat(4096);
    for indicator in ['|', '>'] {
        for character in [
            '\0', '\u{1}', '\u{8}', '\u{7f}', '\u{80}', '\u{9f}', '\u{fffe}', '\u{ffff}',
        ] {
            let prefix = format!("{indicator}-\n  é{run}");
            let source = format!("{prefix}{character}after\n");
            let error = error_all_inputs(&source);
            assert_eq!(error.kind(), &ErrorKind::UnexpectedCharacter { character });
            // NUL stops the line reader and is rejected at its source position. Other
            // controls retain the existing validation marker at the first content character.
            if character == '\0' {
                assert_eq!(
                    *error.marker(),
                    Marker::new(prefix.chars().count(), 2, run.len() + 3)
                );
                assert_eq!(error.marker().byte_offset(), Some(prefix.len()));
            } else {
                assert_eq!(*error.marker(), Marker::new(5, 2, 2));
                assert_eq!(error.marker().byte_offset(), Some(5));
            }
        }
    }
}

#[test]
fn source_failures_during_long_block_lines_remain_terminal() {
    for indicator in ['|', '>'] {
        let source = format!("{indicator}\n  é{}", "x".repeat(4096));
        let expected = ErrorKind::InputIo {
            error: InputIoError::from_message("block scalar read failed"),
        };
        let input = source
            .chars()
            .map(Ok)
            .chain(std::iter::once(Err(expected.clone())))
            .chain(std::iter::from_fn(|| {
                panic!("source must not be polled after its first error")
            }));
        let error = terminal_error(Parser::new_from_fallible_iter(input));
        assert_eq!(error.kind(), &expected);
    }
}
