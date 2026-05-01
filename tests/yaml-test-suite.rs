use std::{
    borrow::Cow,
    fs::{self, DirEntry},
    process::ExitCode,
};

use libtest_mimic::{run, Arguments, Failed, Trial};

use granit_parser::{
    Event, Marker, Parser, ScalarStyle, ScanError, Span, SpannedEventReceiver, Tag,
};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

struct YamlTest {
    yaml_visual: String,
    yaml: String,
    expected_events: String,
    expected_error: bool,
}

#[derive(Default)]
struct RawYamlTest {
    yaml_visual: Option<String>,
    expected_events: Option<String>,
    expected_error: Option<bool>,
    skip: Option<bool>,
}

fn main() -> Result<ExitCode> {
    if cfg!(miri) {
        eprintln!("========================================================");
        eprintln!("/!\\ yaml-test-suite is skipped under Miri isolation /!\\");
        eprintln!("========================================================");
        return Ok(ExitCode::SUCCESS);
    }

    if !std::path::Path::new("tests/yaml-test-suite").is_dir() {
        eprintln!("===================================================================");
        eprintln!("/!\\ yaml-test-suite directory not found, Skipping tests /!\\");
        eprintln!("If you intend to contribute to the library, restore the test suite.");
        eprintln!("===================================================================");
        return Ok(ExitCode::SUCCESS);
    }

    let mut arguments = Arguments::from_args();
    if arguments.test_threads.is_none() {
        arguments.test_threads = Some(1);
    }
    let tests: Vec<Vec<_>> = std::fs::read_dir("tests/yaml-test-suite/src")?
        .map(|entry| -> Result<_> {
            let entry = entry?;
            let tests = load_tests_from_file(&entry)?;
            Ok(tests)
        })
        .collect::<Result<_>>()?;
    let mut tests: Vec<_> = tests.into_iter().flatten().collect();
    tests.sort_by(|a, b| a.name().cmp(b.name()));

    Ok(run(&arguments, tests).exit_code())
}

#[allow(clippy::needless_pass_by_value)]
fn run_yaml_test(data: YamlTest) -> Result<(), Failed> {
    let reporter = parse_to_events(&data.yaml);
    let actual_events = reporter.as_ref().map(|reporter| &reporter.events);
    let events_diff = actual_events.map(|events| events_differ(events, &data.expected_events));
    let error_text = match (&events_diff, data.expected_error) {
        (Ok(x), true) => Some(format!("no error when expected: {x:#?}")),
        (Err(_), true) | (Ok(None), false) => None,
        (Err(e), false) => Some(format!("unexpected error {e:?}")),
        (Ok(Some(diff)), false) => Some(format!("events differ: {diff}")),
    };

    if let Some(mut txt) = error_text {
        add_error_context(&mut txt, &data, events_diff.err().map(ScanError::marker));
        Err(txt.into())
    } else if let Some((mut msg, span)) = reporter
        .as_ref()
        .ok()
        .and_then(|reporter| reporter.span_failures.first().cloned())
    {
        add_error_context(&mut msg, &data, Some(&span.start));
        Err(msg.into())
    } else {
        Ok(())
    }
}

// Enrich the error message with the failing input, and a caret pointing
// at the position that errored.
fn add_error_context(text: &mut String, desc: &YamlTest, marker: Option<&Marker>) {
    use std::fmt::Write;
    let _ = writeln!(text, "\n### Input:\n{}\n### End", desc.yaml_visual);
    if let Some(mark) = marker {
        writeln!(text, "### Error position").unwrap();
        let mut lines = desc.yaml.lines();
        for _ in 0..(mark.line() - 1) {
            let l = lines.next().unwrap();
            writeln!(text, "{l}").unwrap();
        }
        writeln!(text, "\x1B[91;1m{}", lines.next().unwrap()).unwrap();
        for _ in 0..mark.col() {
            write!(text, " ").unwrap();
        }
        writeln!(text, "^\x1b[m").unwrap();
        for l in lines {
            writeln!(text, "{l}").unwrap();
        }
        writeln!(text, "### End error position").unwrap();
    }
}

