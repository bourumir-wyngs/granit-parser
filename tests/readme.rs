use granit_parser::{Event, Parser, ScanError};

const README: &str = include_str!("../README.md");

fn minimal_example_section() -> &'static str {
    README
        .split_once("## Minimal example")
        .and_then(|(_, tail)| tail.split_once("\n## ").map(|(section, _)| section))
        .expect("README must contain a '## Minimal example' section before the next heading")
}

fn extract_yaml_input(section: &str) -> &str {
    section
        .split_once("let yaml = r#\"")
        .and_then(|(_, tail)| tail.split_once("\"#;").map(|(yaml, _)| yaml))
        .expect("README minimal example must contain a raw string YAML input")
}

fn extract_expected_output(section: &str) -> &str {
    section
        .split_once("```text\n")
        .and_then(|(_, tail)| tail.split_once("\n```").map(|(text, _)| text))
        .expect("README minimal example must contain a fenced text block with expected output")
}

fn render_readme_example(yaml: &str) -> Result<String, ScanError> {
    let mut lines = Vec::new();

    for next in Parser::new_from_str(yaml) {
        let (event, _span) = next?;

        match &event {
            Event::SequenceStart(_, Some(tag)) => {
                lines.push(format!("sequence tag: {}{}", tag.handle, tag.suffix));
            }
            Event::Scalar(value, _, _, Some(tag)) => {
                lines.push(format!(
                    "scalar tag: {}{} for {value:?}",
                    tag.handle, tag.suffix
                ));
            }
            _ => {}
        }

        lines.push(format!("{event:?}"));
    }

    Ok(lines.join("\n"))
}

#[test]
fn minimal_example_output_matches_readme() {
    let section = minimal_example_section();
    let yaml = extract_yaml_input(section);
    let expected = extract_expected_output(section);
    let actual =
        render_readme_example(yaml).expect("README example YAML should parse successfully");

    println!("Actual output:\n{}", actual);
    println!("Expected output:\n{}", expected);

    assert_eq!(actual, expected, "README example output is out of sync");
}
