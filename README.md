# granit-parser

[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![panic-free](https://img.shields.io/badge/panic--free-%E2%9C%94%EF%B8%8F-brightgreen)](https://effective-rust.com/panic.html)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/bourumir-wyngs/granit-parser/ci.yml)](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/ci.yml)
[![docs.rs](https://docs.rs/granit-parser/badge.svg)](https://docs.rs/granit-parser)
[![codecov](https://codecov.io/gh/bourumir-wyngs/granit-parser/graph/badge.svg)](https://codecov.io/gh/bourumir-wyngs/granit-parser)
[![Socket Badge](https://badge.socket.dev/cargo/package/granit-parser/0.0.1)](https://badge.socket.dev/cargo/package/granit-parser/0.0.1)

[![crates.io](https://img.shields.io/crates/l/granit-parser.svg)](https://crates.io/crates/granit-parser)
[![crates.io](https://img.shields.io/crates/v/granit-parser.svg)](https://crates.io/crates/granit-parser)
[![0.0.3 compatible (see API note)](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/api-compat.yml/badge.svg)](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/api-compat.yml)
[![crates.io](https://img.shields.io/crates/d/granit-parser.svg)](https://crates.io/crates/granit-parser)

> “YAML is hard. Much more than I had anticipated. If you are exploring dark corners of YAML ... I'm curious to know what it is.”
>
> — [Ethiraric](https://crates.io/users/Ethiraric)

**granit-parser** is both YAML 1.1 and 1.2 compliant parser in pure Rust with strict compliance, comment and style support, no-std support, and spans for parser events. “Granit” is a correct word in many European languages (English *granite*).

This crate started as a fork of [saphyr-parser](https://crates.io/crates/saphyr-parser) that descends from [yaml-rust](https://github.com/chyh1990/yaml-rust), with influences from [libyaml](https://crates.io/crates/libyaml) and [yaml-cpp](https://github.com/jbeder/yaml-cpp). The project has since diverged significantly and is now maintained as an independent project.

Its primary goals are:

* [`Comment`](https://docs.rs/granit-parser/latest/granit_parser/struct.Comment.html) support and [`StructureStyle`](https://docs.rs/granit-parser/latest/granit_parser/enum.StructureStyle.html) information. This is for linting, formatting, and analysis.
* compliance with the [yaml-test-suite](https://github.com/yaml/yaml-test-suite), including correctness in edge cases
* compatibility with real-world YAML usage
* quickly incorporate the changes we need for the upstream dependency [serde-saphyr](https://crates.io/crates/serde-saphyr).

`granit-parser`’s 0.0.1 or 0.0.2 public API is very similar to that of `saphyr-parser`, so it is typically an easy replacement. Later versions emit style and comment information, you need to adjust your code to handle or discard them.

See [releases](https://github.com/bourumir-wyngs/granit-parser/releases)

## Minimal example

[`Parser::new_from_str`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.new_from_str) returns an iterator of ([`Event`](https://docs.rs/granit-parser/latest/granit_parser/enum.Event.html), [`Span`](https://docs.rs/granit-parser/latest/granit_parser/struct.Span.html)) pairs. The event helpers expose common node metadata, and spans provide byte ranges plus source slices:

Comments are emitted as `Event::Comment(text, placement)`. They are presentation metadata for tools such as linters and formatters, not YAML data nodes, so consumers that build YAML values should filter them out. The companion `Span` for a comment covers the whole source comment, including `#` and excluding the line break; when parsing from `Parser::new_from_str`, `span.slice(yaml)` returns that source comment text.

```rust
use granit_parser::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = r#"
items: !shopping
  - milk
  - !!str bread
locations: # Example with composite keys
  [47.3769, 8.5417]: local
  [40.7128, -74.0060]: remote

# JSON-style \uXXXX surrogate pairs:
music: "\uD834\uDD1E\uD83C\uDFB5\uD83C\uDFB6"
"#;

    for next in Parser::new_from_str(yaml) {
        let (event, span) = next?;

        if let Some(tag) = event.tag() {
            if let Some((value, _style)) = event.scalar() {
                println!(
                    "scalar tag: {tag} core-str={} for {value:?}",
                    tag.is_yaml_core_schema_tag("str")
                );
            } else if event.is_node() {
                println!("node tag: {tag} custom={}", tag.is_custom());
            }
        }

        println!(
            "{event:?} bytes={:?} source={:?}",
            span.byte_range(),
            span.slice(yaml)
        );
    }

    Ok(())
}
```

This prints an event stream like:

```text
StreamStart bytes=Some(0..0) source=Some("")
DocumentStart(false) bytes=Some(1..1) source=Some("")
MappingStart(Block, 0, None) bytes=Some(1..1) source=Some("")
Scalar("items", Plain, 0, None) bytes=Some(1..6) source=Some("items")
node tag: !shopping custom=true
SequenceStart(Block, 0, Some(Tag { handle: "!", suffix: "shopping" })) bytes=Some(20..20) source=Some("")
Scalar("milk", Plain, 0, None) bytes=Some(22..26) source=Some("milk")
scalar tag: tag:yaml.org,2002:str core-str=true for "bread"
Scalar("bread", Plain, 0, Some(Tag { handle: "tag:yaml.org,2002:", suffix: "str" })) bytes=Some(37..42) source=Some("bread")
SequenceEnd bytes=Some(43..43) source=Some("")
Scalar("locations", Plain, 0, None) bytes=Some(43..52) source=Some("locations")
Comment(" Example with composite keys", Right) bytes=Some(54..83) source=Some("# Example with composite keys")
MappingStart(Block, 0, None) bytes=Some(86..86) source=Some("")
SequenceStart(Flow, 0, None) bytes=Some(86..87) source=Some("[")
Scalar("47.3769", Plain, 0, None) bytes=Some(87..94) source=Some("47.3769")
Scalar("8.5417", Plain, 0, None) bytes=Some(96..102) source=Some("8.5417")
SequenceEnd bytes=Some(102..103) source=Some("]")
Scalar("local", Plain, 0, None) bytes=Some(105..110) source=Some("local")
SequenceStart(Flow, 0, None) bytes=Some(113..114) source=Some("[")
Scalar("40.7128", Plain, 0, None) bytes=Some(114..121) source=Some("40.7128")
Scalar("-74.0060", Plain, 0, None) bytes=Some(123..131) source=Some("-74.0060")
SequenceEnd bytes=Some(131..132) source=Some("]")
Scalar("remote", Plain, 0, None) bytes=Some(134..140) source=Some("remote")
Comment(" JSON-style \\uXXXX surrogate pairs:", Above) bytes=Some(142..178) source=Some("# JSON-style \\uXXXX surrogate pairs:")
MappingEnd bytes=Some(179..179) source=Some("")
Scalar("music", Plain, 0, None) bytes=Some(179..184) source=Some("music")
Scalar("𝄞🎵🎶", DoubleQuoted, 0, None) bytes=Some(186..224) source=Some("\"\\uD834\\uDD1E\\uD83C\\uDFB5\\uD83C\\uDFB6\"")
MappingEnd bytes=Some(225..225) source=Some("")
DocumentEnd bytes=Some(225..225) source=Some("")
StreamEnd bytes=Some(225..225) source=Some("")
```

## Event API choices

Use [`try_load`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.try_load)
when a receiver may return a validation or application error and parsing should
stop immediately. It accepts
[`TryEventReceiver`](https://docs.rs/granit-parser/latest/granit_parser/trait.TryEventReceiver.html)
or
[`TrySpannedEventReceiver`](https://docs.rs/granit-parser/latest/granit_parser/trait.TrySpannedEventReceiver.html)
and returns
[`TryLoadError`](https://docs.rs/granit-parser/latest/granit_parser/enum.TryLoadError.html) to
distinguish parser errors from receiver errors.

Event-only receivers receive comment events as `Event::Comment(text, placement)`.
Spanned receivers receive the same event plus the comment span in
[`on_event`](https://docs.rs/granit-parser/latest/granit_parser/trait.SpannedEventReceiver.html#tymethod.on_event).
When using [`resolve`](https://docs.rs/granit-parser/latest/granit_parser/parser_stack/struct.ParserStack.html#method.resolve)
or [`push_include`](https://docs.rs/granit-parser/latest/granit_parser/parser_stack/struct.ParserStack.html#method.push_include)
on `ParserStack`, comment events
from included documents are forwarded through the normal event stream. Their
spans remain local to the included source, matching the existing span behavior
for other included-document events.

Use the iterator API when the caller should pull events and decide when to stop
parsing. [`load`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.load)
is `infallible`.

## Key differences from saphyr-parser

All changes are intentionally scoped around correctness, compliance, and interoperability.

### YAML compliance fixes

* **Invalid extra closing brackets are rejected**

  ```yaml
  [ a, b, c ] ]
  ```

* **Comments no longer truncate multiline scalars**

  ```yaml
  word1  # comment
  word2
  ```

  This is now correctly treated as invalid YAML instead of silently discarding content.

* **Reserved directives are ignored**

  Previously reported as errors; now handled according to the YAML specification.


### Compatibility adjustment

* **Relaxed indentation for closing brackets**

  ```yaml
  key: [ 1, 2, 3,
         4, 5, 6
  ]
  ```

  While not strictly YAML-compliant, this form is accepted for compatibility with other parsers and real-world inputs.


### JSON-style Unicode surrogate pairs
This parser supports explicit handling for JSON-style Unicode surrogate pairs in quoted scalar escape sequences.
* `\uXXXX` escapes that encode a high surrogate are now required to be followed immediately by a valid low surrogate escape, and both escapes are combined into the corresponding Unicode scalar value.
* Unpaired high surrogates, unpaired low surrogates, and reversed surrogate pairs are now rejected during scanning instead of being treated as generic invalid Unicode escape codes.

### Parsing correctness improvements

* **Plain scalar continuation fixed**

 Supports cases like:

  ```yaml
  hello:
    world: this is a string
      --- still a string
  ```

* **More helpful error reporting**
 
  Mismatched brackets and quotes now report the position of the opening token instead of the end of file.


### Performance improvements

* **Zero-copy parsing for `&str` input**

  Uses `Cow<'input, str>` to avoid unnecessary allocations when parsing from in-memory strings.


### Internal extensions

* **Parser stack support**

  Enables features such as `!include` by exposing additional internal capabilities.


### Security

This crate includes fixes to improve resilience against:

* denial-of-service inputs
* parser hangs
* panic conditions

Like the upstream parser, it does **not** interpret application-level types, so parsing YAML does not trigger external side effects.

### Improved ergonomics
Release 0.0.3 includes ergonomic helpers such as `Event::tag`, `Event::scalar`,
`Event::anchor_id`, `Event::alias_id`, `Event::is_node`, `Tag::parts`,
`Tag::is_custom`, `Tag::is_yaml_core_schema_tag`, `Span::slice`, and
`ParserStack::push_include`. See CHANGELOG.md for details.

## Tools

The repository includes a few developer tools for inspecting parser output and measuring performance.

Root package binaries:

* `dump_events` prints the parser event stream for a YAML file.
  ```sh
  cargo run --bin dump_events -- input.yaml
  ```
* `time_parser` measures one full parse and discards the events.
  ```sh
  cargo run --release --bin time_parser -- input.yaml
  ```
* `run_parser` runs repeated parses and reports aggregate timings.
  ```sh
  cargo run --release --bin run_parser -- input.yaml 10
  ```

Standalone helper crates:

* `walk` opens a small REPL for navigating parsed YAML spans.
  ```sh
  cargo run --manifest-path tools/walk/Cargo.toml -- input.yaml
  ```
* `bench_compare` compares benchmark output from multiple parsers.
  ```sh
  cargo bench_compare -- run_bench
  ```
* `gen_large_yaml` generates large YAML inputs for benchmark work.
  ```sh
  cargo gen_large_yaml -- --help
  ```

See `tools/README.md` and `tools/bench_compare/README.md` for the longer tool notes.


## License

Licensed under either:

* Apache License, Version 2.0
* MIT license

At your option.

This project inherits licensing terms from its upstream origins. See the `LICENSE` file and `.licenses/` directory for details.
