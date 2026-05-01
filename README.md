# granit-parser

YAML is hard. Much more than I had anticipated. If you are exploring dark corners of YAML that yaml-rust2 supports but yaml-rust doesn't, I'm curious to know what it is. 

A YAML 1.2 parser in pure Rust with strict compliance and seamless integration with [`serde-saphyr`](https://crates.io/crates/serde-saphyr).

This crate started as a fork of [`saphyr-parser`](https://crates.io/crates/saphyr-parser) that descencds from [`yaml-rust`](https://github.com/chyh1990/yaml-rust), with influences from `libyaml` and `yaml-cpp`. The project has since diverged significantly and is now maintained as an independent project.

Its primary goals are:

* full compliance with the [yaml-test-suite](https://github.com/yaml/yaml-test-suite), including correctness in edge cases
* compatibility with real-world YAML usage
* efficient parsing with reduced allocations

`granit-parser` (mostly) preserves the public API of `saphyr-parser`, typically making it a drop-in replacement.

## Minimal example

`Parser::new_from_str` returns an iterator of `(Event, Span)` pairs. If you only care about parser events, you can ignore the span and match on the emitted `Event` values:

```rust
use granit_parser::{Event, Parser};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = r#"
items: !shopping
  - milk
  - !!str bread
locations: # Example with composite keys
  [47.3769, 8.5417]: local
  [40.7128, -74.0060]: remote
music: "\uD834\uDD1E\uD83C\uDFB5\uD83C\uDFB6" # JSON-style \uXXXX surrogate pairs
"#;

    for next in Parser::new_from_str(yaml) {
        let (event, _span) = next?;

        match &event {
            Event::SequenceStart(_, Some(tag)) => {
                println!("sequence tag: {}{}", tag.handle, tag.suffix);
            }
            Event::Scalar(value, _, _, Some(tag)) => {
                println!("scalar tag: {}{} for {value:?}", tag.handle, tag.suffix);
            }
            _ => {}
        }

        println!("{event:?}");
    }

    Ok(())
}
```

This prints an event stream like:

```text
StreamStart
DocumentStart(false)
MappingStart(0, None)
Scalar("items", Plain, 0, None)
sequence tag: !shopping
SequenceStart(0, Some(Tag { handle: "!", suffix: "shopping" }))
Scalar("milk", Plain, 0, None)
scalar tag: tag:yaml.org,2002:str for "bread"
Scalar("bread", Plain, 0, Some(Tag { handle: "tag:yaml.org,2002:", suffix: "str" }))
SequenceEnd
Scalar("locations", Plain, 0, None)
MappingStart(0, None)
SequenceStart(0, None)
Scalar("47.3769", Plain, 0, None)
Scalar("8.5417", Plain, 0, None)
SequenceEnd
Scalar("local", Plain, 0, None)
SequenceStart(0, None)
Scalar("40.7128", Plain, 0, None)
Scalar("-74.0060", Plain, 0, None)
SequenceEnd
Scalar("remote", Plain, 0, None)
MappingEnd
Scalar("music", Plain, 0, None)
Scalar("𝄞🎵🎶", DoubleQuoted, 0, None)
MappingEnd
DocumentEnd
StreamEnd
```


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