fn load_tests_from_file(entry: &DirEntry) -> Result<Vec<Trial>> {
    let file_name = entry.file_name().to_string_lossy().to_string();
    let test_name = file_name
        .strip_suffix(".yaml")
        .ok_or("unexpected filename")?;
    let tests = parse_yaml_suite_file(&fs::read_to_string(entry.path())?)
        .map_err(|e| format!("While reading {file_name}: {e}"))?;
    let test_count = tests.len();

    let mut result = vec![];
    let mut current_test = RawYamlTest::default();
    for (idx, test_data) in tests.into_iter().enumerate() {
        let name = if test_count > 1 {
            format!("{test_name}-{idx:02}")
        } else {
            test_name.to_string()
        };

        // Test fields except `fail` are "inherited"
        current_test.expected_error = None;
        if test_data.yaml_visual.is_some() {
            current_test.yaml_visual = test_data.yaml_visual;
        }
        if test_data.expected_events.is_some() {
            current_test.expected_events = test_data.expected_events;
        }
        if test_data.expected_error.is_some() {
            current_test.expected_error = test_data.expected_error;
        }
        if test_data.skip.is_some() {
            current_test.skip = test_data.skip;
        }

        if current_test.skip == Some(true) {
            continue;
        }

        let yaml_visual = current_test
            .yaml_visual
            .clone()
            .ok_or_else(|| format!("{name}: missing yaml field"))?;
        let expected_events = current_test
            .expected_events
            .clone()
            .ok_or_else(|| format!("{name}: missing tree field"))?;
        let expected_error = current_test.expected_error == Some(true);

        result.push(Trial::test(name, move || {
            run_yaml_test(YamlTest {
                yaml: visual_to_raw(&yaml_visual),
                expected_events: visual_to_raw(&expected_events),
                yaml_visual,
                expected_error,
            })
        }));
    }
    Ok(result)
}

fn parse_yaml_suite_file(source: &str) -> Result<Vec<RawYamlTest>> {
    let lines: Vec<_> = source.lines().collect();
    let mut tests = Vec::new();
    let mut current = None;
    let mut idx = 0;

    while idx < lines.len() {
        let line = lines[idx];
        if line == "---" || line.trim().is_empty() {
            idx += 1;
            continue;
        }

        if let Some(field) = line.strip_prefix("- ") {
            if let Some(test) = current.take() {
                tests.push(test);
            }
            current = Some(RawYamlTest::default());
            let test = current.as_mut().unwrap();
            idx = parse_suite_field(field, &lines, idx + 1, test)?;
        } else if let Some(field) = line.strip_prefix("  ") {
            let test = current
                .as_mut()
                .ok_or_else(|| format!("field before first test at line {}", idx + 1))?;
            if field.starts_with(' ') {
                idx += 1;
                continue;
            }
            idx = parse_suite_field(field, &lines, idx + 1, test)?;
        } else {
            return Err(format!("unexpected line {}: {line:?}", idx + 1).into());
        }
    }

    if let Some(test) = current {
        tests.push(test);
    }

    Ok(tests)
}

fn parse_suite_field(
    field: &str,
    lines: &[&str],
    next_idx: usize,
    test: &mut RawYamlTest,
) -> Result<usize> {
    let (key, value) = field
        .split_once(':')
        .ok_or_else(|| format!("malformed test field: {field:?}"))?;
    let value = value.trim_start();

    if value.starts_with('|') {
        let (value, next_idx) = parse_literal_block(lines, next_idx, value)?;
        match key {
            "yaml" => test.yaml_visual = Some(value),
            "tree" => test.expected_events = Some(value),
            _ => {}
        }
        Ok(next_idx)
    } else {
        match key {
            "fail" => test.expected_error = Some(value == "true"),
            "skip" => test.skip = Some(true),
            _ => {}
        }
        Ok(next_idx)
    }
}

