use granit_parser::{Event, Parser, ScalarStyle, ScanError, Span, StructureStyle};
use std::{fs, path::Path};

fn collect_ok_events(yaml: &str) -> Vec<(Event<'_>, Span)> {
    Parser::new_from_str(yaml)
        .map(|result| result.expect("regression input should parse"))
        .collect()
}

fn first_error(input: &str) -> ScanError {
    Parser::new_from_str(input)
        .find_map(Result::err)
        .expect("regression input should produce an error")
}

fn scalar_values(input: &str) -> Vec<String> {
    collect_ok_events(input)
        .into_iter()
        .filter_map(|(event, _)| match event {
            Event::Scalar(value, ..) => Some(value.into_owned()),
            _ => None,
        })
        .collect()
}

fn scalar_values_with_style(input: &str, style: ScalarStyle) -> Vec<String> {
    collect_ok_events(input)
        .into_iter()
        .filter_map(|(event, _)| match event {
            Event::Scalar(value, scalar_style, ..) if scalar_style == style => {
                Some(value.into_owned())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn alias_anchor_edge_cases() {
    assert_eq!(
        first_error("a: *nope\n").info(),
        "while parsing node, found unknown anchor"
    );
    assert_eq!(
        first_error("--- &x 1\n--- *x\n").info(),
        "while parsing node, found unknown anchor"
    );

    let self_reference = collect_ok_events("&x [*x]\n");
    assert!(self_reference
        .iter()
        .any(|(event, _)| matches!(event, Event::SequenceStart(StructureStyle::Flow, 1, None))));
    assert!(self_reference
        .iter()
        .any(|(event, _)| matches!(event, Event::Alias(1))));

    let redefinition = collect_ok_events("[&x 1, &x 2, *x]\n");
    assert!(redefinition.iter().any(|(event, _)| matches!(
        event,
        Event::Scalar(value, _, 1, _) if value.as_ref() == "1"
    )));
    assert!(redefinition.iter().any(|(event, _)| matches!(
        event,
        Event::Scalar(value, _, 2, _) if value.as_ref() == "2"
    )));
    assert!(redefinition
        .iter()
        .any(|(event, _)| matches!(event, Event::Alias(2))));
}

#[test]
fn crlf_and_wide_character_spans() {
    assert_eq!(scalar_values("a: 1\r\nb: 2\r\n"), ["a", "1", "b", "2"]);
    assert_eq!(scalar_values("a: 1\rb: 2\r"), ["a", "1", "b", "2"]);
    assert_eq!(
        scalar_values_with_style("k: |\r\n  line1\r\n  line2\r\n", ScalarStyle::Literal),
        ["line1\nline2\n"]
    );
    assert_eq!(
        scalar_values_with_style("k: \"a\r\n  b\"\r\n", ScalarStyle::DoubleQuoted),
        ["a b"]
    );

    let yaml = "\u{5B57}: \u{503C}\n# \u{2605}\u{6CE8}\nb: 2\n";
    let interesting: Vec<_> = collect_ok_events(yaml)
        .into_iter()
        .filter_map(|(event, span)| match event {
            Event::Scalar(value, ..) => Some((
                value.into_owned(),
                span.byte_range(),
                span.slice(yaml).map(ToOwned::to_owned),
            )),
            Event::Comment(text, _) => Some((
                format!("#{text}"),
                span.byte_range(),
                span.slice(yaml).map(ToOwned::to_owned),
            )),
            _ => None,
        })
        .collect();

    assert_eq!(
        interesting,
        vec![
            (
                "\u{5B57}".to_string(),
                Some(0..3),
                Some("\u{5B57}".to_string())
            ),
            (
                "\u{503C}".to_string(),
                Some(5..8),
                Some("\u{503C}".to_string())
            ),
            (
                "# \u{2605}\u{6CE8}".to_string(),
                Some(9..17),
                Some("# \u{2605}\u{6CE8}".to_string())
            ),
            ("b".to_string(), Some(18..19), Some("b".to_string())),
            ("2".to_string(), Some(21..22), Some("2".to_string())),
        ]
    );
}

#[test]
fn nel_and_double_bom_probes() {
    assert_eq!(scalar_values("a\u{85}b\n"), ["a\u{85}b"]);
    assert_eq!(scalar_values("\u{FEFF}\u{FEFF}a: b\n"), ["a", "b"]);
}

#[test]
fn yaml_suite_str_spans_have_valid_byte_offsets() {
    if cfg!(miri) {
        return;
    }

    let suite_dir = Path::new("tests/yaml-test-suite/src");
    if !suite_dir.is_dir() {
        return;
    }

    let mut files = fs::read_dir(suite_dir)
        .expect("yaml-test-suite directory should be readable")
        .map(|entry| {
            entry
                .expect("yaml-test-suite entry should be readable")
                .path()
        })
        .collect::<Vec<_>>();
    files.sort();

    let mut checked = 0usize;
    let mut failures = Vec::new();

    for path in files {
        let content = fs::read_to_string(&path).unwrap_or_default();
        for yaml in extract_yaml_inputs(&content) {
            for result in Parser::new_from_str(&yaml).take(100_000) {
                match result {
                    Ok((event, span)) => {
                        checked += 1;
                        check_span(&path, &yaml, &event, span, &mut failures);
                    }
                    Err(_) => break,
                }
            }
        }
    }

    assert!(
        checked > 3000,
        "span regression checked too few events: {checked}"
    );
    assert!(
        failures.is_empty(),
        "span regression found invalid spans:\n{}",
        failures.join("\n")
    );
}

fn check_span(path: &Path, yaml: &str, event: &Event<'_>, span: Span, failures: &mut Vec<String>) {
    if span.end.index() < span.start.index() {
        push_failure(
            failures,
            format!(
                "{}: {event:?} span end before start: {span:?}",
                path.display()
            ),
        );
    }

    for (which, marker) in [("start", span.start), ("end", span.end)] {
        let Some(byte) = marker.byte_offset() else {
            push_failure(
                failures,
                format!(
                    "{}: {event:?} {which} marker has no byte offset",
                    path.display()
                ),
            );
            continue;
        };

        if byte > yaml.len() || !yaml.is_char_boundary(byte) {
            push_failure(
                failures,
                format!(
                    "{}: {event:?} {which} byte offset {byte} is not a char boundary",
                    path.display()
                ),
            );
            continue;
        }

        let chars_before_byte = yaml[..byte].chars().count();
        if chars_before_byte != marker.index() {
            push_failure(
                failures,
                format!(
                    "{}: {event:?} {which} byte offset {byte} maps to char index \
                     {chars_before_byte}, marker says {}",
                    path.display(),
                    marker.index()
                ),
            );
        }
    }
}

fn push_failure(failures: &mut Vec<String>, failure: String) {
    if failures.len() < 20 {
        failures.push(failure);
    }
}

fn extract_yaml_inputs(file: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = file.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.starts_with("yaml:") && trimmed.trim_end().ends_with('|') {
            let base_indent = line.len() - trimmed.len();
            let mut block = String::new();
            i += 1;

            while i < lines.len() {
                let block_line = lines[i];
                if block_line.trim().is_empty() {
                    block.push('\n');
                    i += 1;
                    continue;
                }

                let indent = block_line.len() - block_line.trim_start().len();
                if indent <= base_indent {
                    break;
                }

                block.push_str(&block_line[(base_indent + 2).min(block_line.len())..]);
                block.push('\n');
                i += 1;
            }

            out.push(decode_yaml_suite_markers(&block));
        } else {
            i += 1;
        }
    }

    out
}

fn decode_yaml_suite_markers(block: &str) -> String {
    block
        .replace("<SPC>", " ")
        .replace("<TAB>", "\t")
        .replace("\u{2014}\u{2014}\u{2014}\u{2014}\u{BB}", "\t")
        .replace("\u{2014}\u{2014}\u{2014}\u{BB}", "\t")
        .replace("\u{2014}\u{2014}\u{BB}", "\t")
        .replace("\u{2014}\u{BB}", "\t")
        .replace('\u{BB}', "\t")
        .replace('\u{220E}', "")
        .replace('\u{21D4}', "\u{FEFF}")
        .replace('\u{21B5}', "")
}
