# Comment Support Plan

This document tracks work needed to preserve YAML comments as parser events with
normal parser spans. This is a breaking event-stream change. When implementing
changes, always add relevant unit tests, format all code at the end and ensure
pedantic Clippy is passing. All public functions and classes must have
documentation comments.

Once you implement the step, make edit in this document marking it as done.

## Goals

- Preserve comments as presentation data in the parser event stream.
- Emit comments as parser events with the existing `(Event, Span)` result shape.
- Keep comment spans local to the source parsed by the parser that emitted them.
- Preserve current scanner/parser error behavior.
- Support both `StrInput` and streaming `BufferedInput`.

## Non-goals

- Do not make comments part of the semantic YAML tree.
- Do not alter scalar spans to include trailing comments.
- Do not add source IDs, file names, or include provenance to `Span` or `Event`.
- Do not add a separate `ParserStack` comment collection API for the event-based
  implementation.

## Event Semantics

- Comments are emitted in source order as parser events.
- A comment event uses the same spanned parser output as other events:
  `(Event::Comment(text, placement), span)`.
- The comment span covers the whole source comment, including `#` and excluding
  the line break.
- The comment event payload is the raw text exactly after `#`, excluding only the
  line break. Preserve leading spaces, including one space immediately after `#`.
- The comment event placement is presentation metadata that provides a best-effort
  positional hint such as `Above`, `Right`, `Free`, or `Last`.
- Comment events are presentation metadata. Consumers building YAML data trees
  must ignore them.
- Comment events do not advance the YAML grammar state; after a comment is
  emitted, parsing resumes in the same parser state.
- `ParserStack` and `ReplayParser` should preserve included-document comments by
  carrying them through the normal event stream.
- Included-document comment spans remain local to the included document/source,
  just as other included-document event spans do today. Source/file provenance is
  left to callers that layer names around `ParserStack`.

## Superseded Collection Design

- The earlier `comments()` / `take_comments()` collection design is superseded.
- Do not preserve comment storage as a second public API after `Event::Comment`
  is implemented.
- Remove `Parser::with_comments()`, `Parser::comments()`,
  `Parser::take_comments()`, `Scanner::with_comments()`, `Scanner::comments()`,
  and `Scanner::take_comments()`.
- Remove scanner-side comment collection fields (`comments`, `collect_comments`)
  unless they are temporarily needed during migration.
- Replace collection tests with event-stream tests instead of maintaining both
  behaviors.

## Task List

### API Design

- [x] Add `Event::Comment(Cow<'input, str>, Placement)`.
  - [x] Store the raw comment payload exactly after `#`, excluding only the line break.
  - [x] Preserve leading spaces in the payload, including a single space immediately after `#`.
  - [x] Store a best-effort placement hint for correlating presentation comments.
  - [x] Use the companion parser `Span` for the full source comment range.
- [x] Add `TokenType::Comment(Comment<'input>)` or an equivalent internal scanner token.
  - [x] Store the comment payload in the token.
  - [x] Store the full source comment range in the token span.
  - [x] Store the scanner's initial placement hint.
- [x] Decide whether the public `Comment<'input>` type remains useful after the event-based API.
  - [x] If kept, document it as a convenience struct rather than the primary parser API.
  - [x] If removed, migrate tests and docs to `Event::Comment` plus `Span`. Not applicable:
        `Comment` is kept as a convenience struct.
- [x] Remove the superseded opt-in collection APIs.
  - [x] Remove `Parser::with_comments()`, `Parser::comments()`, and `Parser::take_comments()`.
  - [x] Remove `Scanner::with_comments()`, `Scanner::comments()`, and `Scanner::take_comments()`.
  - [x] Remove collection-semantics documentation once the event API is complete.

### Scanner Capture And Tokenization

- [x] Rework scanner comment capture to emit comment tokens instead of only collecting
  comments in side storage.
  - [x] Capture comments through `skip_to_next_token`, `skip_yaml_whitespace`, and
        `skip_ws_to_eol` without losing the current error behavior.
  - [x] Preserve the current scanner error behavior; comment tokens are part of the
        normal token stream after this change.
  - [x] Keep `#` inside quoted scalars and block scalar content out of the comment stream.
  - [x] Preserve zero-copy comment payloads for `StrInput` where possible.
  - [x] Preserve owned comment payloads for `BufferedInput`.
  - [x] Keep marker accounting identical to normal skipped-comment behavior.
- [x] Remove scanner-side comment collection storage once comment tokens are emitted.

### Discard Points To Rework

- [x] Rework `Scanner::skip_to_next_token`.
  - [x] Emit full-line and inter-token comment tokens.
  - [x] Preserve line break consumption and simple-key behavior.
