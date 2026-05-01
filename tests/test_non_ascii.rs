use granit_parser::Parser;

#[test]
fn test_non_ascii_comment_start() {
    let yaml = "\
# A \u{AC00}
a1:
  b: 1
a2:
  b: 2
";
    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            panic!("Error: {}", e.info());
        }
    }
}

#[test]
fn test_non_ascii_comment_many() {
    let yaml = "\
# A \u{AC00}\
\u{AC00}: \u{AC00}
a1: # A \u{AC00}
  b: 1 # A \u{AC00}
a2: # A \u{AC00} # A \u{AC00}
  b: 2 # A \u{AC00}\
  c: [ 1, 2, 3 ] # \u{AC00}
  d: # \u{AC00}
    - 1 \u{AC00}
    - 2 \u{AC00}
    - 3 \u{AC00}
# A \u{AC00}
";

    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            panic!("Unexpected error: {}", e.info());
        }
    }
}

#[test]
fn test_non_ascii_comment() {
    let yaml = "\
a1:
  b: 1
# A \u{AC00}
a2:
  b: 2
";

    for item in Parser::new_from_str(yaml) {
        if let Err(e) = item {
            panic!("Unexpected error: {}", e.info());
        }
    }
}