fn parse_literal_block(
    lines: &[&str],
    start_idx: usize,
    indicator: &str,
) -> Result<(String, usize)> {
    let mut idx = start_idx;
    let mut block_lines = Vec::new();

    while let Some(line) = lines.get(idx) {
        if line.trim().is_empty() {
            block_lines.push(*line);
            idx += 1;
            continue;
        }

        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent <= 2 {
            break;
        }

        block_lines.push(*line);
        idx += 1;
    }

    let explicit_indent = indicator
        .strip_prefix('|')
        .and_then(|suffix| suffix.chars().find_map(|ch| ch.to_digit(10)))
        .map(|indent| indent as usize + 2);
    let detected_indent = block_lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min();
    let indent = explicit_indent.or(detected_indent).unwrap_or(0);

    let mut block = String::new();
    for line in block_lines {
        if line.trim().is_empty() {
            block.push('\n');
        } else if let Some(content) = line.get(indent..) {
            block.push_str(content);
            block.push('\n');
        } else {
            return Err(
                format!("literal block line is indented less than {indent}: {line:?}").into(),
            );
        }
    }

    if !block.is_empty() {
        while block.ends_with('\n') {
            block.pop();
        }
        block.push('\n');
    }

    Ok((block, idx))
}

fn parse_to_events(source: &str) -> Result<EventReporter<'_>, ScanError> {
    let mut str_events = vec![];
    let mut str_error = None;
    let mut iter_events = vec![];
    let mut iter_error = None;

    // Parse as string
    for x in Parser::new_from_str(source) {
        match x {
            Ok(event) => str_events.push(event),
            Err(e) => {
                str_error = Some(e);
                break;
            }
        }
    }
    // Parse as iter
    for x in Parser::new_from_iter(source.chars()) {
        match x {
            Ok(event) => iter_events.push(event),
            Err(e) => {
                iter_error = Some(e);
                break;
            }
        }
    }

    // No matter the input, we should parse into the same events.
    assert_eq!(str_events, iter_events);
    // Or the same error.
    assert_eq!(str_error, iter_error);
    // If we had an error, return it so the test fails.
    if let Some(err) = str_error {
        return Err(err);
    }

    // Put events into the reporter, for comparison with the test suite.
    let mut reporter = EventReporter::default();
    for x in str_events {
        reporter.on_event(x.0, x.1);
    }
    Ok(reporter)
}

#[derive(Default)]
/// A [`SpannedEventReceiver`] checking for inconsistencies in event [`Spans`].
pub struct EventReporter<'input> {
    pub events: Vec<String>,
    last_span: Option<(Event<'input>, Span)>,
    pub span_failures: Vec<(String, Span)>,
}

impl<'input> SpannedEventReceiver<'input> for EventReporter<'input> {
    fn on_event(&mut self, ev: Event<'input>, span: Span) {
        if let Some((last_ev, last_span)) = self.last_span.take() {
            if span.start.index() < last_span.start.index()
                || span.end.index() < last_span.end.index()
            {
                self.span_failures.push((
                    format!("event {ev:?}@{span:?} came before event {last_ev:?}@{last_span:?}"),
                    span,
                ));
            }
        }
        self.last_span = Some((ev.clone(), span));

        let line: String = match ev {
            Event::StreamStart => "+STR".into(),
            Event::StreamEnd => "-STR".into(),

            Event::DocumentStart(_) => "+DOC".into(),
            Event::DocumentEnd => "-DOC".into(),

            Event::SequenceStart(idx, tag) => {
                format!("+SEQ{}{}", format_index(idx), format_tag(tag.as_ref()))
            }
            Event::SequenceEnd => "-SEQ".into(),

            Event::MappingStart(idx, tag) => {
                format!("+MAP{}{}", format_index(idx), format_tag(tag.as_ref()))
            }
            Event::MappingEnd => "-MAP".into(),

            Event::Scalar(ref text, style, idx, ref tag) => {
                let kind = match style {
                    ScalarStyle::Plain => ":",
                    ScalarStyle::SingleQuoted => "'",
                    ScalarStyle::DoubleQuoted => r#"""#,
                    ScalarStyle::Literal => "|",
                    ScalarStyle::Folded => ">",
                };
                format!(
                    "=VAL{}{} {kind}{}",
                    format_index(idx),
                    format_tag(tag.as_ref()),
                    escape_text(text)
                )
            }
            Event::Alias(idx) => format!("=ALI *{idx}"),
            Event::Nothing => return,
        };
        self.events.push(line);
    }
}

