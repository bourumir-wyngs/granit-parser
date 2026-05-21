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

- [ ] Add a public `Comment<'input>` type.
  - [ ] Store `span: Span` covering the whole source comment, including `#` and excluding the line break.
  - [ ] Store `text: Cow<'input, str>` containing the raw comment payload exactly after `#`, excluding only the line break.
  - [ ] Preserve leading spaces in `text`, including a single space immediately after `#` when present.
  - [ ] Consider adding an ergonomic `trimmed_text()` helper later; do not strip payload text during capture.
- [ ] Add opt-in comment collection to `Scanner`.
  - [ ] Add `Scanner::with_comments()` or equivalent builder-style method.
  - [ ] Add `Scanner::comments(&self) -> &[Comment<'input>]`.
  - [ ] Add `Scanner::take_comments(&mut self) -> Vec<Comment<'input>>`.
- [ ] Add opt-in comment collection to `Parser`.
  - [ ] Add `Parser::with_comments()` or equivalent builder-style method.
  - [ ] Add `Parser::comments(&self) -> &[Comment<'input>]`.
  - [ ] Add `Parser::take_comments(&mut self) -> Vec<Comment<'input>>`.
  - [ ] Define `comments()` as collected comments in source order up to the scanner's current position, not comments interleaved before the current event.
- [ ] Re-export `Comment` from `src/lib.rs`.
- [ ] Keep `Event` and `TokenType` unchanged for the first implementation unless an API review decides otherwise.

### Scanner Capture

- [ ] Add scanner storage for collected comments.
  - [ ] `comments: Vec<Comment<'input>>`.
  - [ ] `collect_comments: bool`.
- [ ] Add a scanner helper for comment capture.
  - [ ] Capture the start marker before consuming `#`.
  - [ ] Consume through, but not including, the line break or EOF.
  - [ ] Capture the end marker after the comment payload.
  - [ ] Return/update `Span::new(start, end)`.
- [ ] Preserve zero-copy comments for `StrInput`.
  - [ ] Use byte offsets and `slice_borrowed` when available.
  - [ ] Fall back to owned strings if borrowing is unavailable.
- [ ] Preserve owned comments for `BufferedInput`.
  - [ ] Collect payload into a `String` while consuming characters.
  - [ ] Consume through the normal lookahead buffer using `look_ch`, `peek`, and `skip`.
  - [ ] Do not use raw iterator reads for comment capture; they can desynchronize already-buffered input.
  - [ ] Keep marker accounting identical to current skip behavior.
- [ ] Ensure `#` inside quoted scalars and block scalar content is not captured.

### Discard Points To Refactor

- [ ] Refactor `Scanner::skip_to_next_token`.
  - [ ] Capture full-line and inter-token comments when comment collection is enabled.
  - [ ] Keep current `input.skip_while_non_breakz()` fast path when disabled.
  - [ ] Preserve line break consumption and simple-key behavior.
- [ ] Refactor `Scanner::skip_yaml_whitespace`.
  - [ ] Capture comments after explicit key whitespace.
  - [ ] Preserve the current `expected whitespace` behavior.
- [ ] Refactor `Scanner::skip_ws_to_eol`.
  - [ ] Keep the existing `Input::skip_ws_to_eol()` path when comments are disabled.
  - [ ] Add a scanner-owned path when comments are enabled so comment text can be captured before it is discarded.
  - [ ] Preserve `SkipTabs` results.
  - [ ] Preserve the existing error for comments not separated from tokens by whitespace.
  - [ ] Detect the unseparated-comment error before comment capture.
  - [ ] Do not consume or record an unseparated `#` as a valid comment.
- [ ] Review all callers of `skip_ws_to_eol`.
  - [ ] Directives.
  - [ ] Document end marker handling.
  - [ ] Flow collection start/end.
  - [ ] Flow entries.
  - [ ] Block entries.
  - [ ] Block scalar headers.
  - [ ] Quoted scalars after the closing quote.
  - [ ] Plain scalar tab handling.
  - [ ] Mapping values after `:`.

### Parser Integration

- [ ] Thread scanner comment access through `Parser`.
  - [ ] `Parser::comments()` delegates to `self.scanner.comments()`.
  - [ ] `Parser::take_comments()` delegates to `self.scanner.take_comments()`.
- [ ] Keep `Parser::next_event`, `peek`, `load`, and `try_load` output unchanged.
- [ ] Do not add a `ParserStack` comment API in the first implementation.
- [ ] Document that comments are exposed only on `Scanner` and `Parser` initially.
- [ ] Track `ParserStack` comment support as separate follow-up work.
  - [ ] Account for replayed event streams, which currently do not store comments.
  - [ ] Define a policy for comments from included/replayed inputs before implementing.

### Tests

- [ ] Add scanner-level comment capture tests.
  - [ ] Full-line comments.
  - [ ] Indented full-line comments.
  - [ ] Trailing comments after plain scalars.
  - [ ] Multiple consecutive comment lines.
  - [ ] EOF immediately after a comment.
  - [ ] Empty-ish comment: `#`.
  - [ ] Empty-ish comment with one payload space: `# `.
  - [ ] CRLF comment line endings; the comment span must end before `\r`, not after `\n`.
- [ ] Add parser-level comment capture tests.
  - [ ] Parsing events remain identical when comments are enabled.
  - [ ] Comments are available after full parse.
  - [ ] `take_comments()` drains collected comments.
- [ ] Add coverage for comments after syntax elements.
  - [ ] Directives.
  - [ ] Document start marker: `--- # document start comment`.
  - [ ] Document end marker: `... # document end comment`.
  - [ ] Tags and anchors.
  - [ ] Flow delimiters and flow entries.
  - [ ] Double-quoted scalar trailing comment: `key: "value" # after quoted scalar`.
  - [ ] Single-quoted scalar trailing comment: `key: 'value' # after quoted scalar`.
  - [ ] Plain scalar trailing comment: `key: value # after plain scalar`.
  - [ ] Block scalar header comments.
- [ ] Add negative/edge tests.
  - [ ] `#` inside single-quoted scalars is not a comment.
  - [ ] `#` inside double-quoted scalars is not a comment.
  - [ ] `#` inside block scalar content is not a comment.
  - [ ] `key: value#not-a-comment` treats `#not-a-comment` as scalar content.
  - [ ] `key: "value"#must-error` still errors and does not capture `#must-error`.
  - [ ] In `key: |\n  # this is block scalar content, not a captured comment`, the `#` line is scalar content only.
  - [ ] Unseparated comments still error.
  - [ ] Unseparated comment errors leave the invalid `#` unrecorded.
  - [ ] BS4K comment-interrupted multiline plain scalar still errors.
- [ ] Add non-ASCII tests.
  - [ ] Comment payload with multi-byte Unicode.
  - [ ] Plain scalar trailing Unicode comment: `key: value # unicode: äöü`.
  - [ ] Correct character offsets.
  - [ ] Correct byte offsets for `StrInput`.
  - [ ] Matching behavior between `StrInput` and `BufferedInput`.

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
