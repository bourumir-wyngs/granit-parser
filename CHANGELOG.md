# Changelog

## v0.0.1

Initial `granit-parser` release after splitting the parser into its own
standalone package.

**Breaking Changes**:

- Renamed the package to `granit-parser` and the Rust crate path to
  `granit_parser`.
- Raised MSRV to `1.81.0`.
- Made the library `no_std` by default, with `alloc` enabled internally.
- Changed parser events, scanner tokens, receivers, and parse-result aliases to
  carry an input lifetime.
- Changed scalar, tag, anchor, alias, and tag-directive payloads to use
  `Cow<'input, ...>` so `&str` input can avoid allocations.
- Renamed `TScalarStyle` to `ScalarStyle`.
- Changed `Marker::index()` and error messages to report character positions.
  Byte positions are now optional and available through `Marker::byte_offset()`
  and `Span::byte_range()`.
- Added `Span::indent` for parser-emitted indentation hints.
- `debug_prints` no longer reads environment variables. Debug output is now
  controlled by the feature and a local compile-time toggle in `src/debug.rs`.

**Features**:

- Added `BorrowedInput` and byte-slicing support for inputs that can safely
  borrow from stable source storage.
- Added zero-copy scanning for `StrInput` across plain scalars, quoted scalars,
  anchors, aliases, tags, and tag directives when no decoding is needed.
- Added `Parser::new_from_iter` as the iterator-backed constructor.
- Added `ParserTrait`, `ParserStack`, and `ReplayParser` for stacked parser
  workflows and replayed event streams.
- Added anchor offset management for stacked/replayed parser use cases.
- Added `Tag::is_yaml_core_schema()` and `Display` for `Tag`.
- Exported the input module and scanner/token types needed by downstream tools.

**Fixes**:

- Reject unclosed and misplaced flow brackets with diagnostics pointing at the
  relevant opening or closing marker.
- Reject unclosed quoted scalars with an `unclosed quote` diagnostic.
- Reject comment-interrupted multiline plain scalars instead of silently
  truncating the scalar.
- Handle reserved directives according to YAML rules while still rejecting
  malformed `%YAML` directives.
- Handle JSON-style surrogate pairs in quoted scalars and reject unpaired,
  reversed, or invalid surrogate escapes.
- Fix several flow mapping and flow sequence edge cases, including empty
  implicit keys and nested implicit mappings.
- Fix plain-scalar handling around `---`, `...`, `:`, `-`, tabs, and flow
  indicators.
- Fix block scalar indentation, chomping, kept trailing lines, tabs in scalar
  bodies, and non-ASCII span accounting.
- Fix document marker spans so `---` and `...` are emitted at the marker that
  triggered them.
- Fix spans for empty scalars, null values, mapping keys, non-ASCII input,
  comments, and block scalars.
- Avoid parser hangs and panic paths found by regression and fuzz tests.

**Performance**:

- Added fast ASCII paths for `StrInput`.
- Added chunked plain-scalar scanning and faster comment skipping.
- Switched several small parser/scanner stacks from `Vec` to `smallvec`.

**Build and Package**:

- Made the root crate standalone instead of relying on workspace-inherited
  package metadata.
- Removed unused root dependencies and dev-dependencies, including the old
  high-level YAML loader, `hashlink`, `miette`, `quickcheck`, `rustyline`, and
  `thiserror`.
- Added `smallvec` and disabled default features for `arraydeque`.
- Forbid unsafe code through crate attributes and Cargo lints.
- Renamed root tool binaries to `time_parser` and `run_parser`.

**Tests and CI**:

- Updated tests to use `granit_parser` and the lifetime/Cow-based event API.
- Run parser regression tests against both `StrInput` and `BufferedInput`.
- Replaced the YAML test-suite loader with a local fixture reader so the root
  crate no longer needs a high-level YAML parser as a dev-dependency.
- Added regression coverage for flow brackets, comments, document markers,
  block scalars, indentation hints, tabs, plain scalar indicators, surrogate
  escapes, non-ASCII positions, parser stacks, and fuzz-discovered inputs.
- Added a README example regression test.
- Added CI coverage for clippy, formatting, host library tests without default
  features, and `wasm32v1-none` `no_std` library checks.

**Documentation and Tools**:

- Rewrote crate docs and README examples for `granit-parser`.
- Added README coverage for repository tools.
- Added the standalone `tools/walk` helper crate for navigating parsed YAML
  spans.

## v0.0.6

**Fixes**:
- Fix emitting of tags with empty handles. `!tag` no longer emits as `!!tag`.

## v0.0.5

**Breaking Changes**:

- Emit `Cow<'input, Tag>` instead of `Tag` to avoid copies.

**Fixes**:

- 8ef76dcc: Fix `Marker`s for `null` and empty values.
- Fix `Span`s for collections to correctly mark the end of the collection.

**Changes**

- Exclude `yaml-test-suite` from the Cargo package.
- Bump `libtest-mimic` to `0.8.1`.

## v0.0.4

**Breaking Changes**:

- Allow events to borrow from the input.
- Rename `TScalarStyle` to `ScalarStyle`.

## v0.0.3

**Breaking Changes**:

- 926fdfb: Events now use spans rather than markers, allowing for tracking both
  the beginning and the end of scalars.
- 6c57b5b: Add a boolean to `DocumentStart` to know whether the start was
  explicit (`---`) or implicit.

**Features**:

- Add an `Input` interface to prepare the ground to future input-specific.
  optimizations (such as returning `Cow`'d strings when possible). This also
  potentially allows for user-defined optimizations.
- Add `Parser::new_from_iter` to load from an iterator. This automatically
  wraps using `BufferedInput`, which implements the new `Input` trait the
  `Parser` needs.

**Fixes**:

- 750c992: Add support for nested implicit flow mappings.
- 11cffc6: Fix error with deeply indented block scalars.
- d3b9641: Fix assertion that could erroneously trigger with multibyte
  characters.
- 95fe3fe: Fix parse errors when `---` appeared in the middle of plain scalars.
- 3358629: Fix infinite loop with `...` in plain scalars in flow contexts.
- Fix panics on other various erroneous inputs found while fuzzing.

**Internal changes**:

- Run all tests with both `Input` backends
- #15: Add fuzzing

## v0.0.2

This release does not provide much but is needed for the `saphyr` library to
depend on the new features.

**Breaking Changes**:

**Features**:
- Add `Marker::default()`
- Rework string handling in `ScanError`

**Fixes**:
- [yaml-rust2 #21](https://github.com/Ethiraric/yaml-rust2/issues/21#issuecomment-2053513507)
  Fix parser failing when a comment immediately follows a tag.

**Internal changes**:
- Various readability improvements and code cleanups