- [x] Rework `Scanner::skip_yaml_whitespace`.
  - [x] Emit comments after explicit key whitespace.
  - [x] Preserve the current `expected whitespace` behavior.
- [x] Rework `Scanner::skip_ws_to_eol`.
  - [x] Emit comment tokens before end-of-line comments are discarded.
  - [x] Preserve `SkipTabs` results.
  - [x] Preserve the existing error for comments not separated from tokens by whitespace.
  - [x] Detect the unseparated-comment error before comment token emission.
  - [x] Do not consume or emit an unseparated `#` as a valid comment.
- [x] Review all callers of `skip_ws_to_eol`.
  - [x] Directives.
  - [x] Document end marker handling.
  - [x] Flow collection start/end.
  - [x] Flow entries.
  - [x] Block entries.
  - [x] Block scalar headers.
  - [x] Quoted scalars after the closing quote.
  - [x] Plain scalar tab handling.
  - [x] Mapping values after `:`.

### Parser Integration

- [x] Emit `Event::Comment` from `Parser::next_event_impl` before normal YAML
  grammar-state handling.
  - [x] Consume exactly one comment token per emitted comment event.
  - [x] Leave the parser state unchanged after emitting a comment event.
  - [x] Ensure comments after `StreamStart`, around document markers, inside flow
        collections, and before `StreamEnd` are emitted in source order.
- [x] Keep the parser result shape unchanged: `Result<(Event<'input>, Span), ScanError>`.
- [x] Update `peek`, `next_event`, `load`, `try_load`, and iterator behavior to
  carry comment events consistently.
- [x] Preserve comments in `ReplayParser` without a side channel.
  - [x] Replayed event streams should store comment events in the existing event vector.
  - [x] `ReplayParser::load` and `try_load` should forward comment events like all other events.
- [x] Preserve comments in included documents through `ParserStack`.
  - [x] `ParserStack::resolve` should preserve comments because includes are replayed
        through the normal parser event stream.
  - [x] Included-document comment spans remain local to the included source.
  - [x] Do not add source IDs, file names, or a dedicated comment collection API to
        `ParserStack` for this implementation.

### Tests

- [x] Add parser event tests for `Event::Comment`.
  - [x] Full-line comments are emitted as events with spans.
  - [x] Indented full-line comments are emitted as events with spans.
  - [x] Trailing comments after plain scalars are emitted after the scalar event.
  - [x] Empty-ish comments `#` and `# ` preserve payload text.
  - [x] CRLF comment spans end before `\r`, not after `\n`.
  - [x] `peek()` returns and preserves a pending comment event.
  - [x] `load` and `try_load` deliver comment events to receivers.
- [x] Add parser-stack and replay tests for comment events.
  - [x] `ReplayParser` preserves comment events.
  - [x] `ParserStack` forwards comment events from stacked parsers.
  - [x] `ParserStack::resolve` / `push_include` forwards comments from included documents.
  - [x] Included-document comment spans remain local to the included source.
- [x] Update existing parser-level collection tests.
  - [x] Replace "event stream unchanged when comments are enabled" expectations with
        explicit `Event::Comment` expectations.
  - [x] Remove `comments()` and `take_comments()` drain tests.
- [x] Migrate scanner collection tests to scanner-token or parser-event tests.
  - [x] Full-line comments.
  - [x] Indented full-line comments.
  - [x] Trailing comments after plain scalars.
  - [x] Multiple consecutive comment lines.
  - [x] EOF immediately after a comment.
  - [x] Empty-ish comment: `#`.
  - [x] Empty-ish comment with one payload space: `# `.
  - [x] CRLF comment spans end before `\r`, not after `\n`.
  - [x] Comments after syntax elements: directives, document markers, tags,
        anchors, aliases, flow delimiters, flow entries, block entries, block
        scalar headers, quoted scalars, plain scalars, and mapping values.
  - [x] Negative cases: `#` inside quoted scalars and block scalar content,
        unseparated comments, and BS4K comment-interrupted multiline plain scalar.
  - [x] Non-ASCII payloads, character offsets, byte offsets for `StrInput`, and
        matching behavior between `StrInput` and `BufferedInput`.

### Documentation
- [x] Add a crate-level example in `src/lib.rs`.
- [x] Explain that `Event::Comment` is presentation metadata, not YAML data.
- [x] Document that comment locations use the normal companion `Span`.
- [x] Document that non-spanned receivers receive `Event::Comment(text, placement)`,
      while spanned receivers receive the comment span in `on_event`.
- [x] Document `span.slice(source)` behavior for comments.
- [x] Document that included-document comment spans are local to the included source,
      matching existing included-document event spans.
- [x] Update `CHANGELOG.md` when the implementation is complete.
