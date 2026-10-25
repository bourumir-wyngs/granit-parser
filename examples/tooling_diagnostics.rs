//! Inspect YAML for a linter without matching human-readable error messages.
//!
//! Run with `cargo run --example tooling_diagnostics`. This also works with
//! `--no-default-features`: error categories and source markers remain available even when
//! the parser's English error messages and comment events are disabled.
//!
//! The examples below distinguish three decisions: whether the YAML grammar is valid, whether
//! aliases resolve, and whether an application allows directives in its YAML profile.

use granit_parser::{options, ErrorKind, Event, Parser, ScanError};

/// Render an application-owned diagnostic from a structured error category.
fn report_error(error: &ScanError) {
    // Match `kind()`, not `info()` or `to_string()`: wording can change or be disabled entirely.
    // A real application could use these categories to choose localized messages or help links.
    let message = match error.kind() {
        ErrorKind::UnknownAnchor => "alias has no preceding anchor definition",
        ErrorKind::DirectiveDisallowed => "this application's YAML profile prohibits directives",
        // ErrorKind is non-exhaustive. Keep a fallback for other kinds, including future additions.
        _ => "YAML parsing failed",
    };

    let marker = error.marker();
    // Parser lines are already one-based; columns count Unicode scalar values from zero.
    // Adding one is suitable for this human-readable display, but is NOT an LSP conversion.
    // See source_positions.rs for conversion to UTF-16 editor coordinates.
    println!(
        "{message} at line {}, column {} (kind: {:?})",
        marker.line(),
        marker.col() + 1,
        error.kind(),
    );
}

/// Walk the whole structure while reporting unresolved aliases as separate diagnostics.
///
/// `Ok(count)` means structural parsing completed. A nonzero count still means the input has
/// invalid references: this function's success must not be interpreted as full YAML validity.
fn inspect_for_lint(yaml: &str) -> Result<usize, ScanError> {
    // Construct Options through the macro so new options can be added compatibly.
    // Only unknown-anchor handling changes here. Syntax errors and resource limits still apply.
    let options = options! { emit_unresolved_aliases: true };
    let mut unresolved_count = 0;

    for next in Parser::new_from_str_with_options(yaml, options) {
        // Propagate genuine parse failures instead of treating a partial traversal as success.
        let (event, span) = next?;
        match event {
            Event::DocumentStart(explicit, version) => {
                // Version information already comes from DocumentStart; no directive prescan is
                // needed for this purpose. None means no version was declared for this document.
                // Reporting the version does not select a schema or convert scalar values.
                println!("document start: explicit={explicit}, declared version={version:?}");
            }
            Event::UnresolvedAlias(name) => {
                unresolved_count += 1;
                // This span covers the complete *name token. String input supplies byte offsets,
                // so slice() can recover its spelling from the SAME source used by the parser.
                println!("unresolved alias {name:?}: source={:?}", span.slice(yaml));
                // A later definition cannot repair this occurrence. If building structure, this
                // event consumes a node slot just like a resolved alias; see event_walker.rs.
            }
            Event::Scalar(value, ..) => {
                // Seeing "later" in the sample output demonstrates that traversal continued past
                // the unresolved alias. The value here is decoded scalar text, not raw source.
                println!("scalar: {value:?}");
            }
            // This example only observes selected events; it does not construct a tree or track
            // mapping roles. Ignoring other events is appropriate for this narrow observer.
            _ => {}
        }
    }

    Ok(unresolved_count)
}

fn main() -> Result<(), ScanError> {
    let yaml = "%YAML 1.2\n---\nfirst: *missing\nsecond: later\n";
    // Both parsing modes below use this same input. Print it before their diagnostics.
    println!("YAML:\n{yaml}\nStrict parsing:");

    // Ordinary parsing is strict about aliases. Consume events until the intentional failure.
    let mut strict = Parser::new_from_str(yaml);
    let error = strict
        .by_ref()
        .find_map(Result::err)
        .expect("the sample deliberately references an unknown anchor");
    assert!(matches!(error.kind(), ErrorKind::UnknownAnchor));
    report_error(&error);
    // An iterator error is terminal: calling next() again cannot resume this strict parse.
    assert!(strict.next().is_none());

    // A linter can instead report invalid references and still inspect the remaining structure.
    // The assertion documents that completing this traversal did not make *missing valid.
    println!("\nTooling mode with unresolved aliases:");
    assert_eq!(inspect_for_lint(yaml)?, 1);

    // Allowing unresolved aliases is not general error recovery. This unclosed sequence still
    // fails, even though the earlier alias was allowed through as a diagnostic-producing node.
    let malformed = "first: *missing\nbroken: [one, two\n";
    println!("\nYAML:\n{malformed}\nTooling mode with a syntax error:");
    let error = inspect_for_lint(malformed).expect_err("the sequence is deliberately unclosed");
    assert!(!matches!(error.kind(), ErrorKind::UnknownAnchor));
    report_error(&error);

    // Some applications prohibit every directive, including unused tag declarations. Looking only
    // at node tags or DocumentStart's version would miss this unused %TAG declaration.
    let restricted_yaml = "%TAG !app! tag:example.org,2026:\n---\nname: Alice\n";
    println!("\nYAML:\n{restricted_yaml}\nApplication policy rejecting directives:");
    let error =
        Parser::new_from_str_with_options(restricted_yaml, options! { disallow_directives: true })
            .find_map(Result::err)
            .expect("the sample deliberately violates the application's directive policy");
    assert!(matches!(error.kind(), ErrorKind::DirectiveDisallowed));
    report_error(&error);

    Ok(())
}
