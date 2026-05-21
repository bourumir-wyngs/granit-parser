# Comment Support Plan

This document tracks work needed to preserve YAML comments without changing the
default parser behavior. When implementing changes, always add relevant unit tests,
format all code at the end and ensure pedantic Clippy is passing. All public
functions and classes must have documentation comments.

Once you implement the step, make edit in this document marking it as done.

## Goals

- Preserve comments as presentation data for callers that opt in.
- Keep the existing `Parser` event stream unchanged by default.
- Preserve current scanner/parser error behavior.
- Avoid extra allocations on the current no-comments fast path.
- Support both `StrInput` and streaming `BufferedInput`.

## Non-goals

- Do not make comments part of the semantic YAML tree.
- Do not add `Event::Comment` in the first implementation.
- Do not alter scalar spans to include trailing comments.

## Collection Semantics

- `comments()` and `take_comments()` are collection APIs, not streaming interleaving APIs.
- Comments are returned in source order, up to the scanner's current input position.
- During parsing, collected comments may be ahead of the event most recently returned.
- This can happen because the scanner buffers tokens and may scan ahead to resolve simple keys.
- `peek()` may also scan input and collect comments without consuming a parser event.
- Do not document or rely on `comments()` as "comments before the current event".
- Consumers that need exact event/comment interleaving should use a future streaming API such as
  `ParseItem::Event | ParseItem::Comment`.

## Task List

### API Design

- [x] Add a public `Comment<'input>` type.
  - [x] Store `span: Span` covering the whole source comment, including `#` and excluding the line break.
  - [x] Store `text: Cow<'input, str>` containing the raw comment payload exactly after `#`, excluding only the line break.
  - [x] Preserve leading spaces in `text`, including a single space immediately after `#` when present.
  - [x] Add an ergonomic `trimmed_text()` helper. Do not strip payload text during capture.
- [x] Add opt-in comment collection to `Scanner`.
  - [x] Add `Scanner::with_comments()`.
  - [x] Add `Scanner::comments(&self) -> &[Comment<'input>]`.
  - [x] Add `Scanner::take_comments(&mut self) -> Vec<Comment<'input>>`.
- [x] Add opt-in comment collection to `Parser`.
  - [x] Add `Parser::with_comments()`.
  - [x] Add `Parser::comments(&self) -> &[Comment<'input>]`.
  - [x] Add `Parser::take_comments(&mut self) -> Vec<Comment<'input>>`.
  - [x] Define `comments()` as collected comments in source order up to the scanner's current position, not comments interleaved before the current event.
- [x] Re-export `Comment` from `src/lib.rs`.
- [x] Keep `Event` and `TokenType` unchanged for the first implementation unless an API review decides otherwise.

### Scanner Capture

- [x] Add scanner storage for collected comments.
  - [x] `comments: Vec<Comment<'input>>`.
  - [x] `collect_comments: bool`.
- [x] Add a scanner helper for comment capture.
  - [x] Capture the start marker before consuming `#`.
  - [x] Consume through, but not including, the line break or EOF.
  - [x] Capture the end marker after the comment payload.
  - [x] Return/update `Span::new(start, end)`.
- [x] Preserve zero-copy comments for `StrInput`.
  - [x] Use byte offsets and `slice_borrowed` when available.
  - [x] Fall back to owned strings if borrowing is unavailable.
- [x] Preserve owned comments for `BufferedInput`.
  - [x] Collect payload into a `String` while consuming characters.
  - [x] Consume through the normal lookahead buffer using `look_ch`, `peek`, and `skip`.
  - [x] Do not use raw iterator reads for comment capture; they can desynchronize already-buffered input.
  - [x] Keep marker accounting identical to current skip behavior.
- [x] Ensure `#` inside quoted scalars and block scalar content is not captured.

### Discard Points To Refactor

- [x] Refactor `Scanner::skip_to_next_token`.
  - [x] Capture full-line and inter-token comments when comment collection is enabled.
  - [x] Keep current `input.skip_while_non_breakz()` fast path when disabled.
  - [x] Preserve line break consumption and simple-key behavior.
- [x] Refactor `Scanner::skip_yaml_whitespace`.
  - [x] Capture comments after explicit key whitespace.
  - [x] Preserve the current `expected whitespace` behavior.
