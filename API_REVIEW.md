# API Review — 1.0.0 public API cleanup

Review of the uncommitted working-tree changes on top of `6567777` ("Preparing for 1.0.0 release"),
covering the scanner/parser/parser-stack API cleanup before the 1.0.0 release.

**Overall verdict:** the cleanup is coherent, well-documented, and mechanically sound. The public
surface shrinks to exactly what the CHANGELOG says, all removed items are either documented as
breaking or were never externally reachable, and the whole matrix builds and tests clean. One
finding should be addressed before tagging 1.0.0 (finding 1); the rest are design decisions to
confirm consciously now, because 1.0 is the last cheap opportunity.

## Verification performed

- `cargo test --all-features` — all suites pass (including the 402-case YAML test suite and 4 doctests).
- `cargo check --no-default-features` and `--no-default-features --features error_messages` — no_std builds pass.
- `cargo clippy --all-targets --all-features` — zero warnings (crate is `clippy::pedantic`).
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` — no broken intra-doc links after the removals.
- `tools/walk` (path dependency on the crate) builds against the new API.
- Grepped `src/`, `examples/`, `README.md`, `tools/` for stale uses of every removed item
  (`get_error`, `next_token`, `fetch_*`, `TEncoding`, `Event::Nothing`, `Comment::span`,
  `get_anchor_offset`, `new_str`, `Scanner`-`resolve`): none remain.
- Compared old vs. new `src/lib.rs` re-exports and cross-checked every CHANGELOG bullet against the
  actual API diff (see "CHANGELOG accuracy" below).
- Empirically tested the new `FusedIterator` impls with a scratch crate (finding 1).

## Findings

### 1. `FusedIterator` for `ParserStack` violates the trait contract (fix before tagging)

`src/parser_stack.rs:605` adds `impl FusedIterator for ParserStack`, but `prepare_for_push()`
resets the exhaustion latch (`self.stream_end_emitted = false`, `src/parser_stack.rs:299`). Pushing
a parser after the iterator has returned `None` makes `next()` return `Some` again — reactivation
is a deliberate, tested feature (`parser_stack_push_after_peeked_empty_stream_end_reactivates_stack`).

`FusedIterator` guarantees "once `None`, always `None`", and `Iterator::fuse()` specializes on the
marker to become a transparent pass-through. Verified with a scratch crate against this working tree:

```rust
let mut stack: ParserStack<'_, Empty<char>, StrInput<'_>> = ParserStack::new();
stack.push_str_parser(Parser::new_from_str("a: 1"), "first".into());
while stack.next().is_some() {}
assert!(stack.next().is_none());
stack.push_str_parser(Parser::new_from_str("b: 2"), "second".into());
assert!(stack.next().is_some());          // Some after None
// and through fuse():
assert!((&mut stack).fuse().next().is_some()); // fuse() does not protect — pass-through
```

This is not unsafe (the trait is a safe marker), but downstream generic code and iterator adaptors
may rely on the guarantee. This is the same reason `std::sync::mpsc::TryIter` does *not* implement
`FusedIterator` — new items can appear after `None`.

Options, best first:

- **Remove `impl FusedIterator for ParserStack`** and document that pushing after exhaustion
  resumes iteration. The other three impls (`Parser`, `Scanner`, `ReplayParser`) genuinely latch
  and are correct — keep them. Adjust the CHANGELOG bullet accordingly.
- Alternatively, drop the `stream_end_emitted = false` reset in `prepare_for_push`. The existing
  reactivation test still passes (it reactivates via a *peeked* `StreamEnd`, which never sets the
  latch), but push-after-consumed-`None` would silently stay dead — an asymmetry that seems worse
  than removing the marker.

Note the `ParserTrait::next_event` contract is unaffected either way; this is purely about the
`Iterator`/`FusedIterator` marker.

### 2. `Event` and `TokenType` are exhaustive — confirm this is the intended 1.0 contract

`ErrorKind` is `#[non_exhaustive]` (`src/error.rs:140`) — good. `Event`, `TokenType`, and
`Placement` are exhaustively matchable, so adding any variant during 1.x is semver-major. The
crate's own history shows the risk: comment support added `Event::Comment`/`TokenType::Comment`
variants during 0.0.x. If any new event/token kind (or a new `Placement` such as "below") is
plausible in 1.x, mark those enums `#[non_exhaustive]` now; after 1.0 the change itself is breaking.

Counterpoint to weigh: forcing consumers (serde-saphyr) into exhaustive matches is a legitimate
design choice for a parser event model, and `ScalarStyle`/`StructureStyle` are closed by the YAML
spec. This is a decision to make consciously, not necessarily a defect — but it is one-way after 1.0.

### 3. `Comment` (and `Token`) keep public fields while `Tag` went private

`Tag` fields became private with a complete accessor/constructor set, but `Comment` still exposes
`pub text` / `pub placement` (relied on by tests and presumably consumers), and `Token` is a tuple
struct with two `pub` fields. Public fields make the structs literal-constructible, so adding any
field in 1.x is breaking (same pressure that just forced removing `Comment.span` as a breaking
change). Either privatize `Comment` for symmetry (it already has `new` + `with_placement` +
`trimmed_text`; only field *reads* would need accessors), or accept the layout freeze deliberately.
`Token`'s `(Span, TokenType)` shape is stable enough that freezing it seems fine.