fn format_index(idx: usize) -> String {
    if idx > 0 {
        format!(" &{idx}")
    } else {
        String::new()
    }
}

fn escape_text(text: &str) -> String {
    let mut text = text.to_owned();
    for (ch, replacement) in [
        ('\\', r"\\"),
        ('\n', "\\n"),
        ('\r', "\\r"),
        ('\x08', "\\b"),
        ('\t', "\\t"),
    ] {
        text = text.replace(ch, replacement);
    }
    text
}

fn format_tag(tag: Option<&Cow<'_, Tag>>) -> String {
    if let Some(tag) = tag {
        format!(" <{}{}>", tag.handle, tag.suffix)
    } else {
        String::new()
    }
}

fn events_differ(actual: &[String], expected: &str) -> Option<String> {
    let actual = actual.iter().map(Some).chain(std::iter::repeat(None));
    let expected = expected_events(expected);
    let expected = expected.iter().map(Some).chain(std::iter::repeat(None));
    for (idx, (act, exp)) in actual.zip(expected).enumerate() {
        return match (act, exp) {
            (Some(act), Some(exp)) => {
                if act == exp {
                    continue;
                } else {
                    Some(format!(
                        "line {idx} differs: \n=> expected `{exp}`\n=>    found `{act}`",
                    ))
                }
            }
            (Some(a), None) => Some(format!("extra actual line: {a:?}")),
            (None, Some(e)) => Some(format!("extra expected line: {e:?}")),
            (None, None) => None,
        };
    }
    unreachable!()
}

/// Convert the snippets from "visual" to "actual" representation
fn visual_to_raw(yaml: &str) -> String {
    let mut yaml = yaml.to_owned();
    for (pat, replacement) in [
        ("\u{2423}", " "),
        ("\u{BB}", "\t"),
        ("\u{2014}", ""), // Tab line continuation \u{2014}\u{2014}\u{BB}
        ("\u{2190}", "\r"),
        ("\u{21D4}", "\u{FEFF}"),
        ("\u{21B5}", ""), // Trailing newline marker
        ("\u{220E}\n", ""),
    ] {
        yaml = yaml.replace(pat, replacement);
    }
    yaml
}

/// Adapt the expectations to the yaml-rust reasonable limitations
///
/// Drop information on node styles (flow/block) and anchor names.
/// Both are things that can be omitted according to spec.
fn expected_events(expected_tree: &str) -> Vec<String> {
    let mut anchors = vec![];
    expected_tree
        .split('\n')
        .map(|s| s.trim_start().to_owned())
        .filter(|s| !s.is_empty())
        .map(|mut s| {
            // Anchor name-to-number conversion
            if let Some(start) = s.find('&') {
                if s[..start].find(':').is_none() {
                    let len = s[start..].find(' ').unwrap_or(s[start..].len());
                    anchors.push(s[start + 1..start + len].to_owned());
                    s = s.replace(&s[start..start + len], &format!("&{}", anchors.len()));
                }
            }
            // Alias nodes name-to-number
            if s.starts_with("=ALI") {
                let start = s.find('*').unwrap();
                let name = &s[start + 1..];
                let idx = anchors
                    .iter()
                    .enumerate()
                    .rfind(|(_, v)| v == &name)
                    .unwrap()
                    .0;
                s = s.replace(&s[start..], &format!("*{}", idx + 1));
            }
            // Dropping style information
            match &*s {
                "+DOC ---" => "+DOC".into(),
                "-DOC ..." => "-DOC".into(),
                s if s.starts_with("+SEQ []") => s.replacen("+SEQ []", "+SEQ", 1),
                s if s.starts_with("+MAP {}") => s.replacen("+MAP {}", "+MAP", 1),
                "=VAL :" => "=VAL :~".into(), // FIXME: known bug
                s => s.into(),
            }
        })
        .collect()
}
