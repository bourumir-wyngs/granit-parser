use granit_parser::{options, Event, Parser};

fn scalars(input: &str, options: granit_parser::Options) -> Result<Vec<String>, String> {
    let mut parser = Parser::new_from_str_with_options(input, options);
    let mut out = Vec::new();
    for next in &mut parser {
        match next {
            Ok((Event::Scalar { value, .. }, _)) => out.push(value.to_string()),
            Ok(_) => {}
            Err(e) => return Err(format!("{e}")),
        }
    }
    Ok(out)
}

#[test]
fn a_top_level_block_scalar_may_start_at_column_zero_by_default() {
    assert_eq!(scalars(">\nfoo\n", options! {}).expect("reads"), ["foo\n"]);
}

#[test]
fn with_the_option_column_zero_content_ends_the_block_scalar() {
    let o = options! { block_scalar_indent_at_least_one: true };
    assert!(
        scalars(">\n@", o.clone()).is_err(),
        "`@` is then a reserved indicator"
    );
    assert_eq!(
        scalars(">\n foo\n", o).expect("indented content still reads"),
        ["foo\n"]
    );
}
