//! Scalar and comment workloads covering allocation, folding, ordering and bulk reads.
//!
//! The existing benchmarks retain their names and inputs for historical comparisons. These
//! cases cover comment buffering, quoted folding, common tags, block-scalar breaks,
//! iterator-backed block lines, and unescaped Unicode runs. Input generation stays outside
//! the timed closures. Every case consumes all events through the same receiver as the
//! existing suite, including parser construction and destruction.
//!
//! Comment cases also run without the comments feature, exercising validation and skipping.
//! Their collection and ordering optimizations are exercised with the default features.

use std::fmt::Write as _;

use divan::{black_box, Bencher};
use granit_parser::{ErrorKind, Parser};

use super::{parse_str, CountingSink};

#[divan::bench(args = [1000])]
fn mapping_values_with_inline_comments(bencher: Bencher, entries: usize) {
    let mut input = String::new();
    for i in 0..entries {
        let _ = writeln!(input, "key_{i}: # comment\n  value_{i}");
    }
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [1000])]
fn multiline_quoted_scalars(bencher: Bencher, entries: usize) {
    let mut input = String::new();
    for i in 0..entries {
        let quote = if i % 2 == 0 { '"' } else { '\'' };
        let _ = writeln!(input, "key_{i}: {quote}first line\n  second line{quote}");
    }
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [1000])]
fn core_tagged_scalars(bencher: Bencher, entries: usize) {
    let mut input = String::new();
    for i in 0..entries {
        let _ = writeln!(input, "key_{i}: !!str value_{i}");
    }
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [1000])]
fn comments_around_empty_nodes(bencher: Bencher, entries: usize) {
    let mut input = String::new();
    for i in 0..entries {
        // Empty mapping values and the first/last empty indentless sequence entries
        // exercise both ordered queue helpers as well as comment collection reuse.
        let _ = write!(
            input,
            "empty_{i}: # empty mapping value\n# between entries\n\
             value_{i}: # value comment\n  payload\n\
             sequence_{i}:\n- # first empty entry\n# before next entry\n\
             - value\n- # last empty entry\n# after sequence\n"
        );
    }
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [1000])]
fn literal_and_folded_chomping(bencher: Bencher, entries: usize) {
    let mut input = String::new();
    for i in 0..entries {
        let header = ["|", "|-", "|+", ">", ">-", ">+"][i % 6];
        let _ = writeln!(
            input,
            "key_{i}: {header}\n  first line\n\n  second line\n\n"
        );
    }
    bencher.bench(|| parse_str(black_box(&input)));
}

/// Two 4,096-character lines per scalar, substantially longer than iterator lookahead.
fn long_block_lines(entries: usize, literal: bool) -> String {
    let line = "abcdefgh".repeat(512);
    let header = if literal { '|' } else { '>' };
    let mut input = String::new();
    for i in 0..entries {
        let _ = writeln!(input, "key_{i}: {header}\n  {line}\n  {line}");
    }
    input
}

fn parse_iter(input: &str) -> usize {
    let mut sink = CountingSink::default();
    Parser::new_from_iter(input.chars())
        .load(&mut sink, true)
        .expect("benchmark input must be valid YAML");
    sink.events
}

fn parse_fallible_iter(input: &str) -> usize {
    let mut sink = CountingSink::default();
    Parser::new_from_fallible_iter(input.chars().map(Ok::<_, ErrorKind>))
        .load(&mut sink, true)
        .expect("benchmark input must be valid YAML");
    sink.events
}

#[divan::bench(args = [64])]
fn literal_scalars_from_char_iterator(bencher: Bencher, entries: usize) {
    let input = long_block_lines(entries, true);
    bencher.bench(|| parse_iter(black_box(&input)));
}

#[divan::bench(args = [64])]
fn folded_scalars_from_char_iterator(bencher: Bencher, entries: usize) {
    let input = long_block_lines(entries, false);
    bencher.bench(|| parse_iter(black_box(&input)));
}

#[divan::bench(args = [64])]
fn literal_scalars_from_fallible_iterator(bencher: Bencher, entries: usize) {
    let input = long_block_lines(entries, true);
    bencher.bench(|| parse_fallible_iter(black_box(&input)));
}

#[divan::bench(args = [64])]
fn folded_scalars_from_fallible_iterator(bencher: Bencher, entries: usize) {
    let input = long_block_lines(entries, false);
    bencher.bench(|| parse_fallible_iter(black_box(&input)));
}

/// Repeat text to give every quoted value approximately 1,024 Unicode scalar values.
fn long_quoted_values(entries: usize, text: &str) -> String {
    let contents = text.repeat(1024 / text.chars().count());
    let mut input = String::new();
    for i in 0..entries {
        let quote = if i % 2 == 0 { '"' } else { '\'' };
        let _ = writeln!(input, "key_{i}: {quote}{contents}{quote}");
    }
    input
}

#[divan::bench(args = [128])]
fn long_quoted_ascii_scalars(bencher: Bencher, entries: usize) {
    let input = long_quoted_values(entries, "abcdefgh");
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [128])]
fn long_quoted_cjk_scalars(bencher: Bencher, entries: usize) {
    // Literal source characters are necessary: a YAML \\u escape uses another scanner path.
    let input = long_quoted_values(entries, "中文日本語韓國字");
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [128])]
fn long_quoted_emoji_scalars(bencher: Bencher, entries: usize) {
    let input = long_quoted_values(entries, "😀🎉🚀🌍🦀🌈✨🎈");
    bencher.bench(|| parse_str(black_box(&input)));
}

// Short words exercise transitions between Unicode, ASCII tails, and whitespace.
// Long uninterrupted Unicode runs alone do not expose the cost of those boundaries.
#[divan::bench(args = [128])]
fn quoted_latin_words(bencher: Bencher, entries: usize) {
    let input = long_quoted_values(entries, "Zürich café naïve façade mañana ");
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [128])]
fn quoted_cyrillic_words(bencher: Bencher, entries: usize) {
    let input = long_quoted_values(entries, "Москва Санкт-Петербург Привет мир ");
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [128])]
fn quoted_mixed_script_words(bencher: Bencher, entries: usize) {
    let input = long_quoted_values(entries, "Zürich 東京code Москваdata 🦀rust ");
    bencher.bench(|| parse_str(black_box(&input)));
}

#[divan::bench(args = [128])]
fn quoted_spaced_emoji(bencher: Bencher, entries: usize) {
    let input = long_quoted_values(entries, "🦀 😀 🚀 🌍 ");
    bencher.bench(|| parse_str(black_box(&input)));
}
