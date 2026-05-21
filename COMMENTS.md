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

## Task List

### API Design

- [ ] Add a public `Comment<'input>` type.
  - [ ] Store `span: Span` covering the whole source comment, including `#` and excluding the line break.
  - [ ] Store `text: Cow<'input, str>` containing the comment payload, excluding `#` and the line break.
  - [ ] Decide whether to preserve one optional leading space after `#` or return the raw payload exactly after `#`.
- [ ] Add opt-in comment collection to `Scanner`.
  - [ ] Add `Scanner::with_comments()` or equivalent builder-style method.
  - [ ] Add `Scanner::comments(&self) -> &[Comment<'input>]`.
  - [ ] Add `Scanner::take_comments(&mut self) -> Vec<Comment<'input>>`.
- [ ] Add opt-in comment collection to `Parser`.
  - [ ] Add `Parser::with_comments()` or equivalent builder-style method.
  - [ ] Add `Parser::comments(&self) -> &[Comment<'input>]`.
  - [ ] Add `Parser::take_comments(&mut self) -> Vec<Comment<'input>>`.
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
- [ ] Decide how `ParserStack` should expose comments.
  - [ ] Option 1: do not expose stacked comments initially.
  - [ ] Option 2: collect comments from each parser as it is drained.
  - [ ] Document whichever behavior is chosen.

### Tests

- [ ] Add scanner-level comment capture tests.
  - [ ] Full-line comments.
  - [ ] Indented full-line comments.
  - [ ] Trailing comments after plain scalars.
  - [ ] Multiple consecutive comment lines.
  - [ ] EOF immediately after a comment.
- [ ] Add parser-level comment capture tests.
  - [ ] Parsing events remain identical when comments are enabled.
  - [ ] Comments are available after full parse.
  - [ ] `take_comments()` drains collected comments.
- [ ] Add coverage for comments after syntax elements.
  - [ ] Directives.
  - [ ] Document markers.
  - [ ] Tags and anchors.
  - [ ] Flow delimiters and flow entries.
  - [ ] Quoted scalars.
  - [ ] Block scalar headers.
- [ ] Add negative/edge tests.
  - [ ] `#` inside single-quoted scalars is not a comment.
  - [ ] `#` inside double-quoted scalars is not a comment.
  - [ ] `#` inside block scalar content is not a comment.
  - [ ] Unseparated comments still error.
  - [ ] BS4K comment-interrupted multiline plain scalar still errors.
- [ ] Add non-ASCII tests.
  - [ ] Comment payload with multi-byte Unicode.
  - [ ] Correct character offsets.
  - [ ] Correct byte offsets for `StrInput`.
  - [ ] Matching behavior between `StrInput` and `BufferedInput`.

### Documentation

- [ ] Document comment support in `README.md`.
- [ ] Add a crate-level example in `src/lib.rs`.
- [ ] Explain that comments are presentation metadata, not YAML data events.
- [ ] Document `span.slice(source)` behavior for comments.
- [ ] Update `CHANGELOG.md` when the implementation is complete.

### Follow-up API Option

- [ ] Consider a future streaming API that preserves exact interleaving.
  - [ ] Possible shape: `ParseItem<'input> = Event(Event<'input>, Span) | Comment(Comment<'input>)`.
  - [ ] Keep this separate from the first implementation to avoid broad parser state-machine churn.