- [x] Refactor `Scanner::skip_ws_to_eol`.
  - [x] Keep the existing `Input::skip_ws_to_eol()` path when comments are disabled.
  - [x] Add a scanner-owned path when comments are enabled so comment text can be captured before it is discarded.
  - [x] Preserve `SkipTabs` results.
  - [x] Preserve the existing error for comments not separated from tokens by whitespace.
  - [x] Detect the unseparated-comment error before comment capture.
  - [x] Do not consume or record an unseparated `#` as a valid comment.
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

- [x] Thread scanner comment access through `Parser`.
  - [x] `Parser::comments()` delegates to `self.scanner.comments()`.
  - [x] `Parser::take_comments()` delegates to `self.scanner.take_comments()`.
- [x] Keep `Parser::next_event`, `peek`, `load`, and `try_load` output unchanged.
- [ ] Do not add a `ParserStack` comment API in the first implementation.
- [ ] Document that comments are exposed only on `Scanner` and `Parser` initially.
- [ ] Track `ParserStack` comment support as separate follow-up work.
  - [ ] Account for replayed event streams, which currently do not store comments.
  - [ ] Comments in the included documents must be supported as well. 

### Tests

- [x] Add scanner-level comment capture tests.
  - [x] Full-line comments.
  - [x] Indented full-line comments.
  - [x] Trailing comments after plain scalars.
  - [x] Multiple consecutive comment lines.
  - [x] EOF immediately after a comment.
  - [x] Empty-ish comment: `#`.
  - [x] Empty-ish comment with one payload space: `# `.
  - [x] CRLF comment line endings; the comment span must end before `\r`, not after `\n`.
- [x] Add parser-level comment capture tests.
  - [x] Parsing events remain identical when comments are enabled.
  - [x] Comments are available after full parse.
  - [x] `take_comments()` drains collected comments.
- [x] Add coverage for comments after syntax elements.
  - [x] Directives.
  - [x] Document start marker: `--- # document start comment`.
  - [x] Document end marker: `... # document end comment`.
  - [x] Tags and anchors.
  - [x] Flow delimiters and flow entries.
  - [x] Double-quoted scalar trailing comment: `key: "value" # after quoted scalar`.
  - [x] Single-quoted scalar trailing comment: `key: 'value' # after quoted scalar`.
  - [x] Plain scalar trailing comment: `key: value # after plain scalar`.
  - [x] Block scalar header comments.
- [x] Add negative/edge tests.
  - [x] `#` inside single-quoted scalars is not a comment.
  - [x] `#` inside double-quoted scalars is not a comment.
  - [x] `#` inside block scalar content is not a comment.
  - [x] `key: value#not-a-comment` treats `#not-a-comment` as scalar content.
  - [x] `key: "value"#must-error` still errors and does not capture `#must-error`.
  - [x] In `key: |\n  # this is block scalar content, not a captured comment`, the `#` line is scalar content only.
  - [x] Unseparated comments still error.
  - [x] Unseparated comment errors leave the invalid `#` unrecorded.
  - [x] BS4K comment-interrupted multiline plain scalar still errors.
- [x] Add non-ASCII tests.
  - [x] Comment payload with multi-byte Unicode.
  - [x] Plain scalar trailing Unicode comment: `key: value # unicode: äöü`.
  - [x] Correct character offsets.
  - [x] Correct byte offsets for `StrInput`.
  - [x] Matching behavior between `StrInput` and `BufferedInput`.

### Documentation

- [ ] Document comment support in `README.md`.
- [ ] Add a crate-level example in `src/lib.rs`.
- [ ] Explain that comments are presentation metadata, not YAML data events.
- [ ] Document that `comments()` is not a streaming interleaving API.
- [ ] Document `span.slice(source)` behavior for comments.
- [ ] Update `CHANGELOG.md` when the implementation is complete.

### Follow-up API Option

- [ ] Consider a future streaming API that preserves exact interleaving.
  - [ ] Possible shape: `ParseItem<'input> = Event(Event<'input>, Span) | Comment(Comment<'input>)`.
  - [ ] Keep this separate from the first implementation to avoid broad parser state-machine churn.