### 4. Dual public paths for `ParserStack`/`ReplayParser`

Both `granit_parser::ParserStack` (new root re-export) and `granit_parser::parser_stack::ParserStack`
(pre-existing `pub mod`) are now public API, and both paths must be kept for all of 1.x. If the root
re-export is meant to be the canonical path (tests were migrated to it), consider making the module
private and re-exporting only from the root, matching how `parser` and `scanner` are already
private modules. Low stakes, but it halves the frozen surface.

### 5. README code block uses rustdoc hidden-line syntax but is never doctested

The new scanner example in `README.md` ends with `# Ok::<(), granit_parser::ScanError>(())`.
The README is not included via `#![doc = include_str!(...)]` and there is no README doctest harness,
so (a) the snippet is not compile-checked, and (b) the `#` line renders literally on GitHub and
crates.io. The API usage itself is correct (the identical `collect::<Result<Vec<_>, _>>()` pattern
is exercised in `tests/comment.rs`). Suggest either dropping the hidden line in favor of
`.expect(...)`, or wiring the README into doctests.

### 6. Informational

- **api-compat workflow**: baseline switched to the `1.0.0` tag, which does not exist yet, so the
  job self-skips (green badge, "1.0 API compatibility") until the release is tagged. Tag naming
  matches the repo convention (existing tags have no `v` prefix). Just be aware the badge is
  vacuous until the tag is pushed — after that it starts enforcing 1.0 compatibility for real.
- **CHANGELOG folded v0.0.8 into v1.0.0**: verified safe — crates.io's latest release is 0.0.7 and
  there is no 0.0.8 git tag, so 0.0.8 was never published and its notes correctly move under 1.0.0.
- **Scanner errors are now terminal**: the old public API could drive the scanner past a first
  error (the deleted test `scanner_reports_misplaced_bracket_when_resumed_after_unclosed_bracket`
  documented recovering a second error). The new latched behavior is a deliberate improvement,
  is stated in the CHANGELOG, and is enshrined by the new `scanner_error_is_terminal` test.

## CHANGELOG accuracy (cross-checked bullet by bullet)

Every breaking-change bullet matches the actual diff: scanner iterator item type and method
removals; `Event::Nothing` removal; `TEncoding` removal (it was an unnameable type — private
module, never re-exported, single variant — so the unit `StreamStart` is strictly better);
`Comment` span removal; `Tag` field privatization; `kind() -> &ErrorKind`; `ScanError::new`
consolidation (`new_str` was already removed in `6567777`); `anchor_offset` renames;
`resolve` → `push_include` merge; `AnyParser` privatization. The API-additions list
(`ParseResult`, `ParserStack`, `ReplayParser` re-exports; `FusedIterator` impls; iterable
`ReplayParser`) is also accurate — modulo finding 1, which may remove `ParserStack` from the
`FusedIterator` bullet.

Items privatized *without* CHANGELOG entries — `ScanResult`, `MarkerOffsets`, `Chomping`,
`QueuedComment` (deleted) — were verified unreachable from outside the crate before this change
(private `scanner` module, never re-exported), so no entries are needed. The changelog documents
exactly the real breakage, no more, no less.

## Verified-good details

- **Scanner fallible iterator**: the deferred-error machinery survives the `get_error` removal —
  errors found behind already-scanned comment tokens still drain the comments first and then
  surface as the iterator's single `Err` item (`next_queued_token`, `src/scanner.rs:1469`). The
  `failed` latch makes the iterator genuinely fused after both an error and clean EOF
  (`stream_end_produced` keeps returning `Ok(None)`).
- **`Parser` / `ReplayParser` fusedness**: `Parser::stream_end_emitted` is never reset by any
  public path; `ReplayParser` wraps `vec::IntoIter` with no refill method. Both impls are correct.
- **`Tag` privatization leaves no capability gap**: constructors `Tag::new` (clones handle into
  `original_handle` — sensible default, documented) and `Tag::with_original_handle` cover
  construction; `handle()`/`suffix()`/`original_handle()` plus the pre-existing
  `parts`/`original_parts`/`original`/`core_suffix`/`suffix_in_namespace` helpers cover all reads.
  README's ergonomic-helpers list was updated to match.
- **`ScanError` changes**: `kind() -> &ErrorKind` is consistent with `marker() -> &Marker`, and
  `ErrorKind: Clone` keeps ownership available (`.kind().clone()`, as the test helpers do).
  `new(marker, impl Into<String>)` is a clean consolidation.
- **`QueuedComment` shim deletion**: storing the span only on the `Token` removed ~60 lines of
  mirroring code and a per-comment `Span` duplication; the internal queue now stores the public
  `Comment` directly.
- **Scanner public surface after cleanup** is pleasingly small: `new`, `stream_started`,
  `stream_ended`, `mark`, plus the `Iterator`/`FusedIterator` impls. Queue management
  (`fetch_next_token`, `fetch_more_tokens`, `next_token`) is fully private, as the CHANGELOG states.
- **Test migration quality**: tests were rewritten to the public API idioms (`collect::<Result<_, _>>()`,
  `find_map(Result::err)`), and renamed to describe the new semantics
  (`scanner_error_is_terminal`, `iterator_next_emits_error_and_then_stays_empty`) rather than
  merely patched to compile.
