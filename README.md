# granit-parser

[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![panic-free](https://img.shields.io/badge/panic--free-%E2%9C%94%EF%B8%8F-brightgreen)](https://effective-rust.com/panic.html)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/bourumir-wyngs/granit-parser/ci.yml)](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/ci.yml)
[![Fuzz](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/fuzz.yml/badge.svg)](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/fuzz.yml)
[![docs.rs](https://docs.rs/granit-parser/badge.svg)](https://docs.rs/granit-parser)

[![codecov](https://codecov.io/gh/bourumir-wyngs/granit-parser/graph/badge.svg)](https://codecov.io/gh/bourumir-wyngs/granit-parser)
[![Socket Badge](https://badge.socket.dev/cargo/package/granit-parser)](https://socket.dev/cargo/package/granit-parser)
[![dependency status](https://deps.rs/crate/granit-parser/latest/status.svg)](https://deps.rs/crate/granit-parser/latest)

[![API compatibility](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/api-compat.yml/badge.svg)](https://github.com/bourumir-wyngs/granit-parser/actions/workflows/api-compat.yml)
[![crates.io](https://img.shields.io/crates/l/granit-parser.svg)](https://crates.io/crates/granit-parser)
[![crates.io](https://img.shields.io/crates/v/granit-parser.svg)](https://crates.io/crates/granit-parser)
[![crates.io](https://img.shields.io/crates/d/granit-parser.svg)](https://crates.io/crates/granit-parser)
[![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/bourumir-wyngs/granit-parser?utm_source=badge)

> “YAML is hard. Much more than I had anticipated. If you are exploring dark corners of YAML ... I'm curious to know what it is.”
>
> — [Ethiraric](https://crates.io/users/Ethiraric)

**granit-parser** is a YAML 1.2 parser in pure Rust with comment and style support, no-std support, and spans for parser events. It accepts many real-world YAML documents from the YAML 1.1 era where the syntax overlaps with YAML 1.2. “Granit” is a correct word in many European languages (English *granite*).

This crate started as a fork of [saphyr-parser](https://crates.io/crates/saphyr-parser) that descends from [yaml-rust](https://github.com/chyh1990/yaml-rust), with influences from [libyaml](https://crates.io/crates/libyaml) and [yaml-cpp](https://github.com/jbeder/yaml-cpp). The project has since diverged significantly and is now maintained as an independent project.

Its primary goals are:

* [`Comment`](https://docs.rs/granit-parser/latest/granit_parser/struct.Comment.html) support and [`StructureStyle`](https://docs.rs/granit-parser/latest/granit_parser/enum.StructureStyle.html) information. This is for linting, formatting, and analysis.
* compliance with the [yaml-test-suite](https://github.com/yaml/yaml-test-suite), including correctness in edge cases
* compatibility with real-world YAML usage
* improved support for formatting, linting, syntax highlight and other comparable projects that need additional metadata  
* quickly incorporate the changes we need for the upstream dependency [serde-saphyr](https://crates.io/crates/serde-saphyr).

`granit-parser` 3.0 exposes block-scalar header syntax and uses named, non-exhaustive
fields on variants carrying data. See [Migrating to 3.0](#migrating-to-30) below and
[`CHANGELOG.md`](CHANGELOG.md) for earlier migrations.

See [releases](https://github.com/bourumir-wyngs/granit-parser/releases)

## Minimal example

[`Parser::new_from_str`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.new_from_str) returns an iterator of ([`Event`](https://docs.rs/granit-parser/latest/granit_parser/enum.Event.html), [`Span`](https://docs.rs/granit-parser/latest/granit_parser/struct.Span.html)) pairs. The event helpers expose common node metadata, and spans provide byte ranges, source slices, and explicit tag-token starts for tagged nodes:

Comments are emitted as [`Event::Comment { value, placement, .. }`](https://docs.rs/granit-parser/latest/granit_parser/enum.Event.html#variant.Comment). They are presentation metadata for tools such as linters and formatters, not YAML data nodes, so consumers that build YAML values should filter them out. The companion [`Span`](https://docs.rs/granit-parser/latest/granit_parser/struct.Span.html) for a comment covers the whole source comment, including `#` and excluding the line break; when parsing from [`Parser::new_from_str`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.new_from_str), [`span.slice(yaml)`](https://docs.rs/granit-parser/latest/granit_parser/struct.Span.html#method.slice) returns that source comment text.

Applications that do not use comments can disable their capture and emission by setting
[`Options::emit_comments`](https://docs.rs/granit-parser/latest/granit_parser/struct.Options.html#structfield.emit_comments)
to `false` or disabling the default `comments` Cargo feature. The field is always available and
defaults to `true` with the feature enabled and `false` without it. Setting it to `true` has no
effect without the feature. Comments are still recognized and validated as YAML syntax.

Document starts are emitted as [`Event::DocumentStart { explicit, version, .. }`](https://docs.rs/granit-parser/latest/granit_parser/enum.Event.html#variant.DocumentStart), where `version` is the optional [`YamlVersion`](https://docs.rs/granit-parser/latest/granit_parser/struct.YamlVersion.html) declared by a preceding `%YAML` directive for that document.

The example also prints literal/folded header syntax: raw indicators, character and byte
ranges, chomping, explicit indentation, and effective content indentation. The Unicode header
comment makes character and byte distances differ. Header ranges exclude comments and line
breaks; the companion scalar span retains its existing meaning. An omitted value and a written
`~` demonstrate the `implicit` flag, while two documents show explicit and implicit ends.

For editors processing multiple documents, `Parser::proceed_next_document()` resumes after a
syntax error at the next separated `---` or valid `...` at column zero. Call it after recording the
error, then continue reading events normally with original source positions. Its
`Result<bool, ScanError>` is `Ok(true)` when another document remains (including an empty or malformed
document), and `Ok(false)` at clean EOF. Pending comments and an unread `StreamEnd` remain available
through normal event reads. Looking ahead past comments respects `Options::max_buffered_comment_events`
and can return `ErrorKind::TooManyComments` when comment emission is enabled.
It also skips the current document during successful parsing.
Consumers must abandon or mark their unfinished tree nodes; recovery emits no synthetic closing
events or error nodes. Anchors remain scoped to their document, and source failures remain terminal.
This recovers later documents, not structure within the broken document.

```rust
use granit_parser::{options, Event, Parser, ScalarStyle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = r#"
%YAML 1.2
%TAG !example! tag:example.com,2000:
%TAG !core! tag:yaml.org,2002:i
---
items: !shopping
  - milk
  - !example!sliced bread
  - !!str bread
  - !core!nt 1
locations: # Example with composite keys
  [47.3769, 8.5417]: local
  [40.7128, -74.0060]: remote

# JSON-style \uXXXX surrogate pairs:
music: "\uD834\uDD1E\uD83C\uDFB5\uD83C\uDFB6"

favorite: &favorite bread
order: *favorite
unavailable: *missing
dessert: cake
literal: |
  first line
  second line
folded: >2- # žąsis
  hello
  world
nested:
  kept: |+2
    last line

omitted:
written_null: ~
...
---
last: value
"#;

    let options = options! {
        // Must be enabled in Options to receive anchor names and token spans (default: false).
        emit_anchor_events: true,
        // Must be enabled in Options to report undefined anchors and continue (default: false).
        emit_unresolved_aliases: true,
    };

    for next in Parser::new_from_str_with_options(yaml, options) {
        let (event, span) = next?;

        match &event {
            Event::Anchor { anchor_id: id, name, .. } => {
                // Metadata for the preceding node, with a span covering the &name token.
                println!("anchor: id={id} name={name:?}");
            }
            Event::Alias { anchor_id: id, .. } => println!("alias: target id={id}"),
            Event::UnresolvedAlias { name, .. } => {
                // Still an invalid reference, with a span covering the *name token.
                println!("undefined anchor: {name:?}");
            }
            Event::Scalar { value, style, .. } => {
                // The same decoded text can represent omitted or explicitly written input.
                if let ScalarStyle::Plain { implicit, .. } = style {
                    if value == "~" {
                        println!("plain scalar: value={value:?} implicit={implicit}");
                    }
                }
                if let Some(syntax) = style.block_syntax() {
                    // Pass the original companion span to resolve the compact offsets.
                    println!(
                        "block header: style={:?} text={:?} chars={:?} bytes={:?}",
                        style.kind(),
                        syntax.header_slice(&span, yaml),
                        syntax.header_char_range(&span),
                        syntax.header_byte_range(&span)
                    );
                    println!(
                        "  chomping={:?} indentation_indicator={:?} content_indent={}",
                        syntax.chomping, syntax.indentation_indicator, syntax.content_indent
                    );
                    println!(
                        "  chomping indicator: chars={:?} bytes={:?}",
                        syntax.chomping_char_range(&span), syntax.chomping_byte_range(&span)
                    );
                    println!(
                        "  indentation indicator: chars={:?} bytes={:?}",
                        syntax.indentation_char_range(&span), syntax.indentation_byte_range(&span)
                    );
                }
            }
            Event::DocumentEnd { explicit, .. } => println!("document end: explicit={explicit}"),
            _ => {}
        }

        if let Some(tag) = event.tag() {
            let tag_start = span
                .tag_start()
                .map(|mark| (mark.line(), mark.col(), mark.byte_offset()));

            if let Some((value, _style)) = event.scalar() {
                println!(
                    "scalar tag: {tag} core-suffix={:?} core-str={} tag_start(line,col,byte)={tag_start:?} for {value:?}",
                    tag.core_suffix(),
                    tag.is_yaml_core_schema_tag("str")
                );
            } else if event.is_node() {
                println!(
                    "node tag: {tag} custom={} tag_start(line,col,byte)={tag_start:?}",
                    tag.is_custom()
                );
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
DocumentStart { explicit: true, version: Some(YamlVersion { major: 1, minor: 2 }) } bytes=Some(80..83) source=Some("---")
MappingStart { style: Block, anchor_id: 0, tag: None } bytes=Some(84..84) source=Some("")
Scalar { value: "items", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(84..89) source=Some("items")
node tag: !shopping custom=true tag_start(line,col,byte)=Some((6, 7, Some(91)))
SequenceStart { style: Block, anchor_id: 0, tag: Some(Tag { handle: "!", suffix: "shopping", original_handle: "!" }) } bytes=Some(103..103) source=Some("")
Scalar { value: "milk", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(105..109) source=Some("milk")
scalar tag: tag:example.com,2000:sliced core-suffix=None core-str=false tag_start(line,col,byte)=Some((8, 4, Some(114))) for "bread"
Scalar { value: "bread", style: Plain { implicit: false }, anchor_id: 0, tag: Some(Tag { handle: "tag:example.com,2000:", suffix: "sliced", original_handle: "!example!" }) } bytes=Some(130..135) source=Some("bread")
scalar tag: tag:yaml.org,2002:str core-suffix=Some("str") core-str=true tag_start(line,col,byte)=Some((9, 4, Some(140))) for "bread"
Scalar { value: "bread", style: Plain { implicit: false }, anchor_id: 0, tag: Some(Tag { handle: "tag:yaml.org,2002:", suffix: "str", original_handle: "!!" }) } bytes=Some(146..151) source=Some("bread")
scalar tag: tag:yaml.org,2002:int core-suffix=Some("int") core-str=false tag_start(line,col,byte)=Some((10, 4, Some(156))) for "1"
Scalar { value: "1", style: Plain { implicit: false }, anchor_id: 0, tag: Some(Tag { handle: "tag:yaml.org,2002:i", suffix: "nt", original_handle: "!core!" }) } bytes=Some(165..166) source=Some("1")
SequenceEnd bytes=Some(167..167) source=Some("")
Scalar { value: "locations", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(167..176) source=Some("locations")
Comment { value: " Example with composite keys", placement: Right } bytes=Some(178..207) source=Some("# Example with composite keys")
MappingStart { style: Block, anchor_id: 0, tag: None } bytes=Some(210..210) source=Some("")
SequenceStart { style: Flow, anchor_id: 0, tag: None } bytes=Some(210..211) source=Some("[")
Scalar { value: "47.3769", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(211..218) source=Some("47.3769")
Scalar { value: "8.5417", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(220..226) source=Some("8.5417")
SequenceEnd bytes=Some(226..227) source=Some("]")
Scalar { value: "local", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(229..234) source=Some("local")
SequenceStart { style: Flow, anchor_id: 0, tag: None } bytes=Some(237..238) source=Some("[")
Scalar { value: "40.7128", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(238..245) source=Some("40.7128")
Scalar { value: "-74.0060", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(247..255) source=Some("-74.0060")
SequenceEnd bytes=Some(255..256) source=Some("]")
Scalar { value: "remote", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(258..264) source=Some("remote")
Comment { value: " JSON-style \\uXXXX surrogate pairs:", placement: Above } bytes=Some(266..302) source=Some("# JSON-style \\uXXXX surrogate pairs:")
MappingEnd bytes=Some(303..303) source=Some("")
Scalar { value: "music", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(303..308) source=Some("music")
Scalar { value: "𝄞🎵🎶", style: DoubleQuoted, anchor_id: 0, tag: None } bytes=Some(310..348) source=Some("\"\\uD834\\uDD1E\\uD83C\\uDFB5\\uD83C\\uDFB6\"")
Scalar { value: "favorite", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(350..358) source=Some("favorite")
Scalar { value: "bread", style: Plain { implicit: false }, anchor_id: 1, tag: None } bytes=Some(370..375) source=Some("bread")
anchor: id=1 name="favorite"
Anchor { anchor_id: 1, name: "favorite" } bytes=Some(360..369) source=Some("&favorite")
Scalar { value: "order", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(376..381) source=Some("order")
alias: target id=1
Alias { anchor_id: 1 } bytes=Some(383..392) source=Some("*favorite")
Scalar { value: "unavailable", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(393..404) source=Some("unavailable")
undefined anchor: "missing"
UnresolvedAlias { name: "missing" } bytes=Some(406..414) source=Some("*missing")
Scalar { value: "dessert", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(415..422) source=Some("dessert")
Scalar { value: "cake", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(424..428) source=Some("cake")
Scalar { value: "literal", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(429..436) source=Some("literal")
block header: style=Literal text=Some("|") chars=Some(438..439) bytes=Some(438..439)
  chomping=Clip indentation_indicator=None content_indent=2
  chomping indicator: chars=None bytes=None
  indentation indicator: chars=None bytes=None
Scalar { value: "first line\nsecond line\n", style: Literal { syntax: BlockScalarSyntax { header_chars_before_span: 4, header_bytes_before_span: 4, chomping: Clip, indentation_indicator: None, digit_first: false, content_indent: 2 } }, anchor_id: 0, tag: None } bytes=Some(442..467) source=Some("first line\n  second line\n")
Scalar { value: "folded", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(467..473) source=Some("folded")
block header: style=Folded text=Some(">2-") chars=Some(475..478) bytes=Some(475..478)
  chomping=Strip indentation_indicator=Some(2) content_indent=2
  chomping indicator: chars=Some(477..478) bytes=Some(477..478)
  indentation indicator: chars=Some(476..477) bytes=Some(476..477)
Scalar { value: "hello world", style: Folded { syntax: BlockScalarSyntax { header_chars_before_span: 14, header_bytes_before_span: 16, chomping: Strip, indentation_indicator: Some(2), digit_first: true, content_indent: 2 } }, anchor_id: 0, tag: None } bytes=Some(491..505) source=Some("hello\n  world\n")
Comment { value: " žąsis", placement: Right } bytes=Some(479..488) source=Some("# žąsis")
Scalar { value: "nested", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(505..511) source=Some("nested")
MappingStart { style: Block, anchor_id: 0, tag: None } bytes=Some(515..515) source=Some("")
Scalar { value: "kept", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(515..519) source=Some("kept")
block header: style=Literal text=Some("|+2") chars=Some(519..522) bytes=Some(521..524)
  chomping=Keep indentation_indicator=Some(2) content_indent=4
  chomping indicator: chars=Some(520..521) bytes=Some(522..523)
  indentation indicator: chars=Some(521..522) bytes=Some(523..524)
Scalar { value: "last line\n\n", style: Literal { syntax: BlockScalarSyntax { header_chars_before_span: 8, header_bytes_before_span: 8, chomping: Keep, indentation_indicator: Some(2), digit_first: false, content_indent: 4 } }, anchor_id: 0, tag: None } bytes=Some(529..540) source=Some("last line\n\n")
MappingEnd bytes=Some(540..540) source=Some("")
Scalar { value: "omitted", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(540..547) source=Some("omitted")
plain scalar: value="~" implicit=true
Scalar { value: "~", style: Plain { implicit: true }, anchor_id: 0, tag: None } bytes=Some(547..547) source=Some("")
Scalar { value: "written_null", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(549..561) source=Some("written_null")
plain scalar: value="~" implicit=false
Scalar { value: "~", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(563..564) source=Some("~")
MappingEnd bytes=Some(565..565) source=Some("")
document end: explicit=true
DocumentEnd { explicit: true } bytes=Some(565..568) source=Some("...")
DocumentStart { explicit: true, version: None } bytes=Some(569..572) source=Some("---")
MappingStart { style: Block, anchor_id: 0, tag: None } bytes=Some(573..573) source=Some("")
Scalar { value: "last", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(573..577) source=Some("last")
Scalar { value: "value", style: Plain { implicit: false }, anchor_id: 0, tag: None } bytes=Some(579..584) source=Some("value")
MappingEnd bytes=Some(585..585) source=Some("")
document end: explicit=false
DocumentEnd { explicit: false } bytes=Some(585..585) source=Some("")
StreamEnd bytes=Some(585..585) source=Some("")
```

## Migrating to 3.0

Inspect `Event`, `TokenType`, and `ScalarStyle` with `matches!`, `if let`, or `match`.
Every variant is non-exhaustive: write `{ .. }` even when it currently has no fields.
The `..` ignores fields you do not need, so new metadata can be added to a variant without
breaking your code. A complete `match` also needs a fallback arm because new variants may
be added. For example, `Event::Scalar(value, style, _, _)` becomes
`Event::Scalar { value, style, .. }`.

Pattern matching checks the variant without constructing a value to compare against.
Whole-value equality would also compare metadata and could change meaning when fields are
added. Normal builds therefore provide no equality, hashing, or ordering for these output
enums, `Token`, or `BlockScalarSyntax`. Inspect only the fields relevant to your application;
`Event::scalar()` remains a read accessor.

Constructors serve a different purpose: creating synthetic values for custom `ParserTrait`
implementations, event transformations, ownership conversions, and `ReplayParser` streams.
Use `Event::new_scalar(value, style, anchor_id, tag)`, `ScalarStyle::plain()` or
`ScalarStyle::implicit_plain()`, and the corresponding constructors for other variants.
Unit variants also have constructors, such as `Event::stream_end()`; every variant remains
non-exhaustive. Construction is supported independently of whether comparison is enabled.

The opt-in `test-equality` feature enables `PartialEq` and `Eq` for real output types in
regression tests. Hashing and ordering remain unavailable. Equality includes all current
fields, with the established exception that optional byte coordinates are ignored across
input backends; block-header order is irrelevant when either indicator is absent. Derived
enum equality includes future fields automatically, and the block-header comparator uses
exhaustive destructuring so a new field requires an explicit comparison policy.
Keep this feature in development dependencies: Cargo features are additive, so enabling it
makes equality available throughout that build. Continue using patterns for classification.

```rust
use granit_parser::{Event, ScalarStyle};

fn inspect(event: &Event<'_>) {
    if matches!(event, Event::StreamEnd { .. }) {
        println!("end of stream");
    }
    if let Event::Scalar { value, style, .. } = event {
        if value == "~" && matches!(style, ScalarStyle::Plain { .. }) {
            println!("plain tilde or omitted value");
        }
        if let ScalarStyle::Plain { implicit, .. } = style {
            println!("omitted: {implicit}");
        }
        if let ScalarStyle::Literal { syntax, .. } = style {
            println!("literal chomping: {:?}", syntax.chomping);
        }
    }
}
```

Use `matches!(style, ScalarStyle::Plain { .. })` in place of
`style == ScalarStyle::Plain`. This matches both written and omitted plain scalars.
Optional predicates such as `style.is_plain()` provide the same category check.
The unit-only enums `ScalarKind`, `Chomping`, `StructureStyle`, and `Placement` retain
their existing comparison traits; they also support matching. For example,
`matches!(chomping, Chomping::Keep)` is equivalent to `chomping == Chomping::Keep`.
Use `style.kind()` when an equality-comparable or hashable notation category is needed.

`ScalarStyle::Plain { implicit, .. }` distinguishes an omitted YAML node (`key:`) from
written content (`key: ~`). An explicit tag or anchor does not make omitted content written.
Quoted and block styles describe written scalars even when their decoded value is empty.
Decoded values retain their existing behavior. Document-end events expose `explicit`,
which is true only for a written `...` marker.

Literal and folded styles carry `BlockScalarSyntax`: chomping, an optional explicit
indentation digit, their order (`digit_first`), and effective content indentation.
The `chomping_offset()` and `indentation_offset()` methods compute indicator positions
from these values; the order flag has no effect when either indicator is absent.
Header locations are stored as backward distances from the accompanying `Span::start`.
Keep the original span with the style; changing its start invalidates those distances.
The existing span still has its original meaning, and a header range excludes the trailing
whitespace, comment, and line break.

```rust
use granit_parser::{Parser, ScalarStyle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "message: >2- # explanation\n  hello\n  world\n";
    for result in Parser::new_from_str(source) {
        let (event, span) = result?;
        if let Some((_, style)) = event.scalar() {
            match style {
                ScalarStyle::Literal { syntax, .. }
                | ScalarStyle::Folded { syntax, .. } => {
                    let range = syntax.header_byte_range(&span)
                        .expect("string input provides byte offsets");
                    assert_eq!(&source[range], ">2-");
                    assert_eq!(syntax.indentation_indicator, Some(2));
                    assert_eq!(syntax.content_indent, 2);
                }
                _ => {}
            }
        }
    }
    Ok(())
}
```

`header_char_range` also works for character-iterator inputs; `header_byte_range` returns
`None` when the companion span lacks byte offsets. These ranges locate the header without
reconstructing its line and column. They are not a complete scalar replacement range.
Use `style.kind()` or predicates such as `style.is_literal()` when comparing notation
categories without comparing source metadata.

## Learning examples

These examples include explanatory comments and assertions for their sample inputs. Run them from
the repository root, then read the source alongside the output:

| Example | What it teaches | Run |
| --- | --- | --- |
| [Event walker](examples/event_walker.rs) | Track mapping keys/values, sequence elements, and depth through nested collections, aliases, comments, and document boundaries. Read collection style directly from events. | `cargo run --example event_walker` |
| [Source positions](examples/source_positions.rs) | Distinguish character indexes, UTF-8 byte offsets, and UTF-16 editor positions; handle Unicode and line endings using the original source. | `cargo run --example source_positions` |
| [String spans](examples/string_spans.rs) | Convert `Span::byte_range()` into a required consumer range with explicit errors; preserve raw Unicode and escaped text, distinguish empty ranges from missing offsets. | `cargo run --example string_spans` |
| [Tooling diagnostics](examples/tooling_diagnostics.rs) | Match structured errors, inspect declared YAML versions, continue past unresolved aliases explicitly, and enforce a no-directives policy. | `cargo run --example tooling_diagnostics` |

All four also run with `--no-default-features`, for example:

```sh
cargo run --no-default-features --example tooling_diagnostics
```

The example executables use Rust's standard library; disabling the parser's default features
demonstrates that their logic does not depend on comment events or built-in error messages.

Spans emitted by `Parser::new_from_str` and `Parser::new_from_str_with_options` always have
UTF-8 byte ranges into the original input, including empty ranges for implicit events and EOF.
Use `Span::byte_range()` when storing offsets and `Span::slice(source)` when only raw text is
needed. Keep the same, unchanged source: decoded scalar text and character indexes are not
byte offsets. The shared span API returns `Option` because iterator inputs and manually
constructed spans may lack offsets; missing offsets should not silently become zero.

## Key differences from saphyr-parser

All changes are intentionally scoped around correctness, compliance, and interoperability. This list summarizes 
differences done from the time of forking (0.0.6). Saphyr-parser may also have changes in its later releases.

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
While linters would insist on strict compliance, other applications may prefer full backward compatibility. 
To balance requirements for all use cases, the compliance is governed by settings in [`Options`](https://docs.rs/granit-parser/latest/granit_parser/struct.Options.html).

* **Relaxed indentation for flow collection entries and delimiters**

  ```yaml
  key: [
  1, 2, 3,
  4, 5, 6
  ]
  ```

  While not strictly YAML-compliant, under-indented entries and delimiters in flow sequences
  and mappings are accepted for compatibility with PyYAML, ruamel.yaml, and real-world inputs.
  The same applies to quoted scalar continuation lines, including in block context.
  Under-indented implicit mapping keys must keep the key and its `:` on the same line.

  Set [`strict_indentation: true`](https://docs.rs/granit-parser/latest/granit_parser/struct.Options.html#structfield.strict_indentation) to require YAML-compliant indentation, including flow entries,
  delimiters, and multiline scalar continuations. The default is `false`.

  ```rust
  use granit_parser::{options, Parser};

  let parser = Parser::new_from_str_with_options(
      "key: [\n  1, 2, 3\n  ]\n",
      options! { strict_indentation: true },
  );
  assert!(parser.collect::<Result<Vec<_>, _>>().is_ok());
  ```


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


### Performance

* **Zero-copy parsing for `&str` input**

  Uses `Cow<'input, str>` to avoid unnecessary allocations when parsing from in-memory strings.

* Comment handling
  If your application does not use comment parsing, set `Options::emit_comments = false` and disable the
  `comments` Cargo feature. The option suppresses capture and emission even if another dependency enables
  the feature through Cargo feature unification. Disabling the feature compiles out comment handling
  state when no dependency enables it. Comments can still be present in YAML, they are not emitted.

### Internal extensions

* **Parser stack support**

  Enables features such as `!include` by exposing additional internal capabilities.

### API migration notes

`Span` is non-exhaustive. Its public fields remain readable and writable, but external code
must use constructors instead of struct literals or struct-update syntax. Use the metadata
builders or assign individual fields, and include `..` when destructuring:

```rust
use granit_parser::{Marker, Span};

let start = Marker::new(0, 1, 0);
let end = Marker::new(3, 1, 3);
let span = Span::new(start, end).with_indent(Some(0));
let Span { start: actual_start, .. } = span;
assert_eq!(actual_start, start);
```

`Span::with_tag_start` is deprecated and remains available for compatibility. For exact tag
locations, use `Span::tag_span()` on parser-emitted spans; a start marker alone cannot provide the
complete token range.

For custom `Input` implementations, `SkipTabs` now contains only the input policies `Yes` and
`No`. Whitespace results use `granit_parser::input::WhitespaceResult` instead:

- `skip_ws_to_eol` returns `(usize, Result<WhitespaceResult, ErrorKind>)`.
- `skip_ws_to_eol_blanks` returns `(usize, WhitespaceResult)`.

Replace `SkipTabs::Result(found_tabs, has_valid_yaml_ws)` with
`WhitespaceResult::new(found_tabs, has_valid_yaml_ws)`. The `found_tabs()` and
`has_valid_yaml_ws()` accessors remain available on the result. The tuple's character count
still reports consumed characters, including when `skip_ws_to_eol` returns an error.
These are breaking API changes intended for the next major release.

### Error handling without grepping message text
  
Granit-parser uses the [`ErrorKind`](https://docs.rs/granit-parser/latest/granit_parser/enum.ErrorKind.html) enum to encode the error kind, and this enum may carry additional metadata. 
Scanning the English message text for specific substrings is fragile and better to be avoided. See example [tooling_diagnostics.rs](https://github.com/bourumir-wyngs/granit-parser/blob/master/examples/tooling_diagnostics.rs).

### Fallible streaming input

[`Parser::new_from_iter`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.new_from_iter) remains available for genuinely infallible sources. Do not use it for an adapter that turns I/O, decoding, or size-limit failures into EOF: an incomplete YAML prefix may itself be a valid document.

Use [`Parser::new_from_fallible_iter`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.new_from_fallible_iter) for a streaming source that can fail. It accepts an [`Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html) whose items are [`Result<char, ErrorKind>`](https://doc.rust-lang.org/std/result/enum.Result.html). The iterator protocol has three distinct outcomes:

* `Some(Ok(character))` supplies another character.
* `None` reports clean physical end-of-input.
* `Some(Err(error_kind))` reports a terminal source failure.

The parser returns a [`ScanError`](https://docs.rs/granit-parser/latest/granit_parser/struct.ScanError.html) carrying that same [`ErrorKind`](https://docs.rs/granit-parser/latest/granit_parser/enum.ErrorKind.html) and never polls the source again.
See the [fallible reader example](examples/fallible_reader.rs).

The lower-level [`Scanner`](https://docs.rs/granit-parser/latest/granit_parser/struct.Scanner.html)
is also fallible. Its iterator item is `Result<Token, ScanError>`, so collecting all tokens keeps
scanner failures distinct from clean EOF:

```rust
use granit_parser::{Scanner, StrInput};

let tokens = Scanner::new(StrInput::new("key: value\n"))
    .collect::<Result<Vec<_>, _>>()
    .expect("valid YAML should scan");

assert!(!tokens.is_empty());
```

### Security

This crate includes fixes to improve resilience against:

* denial-of-service inputs
* parser hangs
* panic conditions

Like the upstream parser, it does **not** interpret application-level types, so parsing YAML does not trigger external side effects.

### Improved ergonomics
The following ergonomic helpers are available:
- `Event::tag`
- `Event::scalar`
- `Event::anchor_id`
- `Event::alias_id`
- `Event::is_node`
- `Tag::parts`
- `Tag::handle`
- `Tag::suffix`
- `Tag::original_handle`
- `Tag::original_parts`
- `Tag::original`
- `Tag::core_suffix`
- `Tag::suffix_in_namespace`
- `Tag::is_custom`
- `Tag::is_yaml_core_schema_tag`
- `Span::slice`
- `Span::tag_span` for the complete raw tag-token range, including URI escapes
- `Span::tag_start` (you more likely need `tag_span`)
- `ParserStack::push_include`

See CHANGELOG.md for details.

### Restartable parsing

Some use cases require parsing to continue after an error. `granit-parser` supports continuing past unresolved
aliases (see [`Event::UnresolvedAlias`](https://docs.rs/granit-parser/latest/granit_parser/enum.Event.html#variant.UnresolvedAlias)), 
and continuing with the next document after a document error using
[`Parser::proceed_next_document`](https://docs.rs/granit-parser/latest/granit_parser/struct.Parser.html#method.proceed_next_document).

### Anchor events
Anchors were traditionally seen as opaque numeric IDs. 
The new opt-in [`Event::Anchor`](https://docs.rs/granit-parser/latest/granit_parser/enum.Event.html#variant.Anchor) event 
provides the anchor name if required for advanced processing. The companion `Span` of this event provides anchor location.

### Unsupported features
YAML 1.1 line breaks. YAML 1.1 defines NEL (U+0085), LS (U+2028) and PS (U+2029) as line breaks, this parser
uses `\n`/`\r` only line-breaking that as expected in YAML 1.2. 

## Testing

Run the tests from a repository checkout with the official YAML suite initialized:

```sh
git submodule update --init tests/yaml-test-suite
cargo test --features test-equality
cargo test --features test-equality --test yaml-test-suite -- ::strict
```

The repository enables `test-equality` through a development dependency so its unit and
integration tests compare real parser values. When testing an extracted crate package,
enable the feature explicitly; the YAML suite still requires a repository checkout.
Downstream tests can enable it without changing ordinary dependency builds:

```toml
[dev-dependencies]
granit-parser = { version = "3", features = ["test-equality"] }
```

An independent consumer fixture verifies that constructors work and equality remains
unavailable without the feature, then checks the opt-in comparisons:

```sh
cargo test --manifest-path tests/api-contract/Cargo.toml --locked
cargo test --manifest-path tests/api-contract/Cargo.toml --locked --features test-equality
```

Strict mode runs every active upstream case without case-specific overrides. The
runner reports cases disabled by upstream's own `skip: true` metadata separately
(currently the four `ZYU8` directive variants). The relaxed-mode compatibility
override for `9C9N` does not apply to strict mode.

Missing or empty suite data fails the run. The suite data is not included in the
published crate, so running these conformance tests requires a repository checkout.
The filesystem-backed suite is not run under Miri isolation.

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
