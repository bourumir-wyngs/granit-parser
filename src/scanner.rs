//! Home to the YAML Scanner.
//!
//! The scanner is the lowest-level parsing utility. It is the lexer / tokenizer, reading input a
//! character at a time and emitting tokens that can later be interpreted by the [`crate::parser`]
//! to check for more context and validity.
//!
//! Due to the grammar of YAML, the scanner has to have some context and is not error-free.

#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

use alloc::{
    borrow::{Cow, ToOwned},
    collections::VecDeque,
    string::String,
    vec::Vec,
};
use core::{char, fmt};

use crate::{
    char_traits::{
        as_hex, is_anchor_char, is_blank_or_breakz, is_break, is_breakz, is_flow, is_hex,
        is_tag_char, is_uri_char,
    },
    input::{BorrowedInput, SkipTabs},
};

/// The encoding of the input. Currently, only UTF-8 is supported.
#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum TEncoding {
    /// UTF-8 encoding.
    Utf8,
}

/// The style as which the scalar was written in the YAML document.
#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash, PartialOrd, Ord)]
pub enum ScalarStyle {
    /// A YAML plain scalar.
    Plain,
    /// A YAML single quoted scalar.
    SingleQuoted,
    /// A YAML double quoted scalar.
    DoubleQuoted,

    /// A YAML literal block (`|` block).
    ///
    /// See [8.1.2](https://yaml.org/spec/1.2.2/#812-literal-style).
    /// In literal blocks, any indented character is content, including white space characters.
    /// There is no way to escape characters, nor to break a long line.
    Literal,
    /// A YAML folded block (`>` block).
    ///
    /// See [8.1.3](https://yaml.org/spec/1.2.2/#813-folded-style).
    /// In folded blocks, any indented character is content, including white space characters.
    /// There is no way to escape characters. Content is subject to line folding, allowing breaking
    /// long lines.
    Folded,
}

/// Offset information for a [`Marker`].
///
/// YAML inputs can come from either a full `&str` (stable backing storage) or a streaming
/// character source. For stable inputs, we can track both a character index and a byte offset.
/// For streaming inputs, byte offsets are not generally useful (and may not correspond to any
/// meaningful underlying file/source), so they are optional.
#[derive(Clone, Copy, Debug, Default)]
pub struct MarkerOffsets {
    /// The index (in characters) in the source.
    chars: usize,
    /// The offset (in bytes) in the source, if available.
    bytes: Option<usize>,
}

impl PartialEq for MarkerOffsets {
    fn eq(&self, other: &Self) -> bool {
        // Byte offsets are an optional diagnostic enhancement and may differ between input
        // backends (e.g., `&str` vs streaming). Equality is therefore based on the character
        // position only.
        self.chars == other.chars
    }
}

impl Eq for MarkerOffsets {}

/// A location in a yaml document.
#[derive(Clone, Copy, PartialEq, Debug, Eq, Default)]
pub struct Marker {
    /// Offsets in the source.
    offsets: MarkerOffsets,
    /// The line (1-indexed).
    line: usize,
    /// The column (0-indexed).
    col: usize,
}

impl Marker {
    /// Create a new [`Marker`] at the given position.
    #[must_use]
    pub fn new(index: usize, line: usize, col: usize) -> Marker {
        Marker {
            offsets: MarkerOffsets {
                chars: index,
                bytes: None,
            },
            line,
            col,
        }
    }

    /// Return a copy of the marker with the given optional byte offset.
    #[must_use]
    pub fn with_byte_offset(mut self, byte_offset: Option<usize>) -> Marker {
        self.offsets.bytes = byte_offset;
        self
    }

    /// Return the index (in characters) of the marker in the source.
    #[must_use]
    pub fn index(&self) -> usize {
        self.offsets.chars
    }

    /// Return the byte offset of the marker in the source, if available.
    #[must_use]
    pub fn byte_offset(&self) -> Option<usize> {
        self.offsets.bytes
    }

    /// Return the line of the marker in the source.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// Return the column of the marker in the source.
    #[must_use]
    pub fn col(&self) -> usize {
        self.col
    }
}

/// A range of locations in a Yaml document.
#[derive(Clone, Copy, PartialEq, Debug, Eq, Default)]
pub struct Span {
    /// The start (inclusive) of the range.
    pub start: Marker,
    /// The end (exclusive) of the range.
    pub end: Marker,

    /// Optional indentation hint associated with this span.
    ///
    /// This is only meaningful for certain parser-emitted events (notably: block mapping keys).
    /// When indentation is not meaningful or cannot be provided, it must be `None`.
    pub indent: Option<usize>,
}

impl Span {
    /// Create a new [`Span`] for the given range.
    #[must_use]
    pub fn new(start: Marker, end: Marker) -> Span {
        Span {
            start,
            end,
            indent: None,
        }
    }

    /// Create a empty [`Span`] at a given location.
    ///
    /// An empty span doesn't contain any characters, but its position may still be meaningful.
    /// For example, for an indented sequence [`SequenceEnd`] has a location but an empty span.
    ///
    /// [`SequenceEnd`]: crate::Event::SequenceEnd
    #[must_use]
    pub fn empty(mark: Marker) -> Span {
        Span {
            start: mark,
            end: mark,
            indent: None,
        }
    }

    /// Return a copy of this [`Span`] with the given indentation hint.
    #[must_use]
    pub fn with_indent(mut self, indent: Option<usize>) -> Span {
        self.indent = indent;
        self
    }

    /// Return the length of the span (in characters).
    #[must_use]
    pub fn len(&self) -> usize {
        self.end.index() - self.start.index()
    }

    /// Return whether the [`Span`] has a length of zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the byte range of the span, if available.
    #[must_use]
    pub fn byte_range(&self) -> Option<core::ops::Range<usize>> {
        let start = self.start.byte_offset()?;
        let end = self.end.byte_offset()?;
        Some(start..end)
    }
}

/// An error that occurred while scanning.
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct ScanError {
    /// The position at which the error happened in the source.
    mark: Marker,
    /// Human-readable details about the error.
    info: String,
}

impl ScanError {
    /// Create a new error from a location and an error string.
    #[must_use]
    #[cold]
    pub fn new(loc: Marker, info: String) -> ScanError {
        ScanError { mark: loc, info }
    }

    /// Convenience alias for string slices.
    #[must_use]
    #[cold]
    pub fn new_str(loc: Marker, info: &str) -> ScanError {
        ScanError {
            mark: loc,
            info: info.to_owned(),
        }
    }

    /// Return the marker pointing to the error in the source.
    #[must_use]
    pub fn marker(&self) -> &Marker {
        &self.mark
    }

    /// Return the information string describing the error that happened.
    #[must_use]
    pub fn info(&self) -> &str {
        self.info.as_ref()
    }
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at char {} line {} column {}",
            self.info,
            self.mark.index(),
            self.mark.line(),
            self.mark.col() + 1
        )
    }
}

impl core::error::Error for ScanError {}

/// The contents of a scanner token.
#[derive(Clone, PartialEq, Debug, Eq)]
pub enum TokenType<'input> {
    /// The start of the stream. Sent first, before even [`TokenType::DocumentStart`].
    StreamStart(TEncoding),
    /// The end of the stream, EOF.
    StreamEnd,
    /// A YAML version directive.
    VersionDirective(
        /// Major
        u32,
        /// Minor
        u32,
    ),
    /// A YAML tag directive (e.g.: `!!str`, `!foo!bar`, ...).
    TagDirective(
        /// Handle
        Cow<'input, str>,
        /// Prefix
        Cow<'input, str>,
    ),
    /// The start of a YAML document (`---`).
    DocumentStart,
    /// The end of a YAML document (`...`).
    DocumentEnd,
    /// The start of a sequence block.
    ///
    /// Sequence blocks are arrays starting with a `-`.
    BlockSequenceStart,
    /// The start of a sequence mapping.
    ///
    /// Sequence mappings are "dictionaries" with "key: value" entries.
    BlockMappingStart,
    /// End of the corresponding `BlockSequenceStart` or `BlockMappingStart`.
    BlockEnd,
    /// Start of an inline sequence (`[ a, b ]`).
    FlowSequenceStart,
    /// End of an inline sequence.
    FlowSequenceEnd,
    /// Start of an inline mapping (`{ a: b, c: d }`).
    FlowMappingStart,
    /// End of an inline mapping.
    FlowMappingEnd,
    /// An entry in a block sequence (c.f.: [`TokenType::BlockSequenceStart`]).
    BlockEntry,
    /// An entry in a flow sequence (c.f.: [`TokenType::FlowSequenceStart`]).
    FlowEntry,
    /// A key in a mapping.
    Key,
    /// A value in a mapping.
    Value,
    /// A reference to an anchor.
    Alias(Cow<'input, str>),
    /// A YAML anchor (`&`/`*`).
    Anchor(Cow<'input, str>),
    /// A YAML tag (starting with bangs `!`).
    Tag(
        /// The handle of the tag.
        Cow<'input, str>,
        /// The suffix of the tag.
        Cow<'input, str>,
    ),
    /// A regular YAML scalar.
    Scalar(ScalarStyle, Cow<'input, str>),
    /// A reserved YAML directive.
    ReservedDirective(
        /// Name
        String,
        /// Parameters
        Vec<String>,
    ),
}

/// A scanner token.
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct Token<'input>(pub Span, pub TokenType<'input>);

/// A scalar that was parsed and may correspond to a simple key.
///
/// Upon scanning the following yaml:
/// ```yaml
/// a: b
/// ```
/// We do not know that `a` is a key for a map until we have reached the following `:`. For this
/// YAML, we would store `a` as a scalar token in the [`Scanner`], but not emit it yet. It would be
/// kept inside the scanner until more context is fetched and we are able to know whether it is a
/// plain scalar or a key.
///
/// For example, see the following 2 yaml documents:
/// ```yaml
/// ---
/// a: b # Here, `a` is a key.
/// ...
/// ---
/// a # Here, `a` is a plain scalar.
/// ...
/// ```
/// An instance of [`SimpleKey`] is created in the [`Scanner`] when such ambiguity occurs.
///
/// In both documents, scanning `a` would lead to the creation of a [`SimpleKey`] with
/// [`Self::possible`] set to `true`. The token for `a` would be pushed in the [`Scanner`] but not
/// yet emitted. Instead, more context would be fetched (through [`Scanner::fetch_more_tokens`]).
///
/// In the first document, upon reaching the `:`, the [`SimpleKey`] would be inspected and our
/// scalar `a` since it is a possible key, would be "turned" into a key. This is done by prepending
/// a [`TokenType::Key`] to our scalar token in the [`Scanner`]. This way, the
/// [`crate::parser::Parser`] would read the [`TokenType::Key`] token before the
/// [`TokenType::Scalar`] token.
///
/// In the second document however, reaching the EOF would stale the [`SimpleKey`] and no
/// [`TokenType::Key`] would be emitted by the scanner.
#[derive(Clone, PartialEq, Debug, Eq)]
struct SimpleKey {
    /// Whether the token this [`SimpleKey`] refers to may still be a key.
    ///
    /// Sometimes, when we have more context, we notice that what we thought could be a key no
    /// longer can be. In that case, [`Self::possible`] is set to `false`.
    ///
    /// For instance, let us consider the following invalid YAML:
    /// ```yaml
    /// key
    ///   : value
    /// ```
    /// Upon reading the `\n` after `key`, the [`SimpleKey`] that was created for `key` is staled
    /// and [`Self::possible`] set to `false`.
    possible: bool,
    /// Whether the token this [`SimpleKey`] refers to is required to be a key.
    ///
    /// With more context, we may know for sure that the token must be a key. If the YAML is
    /// invalid, it may happen that the token be deemed not a key. In such event, an error has to
    /// be raised. This boolean helps us know when to raise such error.
    ///
    /// TODO(ethiraric, 30/12/2023): Example of when this happens.
    required: bool,
    /// The index of the token referred to by the [`SimpleKey`].
    ///
    /// This is the index in the scanner, which takes into account both the tokens that have been
    /// emitted and those about to be emitted. See [`Scanner::tokens_parsed`] and
    /// [`Scanner::tokens`] for more details.
    token_number: usize,
    /// The position at which the token the [`SimpleKey`] refers to is.
    mark: Marker,
}

impl SimpleKey {
    /// Create a new [`SimpleKey`] at the given `Marker` and with the given flow level.
    fn new(mark: Marker) -> SimpleKey {
        SimpleKey {
            possible: false,
            required: false,
            token_number: 0,
            mark,
        }
    }
}

/// An indentation level on the stack of indentations.
#[derive(Clone, Debug, Default)]
struct Indent {
    /// The former indentation level.
    indent: isize,
    /// Whether, upon closing, this indents generates a `BlockEnd` token.
    ///
    /// There are levels of indentation which do not start a block. Examples of this would be:
    /// ```yaml
    /// -
    ///   foo # ok
    /// -
    /// bar # ko, bar needs to be indented further than the `-`.
    /// - [
    ///  baz, # ok
    /// quux # ko, quux needs to be indented further than the '-'.
    /// ] # ko, the closing bracket needs to be indented further than the `-`.
    /// ```
    ///
    /// The indentation level created by the `-` is for a single entry in the sequence. Emitting a
    /// `BlockEnd` when this indentation block ends would generate one `BlockEnd` per entry in the
    /// sequence, although we must have exactly one to end the sequence.
    needs_block_end: bool,
}

/// The knowledge we have about an implicit mapping.
///
/// Implicit mappings occur in flow sequences where the opening `{` for a mapping in a flow
/// sequence is omitted:
/// ```yaml
/// [ a: b, c: d ]
/// # Equivalent to
/// [ { a: b }, { c: d } ]
/// # Equivalent to
/// - a: b
/// - c: d
/// ```
///
/// The state must be carefully tracked for each nested flow sequence since we must emit a
/// [`FlowMappingStart`] event when encountering `a` and `c` in our previous example without a
/// character hinting us. Similarly, we must emit a [`FlowMappingEnd`] event when we reach the `,`
/// or the `]`. If the state is not properly tracked, we may omit to emit these events or emit them
/// out-of-order.
///
/// [`FlowMappingStart`]: TokenType::FlowMappingStart
/// [`FlowMappingEnd`]: TokenType::FlowMappingEnd
#[derive(Debug, PartialEq)]
enum ImplicitMappingState {
    /// It is possible there is an implicit mapping.
    ///
    /// This state is the one when we have just encountered the opening `[`. We need more context
    /// to know whether an implicit mapping follows.
    Possible,
    /// We are inside the implcit mapping.
    ///
    /// Note that this state is not set immediately (we need to have encountered the `:` to know).
    Inside(u8),
}

/// The YAML scanner.
///
/// This corresponds to the low-level interface when reading YAML. The scanner emits token as they
/// are read (akin to a lexer), but it also holds sufficient context to be able to disambiguate
/// some of the constructs. It has understanding of indentation and whitespace and is able to
/// generate error messages for some invalid YAML constructs.
///
/// It is however not a full parser and needs [`crate::parser::Parser`] to fully detect invalid
/// YAML documents.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Scanner<'input, T> {
    /// The input source.
    ///
    /// This must implement [`Input`].
    input: T,
    /// The position of the cursor within the reader.
    mark: Marker,
    /// Buffer for tokens to be returned.
    ///
    /// This buffer can hold some temporary tokens that are not yet ready to be returned. For
    /// instance, if we just read a scalar, it can be a value or a key if an implicit mapping
    /// follows. In this case, the token stays in the `VecDeque` but cannot be returned from
    /// [`Self::next`] until we have more context.
    tokens: VecDeque<Token<'input>>,
    /// The last error that happened.
    error: Option<ScanError>,

    /// Whether we have already emitted the `StreamStart` token.
    stream_start_produced: bool,
    /// Whether we have already emitted the `StreamEnd` token.
    stream_end_produced: bool,
    /// In some flow contexts, the value of a mapping is allowed to be adjacent to the `:`. When it
    /// is, the index at which the `:` may be must be stored in `adjacent_value_allowed_at`.
    adjacent_value_allowed_at: usize,
    /// Whether a simple key could potentially start at the current position.
    ///
    /// Simple keys are the opposite of complex keys which are keys starting with `?`.
    simple_key_allowed: bool,
    /// A stack of potential simple keys.
    ///
    /// Refer to the documentation of [`SimpleKey`] for a more in-depth explanation of what they
    /// are.
    simple_keys: smallvec::SmallVec<[SimpleKey; 8]>,
    /// The current indentation level.
    indent: isize,
    /// List of all block indentation levels we are in (except the current one).
    indents: smallvec::SmallVec<[Indent; 8]>,
    /// Level of nesting of flow sequences.
    flow_level: u8,
    /// The number of tokens that have been returned from the scanner.
    ///
    /// This excludes the tokens from [`Self::tokens`].
    tokens_parsed: usize,
    /// Whether a token is ready to be taken from [`Self::tokens`].
    token_available: bool,
    /// Whether all characters encountered since the last newline were whitespace.
    leading_whitespace: bool,
    /// Whether we started a flow mapping.
    ///
    /// This is used to detect implicit flow mapping starts such as:
    /// ```yaml
    /// [ : foo ] # { null: "foo" }
    /// ```
    flow_mapping_started: bool,
    /// An array of states, representing whether flow sequences have implicit mappings.
    ///
    /// When a flow mapping is possible (when encountering the first `[` or a `,` in a sequence),
    /// the state is set to [`Possible`].
    /// When we encounter the `:`, we know we are in an implicit mapping and can set the state to
    /// [`Inside`].
    ///
    /// There is one entry in this [`Vec`] for each nested flow sequence that we are in.
    /// The entries are created with the opening `]` and popped with the closing `]`.
    ///
    /// [`Possible`]: ImplicitMappingState::Possible
    /// [`Inside`]: ImplicitMappingState::Inside
    implicit_flow_mapping_states: smallvec::SmallVec<[ImplicitMappingState; 8]>,
    /// If a plain scalar was terminated by a `#` comment on its line, we set this
    /// to detect an illegal multiline continuation on the following line.
    interrupted_plain_by_comment: Option<Marker>,
    /// A stack of markers for opening brackets `[` and `{`.
    flow_markers: smallvec::SmallVec<[(Marker, char); 8]>,
    buf_leading_break: String,
    buf_trailing_breaks: String,
    buf_whitespaces: String,
}

impl<'input, T: BorrowedInput<'input>> Iterator for Scanner<'input, T> {
    type Item = Token<'input>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }
        match self.next_token() {
            Ok(Some(tok)) => {
                debug_print!(
                    "    \x1B[;32m\u{21B3} {:?} \x1B[;36m{:?}\x1B[;m",
                    tok.1,
                    tok.0
                );
                Some(tok)
            }
            Ok(tok) => tok,
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }
}

/// A convenience alias for scanner functions that may fail without returning a value.
pub type ScanResult = Result<(), ScanError>;

#[derive(Debug)]
enum FlowScalarBuf {
    /// Candidate for `Cow::Borrowed`.
    ///
    /// `start..end` is the committed verbatim range.
    /// `pending_ws_start..pending_ws_end` is a run of blanks that were seen but not yet
    /// committed (they must be dropped if followed by a line break).
    Borrowed {
        start: usize,
        end: usize,
        pending_ws_start: Option<usize>,
        pending_ws_end: usize,
    },
    Owned(String),
}

impl FlowScalarBuf {
    #[inline]
    fn new_borrowed(start: usize) -> Self {
        Self::Borrowed {
            start,
            end: start,
            pending_ws_start: None,
            pending_ws_end: start,
        }
    }

    #[inline]
    fn new_owned() -> Self {
        Self::Owned(String::new())
    }

    #[inline]
    fn as_owned_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Owned(s) => Some(s),
            Self::Borrowed { .. } => None,
        }
    }

    #[inline]
    fn commit_pending_ws(&mut self) {
        if let Self::Borrowed {
            end,
            pending_ws_start,
            pending_ws_end,
            ..
        } = self
        {
            if pending_ws_start.is_some() {
                *end = *pending_ws_end;
                *pending_ws_start = None;
            }
        }
    }

    #[inline]
    fn note_pending_ws(&mut self, ws_start: usize, ws_end: usize) {
        if let Self::Borrowed {
            pending_ws_start,
            pending_ws_end,
            ..
        } = self
        {
            if pending_ws_start.is_none() {
                *pending_ws_start = Some(ws_start);
            }
            *pending_ws_end = ws_end;
        }
    }

    #[inline]
    fn discard_pending_ws(&mut self) {
        if let Self::Borrowed {
            pending_ws_start,
            pending_ws_end,
            end,
            ..
        } = self
        {
            *pending_ws_start = None;
            *pending_ws_end = *end;
        }
    }
}

impl<'input, T: BorrowedInput<'input>> Scanner<'input, T> {
    #[inline]
    fn promote_flow_scalar_buf_to_owned(
        &self,
        start_mark: &Marker,
        buf: &mut FlowScalarBuf,
    ) -> Result<(), ScanError> {
        let FlowScalarBuf::Borrowed {
            start,
            end,
            pending_ws_start: _,
            pending_ws_end: _,
        } = *buf
        else {
            return Ok(());
        };

        let slice = self.input.slice_bytes(start, end).ok_or_else(|| {
            ScanError::new_str(
                *start_mark,
                "internal error: input advertised offsets but did not provide a slice",
            )
        })?;
        *buf = FlowScalarBuf::Owned(slice.to_owned());
        Ok(())
    }
    /// Try to borrow a slice from the underlying input.
    ///
    /// This method uses the [`BorrowedInput`] trait to safely obtain a slice with the `'input`
    /// lifetime. For inputs that support zero-copy slicing (like `StrInput`), this returns
    /// `Some(&'input str)`. For streaming inputs, this returns `None`.
    #[inline]
    fn try_borrow_slice(&self, start: usize, end: usize) -> Option<&'input str> {
        self.input.slice_borrowed(start, end)
    }

    /// Scan a tag handle for a `%TAG` directive as a `Cow<str>`.
    ///
    /// For `StrInput`, this will borrow from the input when possible. For other inputs, or if
    /// borrowing is not possible, it falls back to allocating.
    fn scan_tag_handle_directive_cow(
        &mut self,
        mark: &Marker,
    ) -> Result<Cow<'input, str>, ScanError> {
        let Some(start) = self.input.byte_offset() else {
            return Ok(Cow::Owned(self.scan_tag_handle(true, mark)?));
        };

        if self.input.look_ch() != '!' {
            return Err(ScanError::new_str(
                *mark,
                "while scanning a tag, did not find expected '!'",
            ));
        }

        // Consume the leading '!'.
        self.skip_non_blank();

        // Consume ns-word-char (ASCII alphanumeric, '_' or '-') characters.
        // This mirrors `StrInput::fetch_while_is_alpha` but avoids allocation.
        self.input.lookahead(1);
        while self.input.next_is_alpha() {
            self.skip_non_blank();
            self.input.lookahead(1);
        }

        // Optional trailing '!'.
        if self.input.peek() == '!' {
            self.skip_non_blank();
        }

        let Some(end) = self.input.byte_offset() else {
            // Should be impossible if `byte_offset()` was `Some` above, but keep safe fallback.
            return Ok(Cow::Owned(self.scan_tag_handle(true, mark)?));
        };

        let Some(slice) = self.try_borrow_slice(start, end) else {
            // Fall back to allocating if zero-copy borrow is not available.
            let slice = self.input.slice_bytes(start, end).ok_or_else(|| {
                ScanError::new_str(
                    *mark,
                    "internal error: input advertised slicing but did not provide a slice",
                )
            })?;
            if !slice.ends_with('!') && slice != "!" {
                return Err(ScanError::new_str(
                    *mark,
                    "while parsing a tag directive, did not find expected '!'",
                ));
            }
            return Ok(Cow::Owned(slice.to_owned()));
        };

        if !slice.ends_with('!') && slice != "!" {
            return Err(ScanError::new_str(
                *mark,
                "while parsing a tag directive, did not find expected '!'",
            ));
        }

        Ok(Cow::Borrowed(slice))
    }

    /// Scan a tag prefix for a `%TAG` directive as a `Cow<str>`.
    ///
    /// This borrows from `StrInput` only when no URI escape sequences are encountered. If a `%`
    /// escape is present, the prefix must be decoded and therefore allocated.
    fn scan_tag_prefix_directive_cow(
        &mut self,
        start_mark: &Marker,
    ) -> Result<Cow<'input, str>, ScanError> {
        let Some(start) = self.input.byte_offset() else {
            return Ok(Cow::Owned(self.scan_tag_prefix(start_mark)?));
        };

        // The prefix must start with either '!' (local) or a valid global tag char.
        if self.input.look_ch() == '!' {
            self.skip_non_blank();
        } else if !is_tag_char(self.input.peek()) {
            return Err(ScanError::new_str(
                *start_mark,
                "invalid global tag character",
            ));
        } else if self.input.peek() == '%' {
            // Needs decoding. Fall back to allocating path below.
        } else {
            self.skip_non_blank();
        }

        // Consume URI chars while we can stay in the borrowed path.
        while is_uri_char(self.input.look_ch()) {
            if self.input.peek() == '%' {
                break;
            }
            self.skip_non_blank();
        }

        // If we encountered an escape sequence, we must decode, therefore allocate.
        if self.input.peek() == '%' {
            let current = self
                .input
                .byte_offset()
                .expect("byte_offset() must remain available once enabled");
            let mut out = if let Some(slice) = self.input.slice_bytes(start, current) {
                slice.to_owned()
            } else {
                String::new()
            };

            while is_uri_char(self.input.look_ch()) {
                if self.input.peek() == '%' {
                    out.push(self.scan_uri_escapes(start_mark)?);
                } else {
                    out.push(self.input.peek());
                    self.skip_non_blank();
                }
            }
            return Ok(Cow::Owned(out));
        }

        let Some(end) = self.input.byte_offset() else {
            return Ok(Cow::Owned(self.scan_tag_prefix(start_mark)?));
        };

        let Some(slice) = self.try_borrow_slice(start, end) else {
            // Fall back to allocating if zero-copy borrow is not available.
            let slice = self.input.slice_bytes(start, end).ok_or_else(|| {
                ScanError::new_str(
                    *start_mark,
                    "internal error: input advertised slicing but did not provide a slice",
                )
            })?;
            return Ok(Cow::Owned(slice.to_owned()));
        };

        Ok(Cow::Borrowed(slice))
    }
    /// Creates the YAML tokenizer.
    pub fn new(input: T) -> Self {
        let initial_byte_offset = input.byte_offset();
        Scanner {
            input,
            mark: Marker::new(0, 1, 0).with_byte_offset(initial_byte_offset),
            tokens: VecDeque::with_capacity(64),
            error: None,

            stream_start_produced: false,
            stream_end_produced: false,
            adjacent_value_allowed_at: 0,
            simple_key_allowed: true,
            simple_keys: smallvec::SmallVec::new(),
            indent: -1,
            indents: smallvec::SmallVec::new(),
            flow_level: 0,
            tokens_parsed: 0,
            token_available: false,
            leading_whitespace: true,
            flow_mapping_started: false,
            implicit_flow_mapping_states: smallvec::SmallVec::new(),
            flow_markers: smallvec::SmallVec::new(),
            interrupted_plain_by_comment: None,

            buf_leading_break: String::with_capacity(128),
            buf_trailing_breaks: String::with_capacity(128),
            buf_whitespaces: String::with_capacity(128),
        }
    }

    /// Get a copy of the last error that was encountered, if any.
    ///
    /// This does not clear the error state and further calls to [`Self::get_error`] will return (a
    /// clone of) the same error.
    #[inline]
    pub fn get_error(&self) -> Option<ScanError> {
        self.error.clone()
    }

    #[cold]
    fn simple_key_expected(&self) -> ScanError {
        ScanError::new_str(self.mark, "simple key expected")
    }

    #[cold]
    fn unclosed_bracket(mark: Marker, bracket: char) -> ScanError {
        ScanError::new(mark, format!("unclosed bracket '{bracket}'"))
    }

    /// Consume the next character. It is assumed the next character is a blank.
    #[inline]
    fn skip_blank(&mut self) {
        self.input.skip();

        self.mark.offsets.chars += 1;
        self.mark.col += 1;
        self.mark.offsets.bytes = self.input.byte_offset();
    }

    /// Consume the next character. It is assumed the next character is not a blank.
    #[inline]
    fn skip_non_blank(&mut self) {
        self.input.skip();

        self.mark.offsets.chars += 1;
        self.mark.col += 1;
        self.mark.offsets.bytes = self.input.byte_offset();
        self.leading_whitespace = false;
    }

    /// Consume the next characters. It is assumed none of the next characters are blanks.
    #[inline]
    fn skip_n_non_blank(&mut self, count: usize) {
        for _ in 0..count {
            self.input.skip();
            self.mark.offsets.chars += 1;
            self.mark.col += 1;
        }
        self.mark.offsets.bytes = self.input.byte_offset();
        self.leading_whitespace = false;
    }

    /// Consume the next character. It is assumed the next character is a newline.
    #[inline]
    fn skip_nl(&mut self) {
        self.input.skip();

        self.mark.offsets.chars += 1;
        self.mark.col = 0;
        self.mark.line += 1;
        self.mark.offsets.bytes = self.input.byte_offset();
        self.leading_whitespace = true;
    }

    /// Consume a linebreak (either CR, LF or CRLF), if any. Do nothing if there's none.
    #[inline]
    fn skip_linebreak(&mut self) {
        if self.input.next_2_are('\r', '\n') {
            // While technically not a blank, this does not matter as `self.leading_whitespace`
            // will be reset by `skip_nl`.
            self.skip_blank();
            self.skip_nl();
        } else if self.input.next_is_break() {
            self.skip_nl();
        }
    }

    /// Return whether the [`TokenType::StreamStart`] event has been emitted.
    #[inline]
    pub fn stream_started(&self) -> bool {
        self.stream_start_produced
    }

    /// Return whether the [`TokenType::StreamEnd`] event has been emitted.
    #[inline]
    pub fn stream_ended(&self) -> bool {
        self.stream_end_produced
    }

    /// Get the current position in the input stream.
    #[inline]
    pub fn mark(&self) -> Marker {
        self.mark
    }

    // Read and consume a line break (either `\r`, `\n` or `\r\n`).
    //
    // A `\n` is pushed into `s`.
    //
    // # Panics (in debug)
    // If the next characters do not correspond to a line break.
    #[inline]
    fn read_break(&mut self, s: &mut String) {
        self.skip_break();
        s.push('\n');
    }

    // Read and consume a line break (either `\r`, `\n` or `\r\n`).
    //
    // # Panics (in debug)
    // If the next characters do not correspond to a line break.
    #[inline]
    fn skip_break(&mut self) {
        let c = self.input.peek();
        let nc = self.input.peek_nth(1);
        debug_assert!(is_break(c));
        if c == '\r' && nc == '\n' {
            self.skip_blank();
        }
        self.skip_nl();
    }

    /// Insert a token at the given position.
    fn insert_token(&mut self, pos: usize, tok: Token<'input>) {
        let old_len = self.tokens.len();
        assert!(pos <= old_len);
        self.tokens.insert(pos, tok);
    }

    #[inline]
    fn allow_simple_key(&mut self) {
        self.simple_key_allowed = true;
    }

    #[inline]
    fn disallow_simple_key(&mut self) {
        self.simple_key_allowed = false;
    }

    /// Fetch the next token in the stream.
    ///
    /// # Errors
    /// Returns `ScanError` when the scanner does not find the next expected token.
    pub fn fetch_next_token(&mut self) -> ScanResult {
        self.input.lookahead(1);

        if !self.stream_start_produced {
            self.fetch_stream_start();
            return Ok(());
        }
        self.skip_to_next_token()?;

        debug_print!(
            "  \x1B[38;5;244m\u{2192} fetch_next_token after whitespace {:?} {:?}\x1B[m",
            self.mark,
            self.input.peek()
        );

        self.stale_simple_keys()?;

        let mark = self.mark;
        self.unroll_indent(mark.col as isize);

        self.input.lookahead(4);

        if self.input.next_is_z() {
            self.fetch_stream_end()?;
            return Ok(());
        }

        if self.mark.col == 0 {
            if self.input.next_char_is('%') {
                return self.fetch_directive();
            } else if self.input.next_is_document_start() {
                return self.fetch_document_indicator(TokenType::DocumentStart);
            } else if self.input.next_is_document_end() {
                self.fetch_document_indicator(TokenType::DocumentEnd)?;
                self.skip_ws_to_eol(SkipTabs::Yes)?;
                if !self.input.next_is_breakz() {
                    return Err(ScanError::new_str(
                        self.mark,
                        "invalid content after document end marker",
                    ));
                }
                return Ok(());
            }
        }

        if (self.mark.col as isize) < self.indent {
            self.input.lookahead(1);
            let c = self.input.peek();
            if self.flow_level == 0 || !matches!(c, ']' | '}' | ',') {
                return Err(ScanError::new_str(self.mark, "invalid indentation"));
            }
        }

        let c = self.input.peek();
        let nc = self.input.peek_nth(1);
        match c {
            '[' => self.fetch_flow_collection_start(TokenType::FlowSequenceStart),
            '{' => self.fetch_flow_collection_start(TokenType::FlowMappingStart),
            ']' => self.fetch_flow_collection_end(TokenType::FlowSequenceEnd),
            '}' => self.fetch_flow_collection_end(TokenType::FlowMappingEnd),
            ',' => self.fetch_flow_entry(),
            '-' if is_blank_or_breakz(nc) => self.fetch_block_entry(),
            '?' if is_blank_or_breakz(nc) => self.fetch_key(),
            ':' if is_blank_or_breakz(nc) => self.fetch_value(),
            ':' if self.flow_level > 0
                && (is_flow(nc) || self.mark.index() == self.adjacent_value_allowed_at) =>
            {
                self.fetch_flow_value()
            }
            // Is it an alias?
            '*' => self.fetch_anchor(true),
            // Is it an anchor?
            '&' => self.fetch_anchor(false),
            '!' => self.fetch_tag(),
            // Is it a literal scalar?
            '|' if self.flow_level == 0 => self.fetch_block_scalar(true),
            // Is it a folded scalar?
            '>' if self.flow_level == 0 => self.fetch_block_scalar(false),
            '\'' => self.fetch_flow_scalar(true),
            '"' => self.fetch_flow_scalar(false),
            // plain scalar
            '-' if !is_blank_or_breakz(nc) => self.fetch_plain_scalar(),
            ':' | '?' if !is_blank_or_breakz(nc) && self.flow_level == 0 => {
                self.fetch_plain_scalar()
            }
            '%' | '@' | '`' => Err(ScanError::new(
                self.mark,
                format!("unexpected character: `{c}'"),
            )),
            _ => self.fetch_plain_scalar(),
        }
    }

    /// Return the next token in the stream.
    /// # Errors
    /// Returns `ScanError` when scanning fails to find an expected next token.
    pub fn next_token(&mut self) -> Result<Option<Token<'input>>, ScanError> {
        if self.stream_end_produced {
            return Ok(None);
        }

        if !self.token_available {
            self.fetch_more_tokens()?;
        }
        let Some(t) = self.tokens.pop_front() else {
            return Err(ScanError::new_str(
                self.mark,
                "did not find expected next token",
            ));
        };
        self.token_available = false;
        self.tokens_parsed += 1;

        if let TokenType::StreamEnd = t.1 {
            self.stream_end_produced = true;
        }
        Ok(Some(t))
    }

    /// Fetch tokens from the token stream.
    /// # Errors
    /// Returns `ScanError` when loading fails.
    pub fn fetch_more_tokens(&mut self) -> ScanResult {
        let mut need_more;
        loop {
            if self.tokens.is_empty() {
                need_more = true;
            } else {
                need_more = false;
                // Stale potential keys that we know won't be keys.
                self.stale_simple_keys()?;
                // If our next token to be emitted may be a key, fetch more context.
                for sk in &self.simple_keys {
                    if sk.possible && sk.token_number == self.tokens_parsed {
                        need_more = true;
                        break;
                    }
                }
            }

            // Stop fetching immediately after document end/start markers
            // to allow the parser to emit the event before reading more content.
            if let Some(token) = self.tokens.back() {
                if matches!(token.1, TokenType::DocumentEnd | TokenType::DocumentStart) {
                    break;
                }
            }

            if !need_more {
                break;
            }
            self.fetch_next_token()?;
        }
        self.token_available = true;

        Ok(())
    }

    /// Mark simple keys that can no longer be keys as such.
    ///
    /// This function sets `possible` to `false` to each key that, now we have more context, we
    /// know will not be keys.
    ///
    /// # Errors
    /// This function returns an error if one of the key we would stale was required to be a key.
    fn stale_simple_keys(&mut self) -> ScanResult {
        for sk in &mut self.simple_keys {
            if sk.possible
                // If not in a flow construct, simple keys cannot span multiple lines.
                && self.flow_level == 0
                && (sk.mark.line < self.mark.line || sk.mark.index() + 1024 < self.mark.index())
            {
                if sk.required {
                    return Err(ScanError::new_str(self.mark, "simple key expect ':'"));
                }
                sk.possible = false;
            }
        }
        Ok(())
    }

    /// Skip over all whitespace (`\t`, ` `, `\n`, `\r`) and comments until the next token.
    ///
    /// # Errors
    /// This function returns an error if a tabulation is encountered where there should not be
    /// one.
    fn skip_to_next_token(&mut self) -> ScanResult {
        // Hot-path helper: consume a single logical linebreak and apply simple-key rules.
        // (Kept local to ensure the compiler can inline it easily.)
        let consume_linebreak = |this: &mut Self| {
            this.input.lookahead(2);
            this.skip_linebreak();
            if this.flow_level == 0 {
                this.allow_simple_key();
            }
        };

        loop {
            match self.input.look_ch() {
                // Tabs may not be used as indentation (block context only).
                '\t' => {
                    if self.is_within_block()
                        && self.leading_whitespace
                        && (self.mark.col as isize) < self.indent
                    {
                        self.skip_ws_to_eol(SkipTabs::Yes)?;

                        // If we have content on that line with a tab, return an error.
                        if !self.input.next_is_breakz() {
                            return Err(ScanError::new_str(
                                self.mark,
                                "tabs disallowed within this context (block indentation)",
                            ));
                        }

                        // Micro-opt: if we stopped on a linebreak, consume it now (avoids another loop trip).
                        if matches!(self.input.look_ch(), '\n' | '\r') {
                            consume_linebreak(self);
                        }
                    } else {
                        // Non-indentation tab behaves like blank.
                        self.skip_blank();
                    }
                }

                ' ' => self.skip_blank(),

                '\n' | '\r' => consume_linebreak(self),

                '#' => {
                    // Skip the whole comment payload in one go.
                    let n = self.input.skip_while_non_breakz();
                    self.mark.offsets.chars += n;
                    self.mark.col += n;
                    self.mark.offsets.bytes = self.input.byte_offset();

                    // Micro-opt: comment-only lines are common; consume the following linebreak here.
                    if matches!(self.input.look_ch(), '\n' | '\r') {
                        consume_linebreak(self);
                    }
                }

                _ => break,
            }
        }

        // If a plain scalar was interrupted by a comment, and the next line could
        // continue the scalar in block context, this is invalid.
        if let Some(err_mark) = self.interrupted_plain_by_comment.take() {
            // BS4K should only trigger when the continuation would start on the immediate next
            // line (no intervening empty/comment-only lines). A blank line resets the folding
            // opportunity and thus should not error.
            let is_immediate_next_line = self.mark.line == err_mark.line + 1;

            // Optimization: do the cheap checks first; only then request extra lookahead / do deeper checks.
            if self.flow_level == 0
                && is_immediate_next_line
                && (self.mark.col as isize) > self.indent
            {
                // Ensure enough lookahead for:
                // - the checks below (peek/peek_nth)
                // - document indicator detection which needs 4 chars.
                self.input.lookahead(4);

                if !self.input.next_is_z()
                    && !self.input.next_is_document_indicator()
                    && self.input.next_can_be_plain_scalar(false)
                {
                    return Err(ScanError::new_str(
                        err_mark,
                        "comment intercepting the multiline text",
                    ));
                }
            }
        }

        Ok(())
    }

    /// Skip over YAML whitespace (` `, `\n`, `\r`).
    ///
    /// # Errors
    /// This function returns an error if no whitespace was found.
    fn skip_yaml_whitespace(&mut self) -> ScanResult {
        let mut need_whitespace = true;
        loop {
            match self.input.look_ch() {
                ' ' => {
                    self.skip_blank();

                    need_whitespace = false;
                }
                '\n' | '\r' => {
                    self.input.lookahead(2);
                    self.skip_linebreak();
                    if self.flow_level == 0 {
                        self.allow_simple_key();
                    }
                    need_whitespace = false;
                }
                '#' => {
                    let comment_length = self.input.skip_while_non_breakz();
                    self.mark.offsets.chars += comment_length;
                    self.mark.col += comment_length;
                    self.mark.offsets.bytes = self.input.byte_offset();
                }
                _ => break,
            }
        }

        if need_whitespace {
            Err(ScanError::new_str(self.mark(), "expected whitespace"))
        } else {
            Ok(())
        }
    }

    fn skip_ws_to_eol(&mut self, skip_tabs: SkipTabs) -> Result<SkipTabs, ScanError> {
        let (n_bytes, result) = self.input.skip_ws_to_eol(skip_tabs);
        self.mark.col += n_bytes;
        self.mark.offsets.chars += n_bytes;
        self.mark.offsets.bytes = self.input.byte_offset();
        result.map_err(|msg| ScanError::new_str(self.mark, msg))
    }

    fn fetch_stream_start(&mut self) {
        let mark = self.mark;
        self.indent = -1;
        self.stream_start_produced = true;
        self.allow_simple_key();
        self.tokens.push_back(Token(
            Span::empty(mark),
            TokenType::StreamStart(TEncoding::Utf8),
        ));
        self.simple_keys.push(SimpleKey::new(Marker::new(0, 0, 0)));
    }

    fn fetch_stream_end(&mut self) -> ScanResult {
        // force new line
        if self.mark.col != 0 {
            self.mark.col = 0;
            self.mark.line += 1;
        }

        if let Some((mark, bracket)) = self.flow_markers.pop() {
            return Err(Self::unclosed_bracket(mark, bracket));
        }

        // If the stream ended, we won't have more context. We can stall all the simple keys we
        // had. If one was required, however, that was an error and we must propagate it.
        for sk in &mut self.simple_keys {
            if sk.required && sk.possible {
                return Err(self.simple_key_expected());
            }
            sk.possible = false;
        }

        self.unroll_indent(-1);
        self.remove_simple_key()?;
        self.disallow_simple_key();

        self.tokens
            .push_back(Token(Span::empty(self.mark), TokenType::StreamEnd));
        Ok(())
    }

    fn fetch_directive(&mut self) -> ScanResult {
        self.unroll_indent(-1);
        self.remove_simple_key()?;

        self.disallow_simple_key();

        let tok = self.scan_directive()?;
        self.tokens.push_back(tok);

        Ok(())
    }

    fn scan_directive(&mut self) -> Result<Token<'input>, ScanError> {
        let start_mark = self.mark;
        self.skip_non_blank();

        let name = self.scan_directive_name()?;
        let tok = match name.as_ref() {
            "YAML" => self.scan_version_directive_value(&start_mark)?,
            "TAG" => self.scan_tag_directive_value(&start_mark)?,
            _ => {
                let mut params = Vec::new();
                while self.input.next_is_blank() {
                    let n_blanks = self.input.skip_while_blank();
                    self.mark.offsets.chars += n_blanks;
                    self.mark.col += n_blanks;
                    self.mark.offsets.bytes = self.input.byte_offset();

                    if !is_blank_or_breakz(self.input.peek()) {
                        let mut param = String::new();
                        let n_chars = self.input.fetch_while_is_yaml_non_space(&mut param);
                        self.mark.offsets.chars += n_chars;
                        self.mark.col += n_chars;
                        self.mark.offsets.bytes = self.input.byte_offset();
                        params.push(param);
                    }
                }

                Token(
                    Span::new(start_mark, self.mark),
                    TokenType::ReservedDirective(name, params),
                )
            }
        };

        self.skip_ws_to_eol(SkipTabs::Yes)?;

        if self.input.next_is_breakz() {
            self.input.lookahead(2);
            self.skip_linebreak();
            Ok(tok)
        } else {
            Err(ScanError::new_str(
                start_mark,
                "while scanning a directive, did not find expected comment or line break",
            ))
        }
    }

    fn scan_version_directive_value(&mut self, mark: &Marker) -> Result<Token<'input>, ScanError> {
        let n_blanks = self.input.skip_while_blank();
        self.mark.offsets.chars += n_blanks;
        self.mark.col += n_blanks;
        self.mark.offsets.bytes = self.input.byte_offset();

        let major = self.scan_version_directive_number(mark)?;

        if self.input.peek() != '.' {
            return Err(ScanError::new_str(
                *mark,
                "while scanning a YAML directive, did not find expected digit or '.' character",
            ));
        }
        self.skip_non_blank();

        let minor = self.scan_version_directive_number(mark)?;

        Ok(Token(
            Span::new(*mark, self.mark),
            TokenType::VersionDirective(major, minor),
        ))
    }

    fn scan_directive_name(&mut self) -> Result<String, ScanError> {
        let start_mark = self.mark;
        let mut string = String::new();

        let n_chars = self.input.fetch_while_is_yaml_non_space(&mut string);
        self.mark.offsets.chars += n_chars;
        self.mark.col += n_chars;
        self.mark.offsets.bytes = self.input.byte_offset();

        if string.is_empty() {
            return Err(ScanError::new_str(
                start_mark,
                "while scanning a directive, could not find expected directive name",
            ));
        }

        if !is_blank_or_breakz(self.input.peek()) {
            return Err(ScanError::new_str(
                start_mark,
                "while scanning a directive, found unexpected non-alphabetical character",
            ));
        }

        Ok(string)
    }

    fn scan_version_directive_number(&mut self, mark: &Marker) -> Result<u32, ScanError> {
        let mut val = 0u32;
        let mut length = 0usize;
        while let Some(digit) = self.input.look_ch().to_digit(10) {
            if length + 1 > 9 {
                return Err(ScanError::new_str(
                    *mark,
                    "while scanning a YAML directive, found extremely long version number",
                ));
            }
            length += 1;
            val = val * 10 + digit;
            self.skip_non_blank();
        }

        if length == 0 {
            return Err(ScanError::new_str(
                *mark,
                "while scanning a YAML directive, did not find expected version number",
            ));
        }

        Ok(val)
    }

    fn scan_tag_directive_value(&mut self, mark: &Marker) -> Result<Token<'input>, ScanError> {
        let n_blanks = self.input.skip_while_blank();
        self.mark.offsets.chars += n_blanks;
        self.mark.col += n_blanks;
        self.mark.offsets.bytes = self.input.byte_offset();

        let handle = self.scan_tag_handle_directive_cow(mark)?;

        let n_blanks = self.input.skip_while_blank();
        self.mark.offsets.chars += n_blanks;
        self.mark.col += n_blanks;
        self.mark.offsets.bytes = self.input.byte_offset();

        let prefix = self.scan_tag_prefix_directive_cow(mark)?;

        self.input.lookahead(1);

        if self.input.next_is_blank_or_breakz() {
            Ok(Token(
                Span::new(*mark, self.mark),
                TokenType::TagDirective(handle, prefix),
            ))
        } else {
            Err(ScanError::new_str(
                *mark,
                "while scanning TAG, did not find expected whitespace or line break",
            ))
        }
    }

    fn fetch_tag(&mut self) -> ScanResult {
        self.save_simple_key();
        self.disallow_simple_key();

        let tok = self.scan_tag()?;
        self.tokens.push_back(tok);
        Ok(())
    }

    fn scan_tag(&mut self) -> Result<Token<'input>, ScanError> {
        let start_mark = self.mark;

        // Check if the tag is in the canonical form (verbatim).
        self.input.lookahead(2);

        // If byte_offset is not available, use the original owned-only path.
        if self.input.byte_offset().is_none() {
            return self.scan_tag_owned(&start_mark);
        }

        let (handle, suffix): (Cow<'input, str>, Cow<'input, str>) =
            if self.input.nth_char_is(1, '<') {
                // Verbatim tags always need owned strings (URI escapes).
                let suffix = self.scan_verbatim_tag(&start_mark)?;
                (Cow::Owned(String::new()), Cow::Owned(suffix))
            } else {
                // The tag has either the '!suffix' or the '!handle!suffix'
                let handle = self.scan_tag_handle_cow(&start_mark)?;
                // Check if it is, indeed, handle.
                if handle.len() >= 2 && handle.starts_with('!') && handle.ends_with('!') {
                    // A tag handle starting with "!!" is a secondary tag handle.
                    let suffix = self.scan_tag_shorthand_suffix_cow(&start_mark, true)?;
                    (handle, suffix)
                } else {
                    // Not a real handle, it's part of the suffix.
                    // E.g., "!foo" -> handle="!", suffix="foo"
                    // The "handle" we scanned is actually "!" + suffix_part1.
                    // We need to also scan any remaining suffix characters.
                    let remaining_suffix = self.scan_tag_shorthand_suffix_cow(&start_mark, false)?;

                    // Extract suffix from handle (skip leading '!') and combine with remaining.
                    let suffix = if handle.len() > 1 {
                        if remaining_suffix.is_empty() {
                            // The suffix is just what's in handle after '!'
                            match handle {
                                Cow::Borrowed(s) => Cow::Borrowed(&s[1..]),
                                Cow::Owned(s) => Cow::Owned(s[1..].to_owned()),
                            }
                        } else {
                            // Combine handle (minus leading '!') with remaining suffix.
                            let mut combined = handle[1..].to_owned();
                            combined.push_str(&remaining_suffix);
                            Cow::Owned(combined)
                        }
                    } else {
                        // handle is just "!", suffix is whatever we scanned after
                        remaining_suffix
                    };

                    // A special case: the '!' tag.  Set the handle to '' and the
                    // suffix to '!'.
                    if suffix.is_empty() {
                        (Cow::Borrowed(""), Cow::Borrowed("!"))
                    } else {
                        (Cow::Borrowed("!"), suffix)
                    }
                }
            };

        if is_blank_or_breakz(self.input.look_ch())
            || (self.flow_level > 0 && self.input.next_is_flow())
        {
            // XXX: ex 7.2, an empty scalar can follow a secondary tag
            Ok(Token(
                Span::new(start_mark, self.mark),
                TokenType::Tag(handle, suffix),
            ))
        } else {
            Err(ScanError::new_str(
                start_mark,
                "while scanning a tag, did not find expected whitespace or line break",
            ))
        }
    }

    /// Original owned-only tag scanning path for inputs without `byte_offset` support.
    fn scan_tag_owned(&mut self, start_mark: &Marker) -> Result<Token<'input>, ScanError> {
        let mut handle = String::new();
        let mut suffix;

        if self.input.nth_char_is(1, '<') {
            suffix = self.scan_verbatim_tag(start_mark)?;
        } else {
            // The tag has either the '!suffix' or the '!handle!suffix'
            handle = self.scan_tag_handle(false, start_mark)?;
            // Check if it is, indeed, handle.
            if handle.len() >= 2 && handle.starts_with('!') && handle.ends_with('!') {
                // A tag handle starting with "!!" is a secondary tag handle.
                let is_secondary_handle = handle == "!!";
                suffix =
                    self.scan_tag_shorthand_suffix(false, is_secondary_handle, "", start_mark)?;
            } else {
                suffix = self.scan_tag_shorthand_suffix(false, false, &handle, start_mark)?;
                "!".clone_into(&mut handle);
                // A special case: the '!' tag.  Set the handle to '' and the
                // suffix to '!'.
                if suffix.is_empty() {
                    handle.clear();
                    "!".clone_into(&mut suffix);
                }
            }
        }

        if is_blank_or_breakz(self.input.look_ch())
            || (self.flow_level > 0 && self.input.next_is_flow())
        {
            // XXX: ex 7.2, an empty scalar can follow a secondary tag
            Ok(Token(
                Span::new(*start_mark, self.mark),
                TokenType::Tag(handle.into(), suffix.into()),
            ))
        } else {
            Err(ScanError::new_str(
                *start_mark,
                "while scanning a tag, did not find expected whitespace or line break",
            ))
        }
    }

    /// Scan a tag handle as a `Cow<str>`, borrowing when possible.
    ///
    /// Tag handles are of the form `!`, `!!`, or `!name!` where name is ASCII alphanumeric.
    /// Since they contain no escape sequences, they can always be borrowed from `StrInput`.
    fn scan_tag_handle_cow(&mut self, mark: &Marker) -> Result<Cow<'input, str>, ScanError> {
        let Some(start) = self.input.byte_offset() else {
            return Ok(Cow::Owned(self.scan_tag_handle(false, mark)?));
        };

        if self.input.look_ch() != '!' {
            return Err(ScanError::new_str(
                *mark,
                "while scanning a tag, did not find expected '!'",
            ));
        }

        // Consume the leading '!'.
        self.skip_non_blank();

        // Consume ns-word-char (ASCII alphanumeric, '_' or '-') characters.
        self.input.lookahead(1);
        while self.input.next_is_alpha() {
            self.skip_non_blank();
            self.input.lookahead(1);
        }

        // Optional trailing '!'.
        if self.input.peek() == '!' {
            self.skip_non_blank();
        }

        let Some(end) = self.input.byte_offset() else {
            return Ok(Cow::Owned(self.scan_tag_handle(false, mark)?));
        };

        if let Some(slice) = self.try_borrow_slice(start, end) {
            Ok(Cow::Borrowed(slice))
        } else {
            let slice = self.input.slice_bytes(start, end).ok_or_else(|| {
                ScanError::new_str(
                    *mark,
                    "internal error: input advertised slicing but did not provide a slice",
                )
            })?;
            Ok(Cow::Owned(slice.to_owned()))
        }
    }

    /// Scan a tag shorthand suffix as a `Cow<str>`, borrowing when possible.
    ///
    /// The suffix can be borrowed only if no `%` URI escape sequences are present.
    fn scan_tag_shorthand_suffix_cow(
        &mut self,
        mark: &Marker,
        require_non_empty: bool,
    ) -> Result<Cow<'input, str>, ScanError> {
        let Some(start) = self.input.byte_offset() else {
            return Ok(Cow::Owned(
                self.scan_tag_shorthand_suffix(false, false, "", mark)?,
            ));
        };

        // Scan tag characters, checking for URI escapes.
        while is_tag_char(self.input.look_ch()) {
            if self.input.peek() == '%' {
                // URI escape found - must decode, so fall back to owned path.
                let current = self
                    .input
                    .byte_offset()
                    .expect("byte_offset() must remain available once enabled");
                let mut out = if let Some(slice) = self.input.slice_bytes(start, current) {
                    slice.to_owned()
                } else {
                    String::new()
                };

                // Continue scanning with owned buffer.
                while is_tag_char(self.input.look_ch()) {
                    if self.input.peek() == '%' {
                        out.push(self.scan_uri_escapes(mark)?);
                    } else {
                        out.push(self.input.peek());
                        self.skip_non_blank();
                    }
                }
                return Ok(Cow::Owned(out));
            }
            self.skip_non_blank();
        }

        let Some(end) = self.input.byte_offset() else {
            return Ok(Cow::Owned(
                self.scan_tag_shorthand_suffix(false, false, "", mark)?,
            ));
        };

        if require_non_empty && start == end {
            return Err(ScanError::new_str(
                *mark,
                "while parsing a tag, did not find expected tag URI",
            ));
        }

        if let Some(slice) = self.try_borrow_slice(start, end) {
            Ok(Cow::Borrowed(slice))
        } else {
            let slice = self.input.slice_bytes(start, end).ok_or_else(|| {
                ScanError::new_str(
                    *mark,
                    "internal error: input advertised slicing but did not provide a slice",
                )
            })?;
            Ok(Cow::Owned(slice.to_owned()))
        }
    }

    fn scan_tag_handle(&mut self, directive: bool, mark: &Marker) -> Result<String, ScanError> {
        let mut string = String::new();
        if self.input.look_ch() != '!' {
            return Err(ScanError::new_str(
                *mark,
                "while scanning a tag, did not find expected '!'",
            ));
        }

        string.push(self.input.peek());
        self.skip_non_blank();

        let n_chars = self.input.fetch_while_is_alpha(&mut string);
        self.mark.offsets.chars += n_chars;
        self.mark.col += n_chars;
        self.mark.offsets.bytes = self.input.byte_offset();

        // Check if the trailing character is '!' and copy it.
        if self.input.peek() == '!' {
            string.push(self.input.peek());
            self.skip_non_blank();
        } else if directive && string != "!" {
            // It's either the '!' tag or not really a tag handle.  If it's a %TAG
            // directive, it's an error.  If it's a tag token, it must be a part of
            // URI.
            return Err(ScanError::new_str(
                *mark,
                "while parsing a tag directive, did not find expected '!'",
            ));
        }
        Ok(string)
    }

    /// Scan for a tag prefix (6.8.2.2).
    ///
    /// There are 2 kinds of tag prefixes:
    ///   - Local: Starts with a `!`, contains only URI chars (`!foo`)
    ///   - Global: Starts with a tag char, contains then URI chars (`!foo,2000:app/`)
    fn scan_tag_prefix(&mut self, start_mark: &Marker) -> Result<String, ScanError> {
        let mut string = String::new();

        if self.input.look_ch() == '!' {
            // If we have a local tag, insert and skip `!`.
            string.push(self.input.peek());
            self.skip_non_blank();
        } else if !is_tag_char(self.input.peek()) {
            // Otherwise, check if the first global tag character is valid.
            return Err(ScanError::new_str(
                *start_mark,
                "invalid global tag character",
            ));
        } else if self.input.peek() == '%' {
            // If it is valid and an escape sequence, escape it.
            string.push(self.scan_uri_escapes(start_mark)?);
        } else {
            // Otherwise, push the first character.
            string.push(self.input.peek());
            self.skip_non_blank();
        }

        while is_uri_char(self.input.look_ch()) {
            if self.input.peek() == '%' {
                string.push(self.scan_uri_escapes(start_mark)?);
            } else {
                string.push(self.input.peek());
                self.skip_non_blank();
            }
        }

        Ok(string)
    }

    /// Scan for a verbatim tag.
    ///
    /// The prefixing `!<` must _not_ have been skipped.
    fn scan_verbatim_tag(&mut self, start_mark: &Marker) -> Result<String, ScanError> {
        // Eat `!<`
        self.skip_non_blank();
        self.skip_non_blank();

        let mut string = String::new();
        while is_uri_char(self.input.look_ch()) {
            if self.input.peek() == '%' {
                string.push(self.scan_uri_escapes(start_mark)?);
            } else {
                string.push(self.input.peek());
                self.skip_non_blank();
            }
        }

        if self.input.peek() != '>' {
            return Err(ScanError::new_str(
                *start_mark,
                "while scanning a verbatim tag, did not find the expected '>'",
            ));
        }
        self.skip_non_blank();

        Ok(string)
    }

    fn scan_tag_shorthand_suffix(
        &mut self,
        _directive: bool,
        _is_secondary: bool,
        head: &str,
        mark: &Marker,
    ) -> Result<String, ScanError> {
        let mut length = head.len();
        let mut string = String::new();

        // Copy the head if needed.
        // Note that we don't copy the leading '!' character.
        if length > 1 {
            string.extend(head.chars().skip(1));
        }

        while is_tag_char(self.input.look_ch()) {
            // Check if it is a URI-escape sequence.
            if self.input.peek() == '%' {
                string.push(self.scan_uri_escapes(mark)?);
            } else {
                string.push(self.input.peek());
                self.skip_non_blank();
            }

            length += 1;
        }

        if length == 0 {
            return Err(ScanError::new_str(
                *mark,
                "while parsing a tag, did not find expected tag URI",
            ));
        }

        Ok(string)
    }

    fn scan_uri_escapes(&mut self, mark: &Marker) -> Result<char, ScanError> {
        let mut width = 0usize;
        let mut code = 0u32;
        loop {
            self.input.lookahead(3);

            let c = self.input.peek_nth(1);
            let nc = self.input.peek_nth(2);

            if !(self.input.peek() == '%' && is_hex(c) && is_hex(nc)) {
                return Err(ScanError::new_str(
                    *mark,
                    "while parsing a tag, found an invalid escape sequence",
                ));
            }

            let byte = (as_hex(c) << 4) + as_hex(nc);
            if width == 0 {
                width = match byte {
                    _ if byte & 0x80 == 0x00 => 1,
                    _ if byte & 0xE0 == 0xC0 => 2,
                    _ if byte & 0xF0 == 0xE0 => 3,
                    _ if byte & 0xF8 == 0xF0 => 4,
                    _ => {
                        return Err(ScanError::new_str(
                            *mark,
                            "while parsing a tag, found an incorrect leading UTF-8 byte",
                        ));
                    }
                };
                code = byte;
            } else {
                if byte & 0xc0 != 0x80 {
                    return Err(ScanError::new_str(
                        *mark,
                        "while parsing a tag, found an incorrect trailing UTF-8 byte",
                    ));
                }
                code = (code << 8) + byte;
            }

            self.skip_n_non_blank(3);

            width -= 1;
            if width == 0 {
                break;
            }
        }

        match char::from_u32(code) {
            Some(ch) => Ok(ch),
            None => Err(ScanError::new_str(
                *mark,
                "while parsing a tag, found an invalid UTF-8 codepoint",
            )),
        }
    }

    fn fetch_anchor(&mut self, alias: bool) -> ScanResult {
        self.save_simple_key();
        self.disallow_simple_key();

        let tok = self.scan_anchor(alias)?;

        self.tokens.push_back(tok);

        Ok(())
    }

    fn scan_anchor(&mut self, alias: bool) -> Result<Token<'input>, ScanError> {
        let start_mark = self.mark;

        // Skip `&` / `*`.
        self.skip_non_blank();

        // Borrow from input when possible.
        if let Some(start) = self.input.byte_offset() {
            while is_anchor_char(self.input.look_ch()) {
                self.skip_non_blank();
            }

            let end = self
                .input
                .byte_offset()
                .expect("byte_offset() must remain available once enabled");

            if start == end {
                return Err(ScanError::new_str(start_mark, "while scanning an anchor or alias, did not find expected alphabetic or numeric character"));
            }

            let cow = if let Some(slice) = self.try_borrow_slice(start, end) {
                Cow::Borrowed(slice)
            } else if let Some(slice) = self.input.slice_bytes(start, end) {
                Cow::Owned(slice.to_owned())
            } else {
                return Err(ScanError::new_str(
                    start_mark,
                    "internal error: input advertised slicing but did not provide a slice",
                ));
            };

            let tok = if alias {
                TokenType::Alias(cow)
            } else {
                TokenType::Anchor(cow)
            };
            return Ok(Token(Span::new(start_mark, self.mark), tok));
        }

        let mut string = String::new();
        while is_anchor_char(self.input.look_ch()) {
            string.push(self.input.peek());
            self.skip_non_blank();
        }

        if string.is_empty() {
            return Err(ScanError::new_str(start_mark, "while scanning an anchor or alias, did not find expected alphabetic or numeric character"));
        }

        let tok = if alias {
            TokenType::Alias(string.into())
        } else {
            TokenType::Anchor(string.into())
        };
        Ok(Token(Span::new(start_mark, self.mark), tok))
    }

    fn fetch_flow_collection_start(&mut self, tok: TokenType<'input>) -> ScanResult {
        // The indicators '[' and '{' may start a simple key.
        self.save_simple_key();

        let start_mark = self.mark;
        let indicator = self.input.peek();
        self.flow_markers.push((start_mark, indicator));

        self.roll_one_col_indent();
        self.increase_flow_level()?;

        self.allow_simple_key();

        self.skip_non_blank();

        if tok == TokenType::FlowMappingStart {
            self.flow_mapping_started = true;
        } else {
            self.implicit_flow_mapping_states
                .push(ImplicitMappingState::Possible);
        }

        self.skip_ws_to_eol(SkipTabs::Yes)?;

        self.tokens
            .push_back(Token(Span::new(start_mark, self.mark), tok));
        Ok(())
    }

    fn fetch_flow_collection_end(&mut self, tok: TokenType<'input>) -> ScanResult {
        // A closing bracket without a corresponding opening is invalid YAML.
        if self.flow_level == 0 {
            return Err(ScanError::new_str(self.mark, "misplaced bracket"));
        }

        let Some((open_mark, open_ch)) = self.flow_markers.pop() else {
            return Err(ScanError::new_str(self.mark, "misplaced bracket"));
        };

        let (expected_open, actual_close) = match tok {
            TokenType::FlowSequenceEnd => ('[', ']'),
            TokenType::FlowMappingEnd => ('{', '}'),
            _ => unreachable!("flow collection end called with non-closing token"),
        };

        if open_ch != expected_open {
            return Err(ScanError::new(
                open_mark,
                format!("mismatched bracket '{open_ch}' closed by '{actual_close}'"),
            ));
        }

        let flow_level = self.flow_level;

        self.remove_simple_key()?;

        if matches!(tok, TokenType::FlowSequenceEnd) {
            self.end_implicit_mapping(self.mark, flow_level);
            // We are out exiting the flow sequence, nesting goes down 1 level.
            self.implicit_flow_mapping_states.pop();
        }

        self.decrease_flow_level();

        self.disallow_simple_key();

        let start_mark = self.mark;
        self.skip_non_blank();
        self.skip_ws_to_eol(SkipTabs::Yes)?;

        // A flow collection within a flow mapping can be a key. In that case, the value may be
        // adjacent to the `:`.
        // ```yaml
        // - [ {a: b}:value ]
        // ```
        if self.flow_level > 0 {
            self.adjacent_value_allowed_at = self.mark.index();
        }

        self.tokens
            .push_back(Token(Span::new(start_mark, self.mark), tok));
        Ok(())
    }

    /// Push the `FlowEntry` token and skip over the `,`.
    fn fetch_flow_entry(&mut self) -> ScanResult {
        self.remove_simple_key()?;
        self.allow_simple_key();

        self.end_implicit_mapping(self.mark, self.flow_level);

        let start_mark = self.mark;
        self.skip_non_blank();
        self.skip_ws_to_eol(SkipTabs::Yes)?;

        self.tokens.push_back(Token(
            Span::new(start_mark, self.mark),
            TokenType::FlowEntry,
        ));
        Ok(())
    }

    fn increase_flow_level(&mut self) -> ScanResult {
        self.simple_keys.push(SimpleKey::new(Marker::new(0, 0, 0)));
        self.flow_level = self
            .flow_level
            .checked_add(1)
            .ok_or_else(|| ScanError::new_str(self.mark, "recursion limit exceeded"))?;
        Ok(())
    }

    fn decrease_flow_level(&mut self) {
        if self.flow_level > 0 {
            self.flow_level -= 1;
            self.simple_keys.pop().unwrap();
        }
    }

    /// Push the `Block*` token(s) and skip over the `-`.
    ///
    /// Add an indentation level and push a `BlockSequenceStart` token if needed, then push a
    /// `BlockEntry` token.
    /// This function only skips over the `-` and does not fetch the entry value.
    fn fetch_block_entry(&mut self) -> ScanResult {
        if self.flow_level > 0 {
            // - * only allowed in block
            return Err(ScanError::new_str(
                self.mark,
                r#""-" is only valid inside a block"#,
            ));
        }
        // Check if we are allowed to start a new entry.
        if !self.simple_key_allowed {
            return Err(ScanError::new_str(
                self.mark,
                "block sequence entries are not allowed in this context",
            ));
        }

        // ???, fixes test G9HC.
        if let Some(Token(span, TokenType::Anchor(..) | TokenType::Tag(..))) = self.tokens.back() {
            if self.mark.col == 0 && span.start.col == 0 && self.indent > -1 {
                return Err(ScanError::new_str(
                    span.start,
                    "invalid indentation for anchor",
                ));
            }
        }

        // Skip over the `-`.
        let mark = self.mark;
        self.skip_non_blank();

        // generate BLOCK-SEQUENCE-START if indented
        self.roll_indent(mark.col, None, TokenType::BlockSequenceStart, mark);
        let found_tabs = self.skip_ws_to_eol(SkipTabs::Yes)?.found_tabs();
        self.input.lookahead(2);
        if found_tabs && self.input.next_char_is('-') && is_blank_or_breakz(self.input.peek_nth(1))
        {
            return Err(ScanError::new_str(
                self.mark,
                "'-' must be followed by a valid YAML whitespace",
            ));
        }

        self.skip_ws_to_eol(SkipTabs::No)?;
        self.input.lookahead(1);
        if self.input.next_is_break() || self.input.next_is_flow() {
            self.roll_one_col_indent();
        }

        self.remove_simple_key()?;
        self.allow_simple_key();

        self.tokens
            .push_back(Token(Span::empty(self.mark), TokenType::BlockEntry));

        Ok(())
    }

    fn fetch_document_indicator(&mut self, t: TokenType<'input>) -> ScanResult {
        if let Some((mark, bracket)) = self.flow_markers.pop() {
            return Err(ScanError::new(
                mark,
                format!("unclosed bracket '{bracket}'"),
            ));
        }

        self.unroll_indent(-1);
        self.remove_simple_key()?;
        self.disallow_simple_key();

        let mark = self.mark;

        self.skip_n_non_blank(3);

        self.tokens.push_back(Token(Span::new(mark, self.mark), t));
        Ok(())
    }

    fn fetch_block_scalar(&mut self, literal: bool) -> ScanResult {
        self.save_simple_key();
        self.allow_simple_key();
        let tok = self.scan_block_scalar(literal)?;

        self.tokens.push_back(tok);
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn scan_block_scalar(&mut self, literal: bool) -> Result<Token<'input>, ScanError> {
        let start_mark = self.mark;
        let mut chomping = Chomping::Clip;
        let mut increment: usize = 0;
        let mut indent: usize = 0;
        let mut trailing_blank: bool;
        let mut leading_blank: bool = false;
        let style = if literal {
            ScalarStyle::Literal
        } else {
            ScalarStyle::Folded
        };

        let mut string = String::new();
        let mut leading_break = String::new();
        let mut trailing_breaks = String::new();
        let mut chomping_break = String::new();

        // skip '|' or '>'
        self.skip_non_blank();
        self.unroll_non_block_indents();

        if self.input.look_ch() == '+' || self.input.peek() == '-' {
            if self.input.peek() == '+' {
                chomping = Chomping::Keep;
            } else {
                chomping = Chomping::Strip;
            }
            self.skip_non_blank();
            self.input.lookahead(1);
            if self.input.next_is_digit() {
                if self.input.peek() == '0' {
                    return Err(ScanError::new_str(
                        start_mark,
                        "while scanning a block scalar, found an indentation indicator equal to 0",
                    ));
                }
                increment = (self.input.peek() as usize) - ('0' as usize);
                self.skip_non_blank();
            }
        } else if self.input.next_is_digit() {
            if self.input.peek() == '0' {
                return Err(ScanError::new_str(
                    start_mark,
                    "while scanning a block scalar, found an indentation indicator equal to 0",
                ));
            }

            increment = (self.input.peek() as usize) - ('0' as usize);
            self.skip_non_blank();
            self.input.lookahead(1);
            if self.input.peek() == '+' || self.input.peek() == '-' {
                if self.input.peek() == '+' {
                    chomping = Chomping::Keep;
                } else {
                    chomping = Chomping::Strip;
                }
                self.skip_non_blank();
            }
        }

        self.skip_ws_to_eol(SkipTabs::Yes)?;

        // Check if we are at the end of the line.
        self.input.lookahead(1);
        if !self.input.next_is_breakz() {
            return Err(ScanError::new_str(
                start_mark,
                "while scanning a block scalar, did not find expected comment or line break",
            ));
        }

        if self.input.next_is_break() {
            self.input.lookahead(2);
            self.read_break(&mut chomping_break);
        }

        if self.input.look_ch() == '\t' {
            return Err(ScanError::new_str(
                start_mark,
                "a block scalar content cannot start with a tab",
            ));
        }

        if increment > 0 {
            indent = if self.indent >= 0 {
                (self.indent + increment as isize) as usize
            } else {
                increment
            }
        }

        // Scan the leading line breaks and determine the indentation level if needed.
        if indent == 0 {
            self.skip_block_scalar_first_line_indent(&mut indent, &mut trailing_breaks);
        } else {
            self.skip_block_scalar_indent(indent, &mut trailing_breaks);
        }

        // We have an end-of-stream with no content, e.g.:
        // ```yaml
        // - |+
        // ```
        if self.input.next_is_z() {
            let contents = match chomping {
                // We strip trailing linebreaks. Nothing remain.
                Chomping::Strip => String::new(),
                // There was no newline after the chomping indicator.
                _ if self.mark.line == start_mark.line() => String::new(),
                // We clip lines, and there was a newline after the chomping indicator.
                // All other breaks are ignored.
                Chomping::Clip => chomping_break,
                // We keep lines. There was a newline after the chomping indicator but nothing
                // else.
                Chomping::Keep if trailing_breaks.is_empty() => chomping_break,
                // Otherwise, the newline after chomping is ignored.
                Chomping::Keep => trailing_breaks,
            };
            return Ok(Token(
                Span::new(start_mark, self.mark),
                TokenType::Scalar(style, contents.into()),
            ));
        }

        if self.mark.col < indent && (self.mark.col as isize) > self.indent {
            return Err(ScanError::new_str(
                self.mark,
                "wrongly indented line in block scalar",
            ));
        }

        let mut line_buffer = String::with_capacity(100);
        let start_mark = self.mark;
        while self.mark.col == indent && !self.input.next_is_z() {
            if indent == 0 {
                self.input.lookahead(4);
                if self.input.next_is_document_end() {
                    break;
                }
            }

            // We are at the first content character of a content line.
            trailing_blank = self.input.next_is_blank();
            if !literal && !leading_break.is_empty() && !leading_blank && !trailing_blank {
                string.push_str(&trailing_breaks);
                if trailing_breaks.is_empty() {
                    string.push(' ');
                }
            } else {
                string.push_str(&leading_break);
                string.push_str(&trailing_breaks);
            }

            leading_break.clear();
            trailing_breaks.clear();

            leading_blank = self.input.next_is_blank();

            self.scan_block_scalar_content_line(&mut string, &mut line_buffer);

            // break on EOF
            self.input.lookahead(2);
            if self.input.next_is_z() {
                break;
            }

            self.read_break(&mut leading_break);

            // Eat the following indentation spaces and line breaks.
            self.skip_block_scalar_indent(indent, &mut trailing_breaks);
        }

        // Chomp the tail.
        if chomping != Chomping::Strip {
            string.push_str(&leading_break);
            // If we had reached an eof but the last character wasn't an end-of-line, check if the
            // last line was indented at least as the rest of the scalar, then we need to consider
            // there is a newline.
            if self.input.next_is_z() && self.mark.col >= indent.max(1) {
                string.push('\n');
            }
        }

        if chomping == Chomping::Keep {
            string.push_str(&trailing_breaks);
        }

        Ok(Token(
            Span::new(start_mark, self.mark),
            TokenType::Scalar(style, string.into()),
        ))
    }

    /// Retrieve the contents of the line, parsing it as a block scalar.
    ///
    /// The contents will be appended to `string`. `line_buffer` is used as a temporary buffer to
    /// store bytes before pushing them to `string` and thus avoiding reallocating more than
    /// necessary. `line_buffer` is assumed to be empty upon calling this function. It will be
    /// `clear`ed before the end of the function.
    ///
    /// This function assumed the first character to read is the first content character in the
    /// line. This function does not consume the line break character(s) after the line.
    fn scan_block_scalar_content_line(&mut self, string: &mut String, line_buffer: &mut String) {
        // Start by evaluating characters in the buffer.
        while !self.input.buf_is_empty() && !self.input.next_is_breakz() {
            string.push(self.input.peek());
            // We may technically skip non-blank characters. However, the only distinction is
            // to determine what is leading whitespace and what is not. Here, we read the
            // contents of the line until either eof or a linebreak. We know we will not read
            // `self.leading_whitespace` until the end of the line, where it will be reset.
            // This allows us to call a slightly less expensive function.
            self.skip_blank();
        }

        // All characters that were in the buffer were consumed. We need to check if more
        // follow.
        if self.input.buf_is_empty() {
            // We will read all consecutive non-breakz characters. We push them into a
            // temporary buffer. The main difference with going through `self.buffer` is that
            // characters are appended here as their real size (1B for ascii, or up to 4 bytes for
            // UTF-8). We can then use the internal `line_buffer` `Vec` to push data into `string`
            // (using `String::push_str`).

            // line_buffer is empty at this point so we can compute n_chars here as well
            let mut n_chars = 0;
            debug_assert!(line_buffer.is_empty());
            while let Some(c) = self.input.raw_read_non_breakz_ch() {
                line_buffer.push(c);
                n_chars += 1;
            }

            // We need to manually update our position; we haven't called a `skip` function.
            self.mark.col += n_chars;
            self.mark.offsets.chars += n_chars;
            self.mark.offsets.bytes = self.input.byte_offset();

            // We can now append our bytes to our `string`.
            string.reserve(line_buffer.len());
            string.push_str(line_buffer);
            // This clears the _contents_ without touching the _capacity_.
            line_buffer.clear();
        }
    }

    /// Skip the block scalar indentation and empty lines.
    fn skip_block_scalar_indent(&mut self, indent: usize, breaks: &mut String) {
        loop {
            // Consume all spaces. Tabs cannot be used as indentation.
            if indent < self.input.bufmaxlen() - 2 {
                self.input.lookahead(self.input.bufmaxlen());
                while self.mark.col < indent && self.input.peek() == ' ' {
                    self.skip_blank();
                }
            } else {
                loop {
                    self.input.lookahead(self.input.bufmaxlen());
                    while !self.input.buf_is_empty()
                        && self.mark.col < indent
                        && self.input.peek() == ' '
                    {
                        self.skip_blank();
                    }
                    // If we reached our indent, we can break. We must also break if we have
                    // reached content or EOF; that is, the buffer is not empty and the next
                    // character is not a space.
                    if self.mark.col == indent
                        || (!self.input.buf_is_empty() && self.input.peek() != ' ')
                    {
                        break;
                    }
                }
                self.input.lookahead(2);
            }

            // If our current line is empty, skip over the break and continue looping.
            if self.input.next_is_break() {
                self.read_break(breaks);
            } else {
                // Otherwise, we have a content line. Return control.
                break;
            }
        }
    }

    /// Determine the indentation level for a block scalar from the first line of its contents.
    ///
    /// The function skips over whitespace-only lines and sets `indent` to the the longest
    /// whitespace line that was encountered.
    fn skip_block_scalar_first_line_indent(&mut self, indent: &mut usize, breaks: &mut String) {
        let mut max_indent = 0;
        loop {
            // Consume all spaces. Tabs cannot be used as indentation.
            while self.input.look_ch() == ' ' {
                self.skip_blank();
            }

            if self.mark.col > max_indent {
                max_indent = self.mark.col;
            }

            if self.input.next_is_break() {
                // If our current line is empty, skip over the break and continue looping.
                self.input.lookahead(2);
                self.read_break(breaks);
            } else {
                // Otherwise, we have a content line. Return control.
                break;
            }
        }

        // In case a yaml looks like:
        // ```yaml
        // |
        // foo
        // bar
        // ```
        // We need to set the indent to 0 and not 1. In all other cases, the indent must be at
        // least 1. When in the above example, `self.indent` will be set to -1.
        *indent = max_indent.max((self.indent + 1) as usize);
        if self.indent > 0 {
            *indent = (*indent).max(1);
        }
    }

    fn fetch_flow_scalar(&mut self, single: bool) -> ScanResult {
        self.save_simple_key();
        self.disallow_simple_key();

        let tok = self.scan_flow_scalar(single)?;

        // From spec: To ensure JSON compatibility, if a key inside a flow mapping is JSON-like,
        // YAML allows the following value to be specified adjacent to the “:”.
        self.skip_to_next_token()?;
        self.adjacent_value_allowed_at = self.mark.index();

        self.tokens.push_back(tok);
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn scan_flow_scalar(&mut self, single: bool) -> Result<Token<'input>, ScanError> {
        let start_mark = self.mark;

        // Output scalar contents.
        let mut buf = match self.input.byte_offset() {
            Some(off) => FlowScalarBuf::new_borrowed(off + self.input.peek().len_utf8()),
            None => FlowScalarBuf::new_owned(),
        };

        // Scratch used to consume the *first* line break in a break run without emitting it.
        // (The first break folds to ' ' or to nothing depending on escaping rules.)
        let mut break_scratch = String::new();

        /* Eat the left quote. */
        self.skip_non_blank();

        loop {
            /* Check for a document indicator. */
            self.input.lookahead(4);

            if self.mark.col == 0 && self.input.next_is_document_indicator() {
                return Err(ScanError::new_str(
                    start_mark,
                    "while scanning a quoted scalar, found unexpected document indicator",
                ));
            }

            if self.input.next_is_z() {
                return Err(ScanError::new_str(start_mark, "unclosed quote"));
            }

            // Do not enforce block indentation inside quoted (flow) scalars.
            // YAML allows line breaks within quoted scalars.
            let mut leading_blanks = false;
            self.consume_flow_scalar_non_whitespace_chars(
                single,
                &mut buf,
                &mut leading_blanks,
                &start_mark,
            )?;

            match self.input.look_ch() {
                '\'' if single => break,
                '"' if !single => break,
                _ => {}
            }

            // --- Faster whitespace / line break handling (no temporary Strings) ---
            //
            // Instead of:
            //   - collecting blanks into `whitespaces` and then copying
            //   - collecting breaks into `leading_break` / `trailing_breaks` and then copying
            //
            // We do:
            //   - append trailing blanks directly to `string`, remember where they started,
            //     and truncate them if a line break follows.
            //   - for line breaks: consume the first break into a scratch (discarded),
            //     append subsequent breaks directly to `string`.
            //
            // These flags mirror the old "is_empty()" checks:
            //   has_leading_break  <=> !leading_break.is_empty()
            //   has_trailing_breaks <=> !trailing_breaks.is_empty()
            let mut trailing_ws_start: Option<usize> = None;
            let mut has_leading_break = false;
            let mut has_trailing_breaks = false;

            // For the borrowed path: track the (byte) start of a pending whitespace run.
            let mut pending_ws_start: Option<usize> = None;

            // Consume blank characters.
            while self.input.next_is_blank() || self.input.next_is_break() {
                if self.input.next_is_blank() {
                    // Consume a space or a tab character.
                    if leading_blanks {
                        if self.input.peek() == '\t' && (self.mark.col as isize) < self.indent {
                            return Err(ScanError::new_str(
                                self.mark,
                                "tab cannot be used as indentation",
                            ));
                        }
                        self.skip_blank();
                    } else {
                        // Append to output immediately; if a break appears next, we'll truncate.
                        match buf {
                            FlowScalarBuf::Owned(ref mut string) => {
                                if trailing_ws_start.is_none() {
                                    trailing_ws_start = Some(string.len());
                                }
                                string.push(self.input.peek());
                            }
                            FlowScalarBuf::Borrowed { .. } => {
                                if pending_ws_start.is_none() {
                                    pending_ws_start = self.input.byte_offset();
                                }
                            }
                        }
                        self.skip_blank();

                        if let (FlowScalarBuf::Borrowed { .. }, Some(ws_start), Some(ws_end)) =
                            (&mut buf, pending_ws_start, self.input.byte_offset())
                        {
                            buf.note_pending_ws(ws_start, ws_end);
                        }
                    }
                } else {
                    self.input.lookahead(2);

                    // Check if it is a first line break.
                    if leading_blanks {
                        // Second+ line break in a run: preserve it.
                        match buf {
                            FlowScalarBuf::Owned(ref mut string) => self.read_break(string),
                            FlowScalarBuf::Borrowed { .. } => {
                                self.promote_flow_scalar_buf_to_owned(&start_mark, &mut buf)?;
                                let Some(string) = buf.as_owned_mut() else {
                                    unreachable!()
                                };
                                self.read_break(string);
                            }
                        }
                        has_trailing_breaks = true;
                    } else {
                        // First break: drop any trailing blanks we appended, then consume the break.
                        if let Some(pos) = trailing_ws_start.take() {
                            if let FlowScalarBuf::Owned(ref mut string) = buf {
                                string.truncate(pos);
                            }
                        }

                        if pending_ws_start.take().is_some() {
                            // Trailing blanks before a break are discarded => transformation.
                            if matches!(buf, FlowScalarBuf::Borrowed { .. }) {
                                self.promote_flow_scalar_buf_to_owned(&start_mark, &mut buf)?;
                            }
                            buf.discard_pending_ws();
                        } else {
                            buf.commit_pending_ws();
                        }

                        break_scratch.clear();
                        self.read_break(&mut break_scratch);
                        // Keep `break_scratch` content (ignored) until next clear; no need to clear twice.

                        has_leading_break = true;
                        leading_blanks = true;
                    }
                }

                self.input.lookahead(1);
            }

            // If we had a line break inside a quoted (flow) scalar, validate indentation
            // of the continuation line in block context.
            if leading_blanks && has_leading_break && self.flow_level == 0 {
                let next_ch = self.input.peek();
                let is_closing_quote = (single && next_ch == '\'') || (!single && next_ch == '"');
                if !is_closing_quote && (self.mark.col as isize) <= self.indent {
                    return Err(ScanError::new_str(
                        self.mark,
                        "invalid indentation in multiline quoted scalar",
                    ));
                }
            }

            // Join the whitespaces or fold line breaks.
            if leading_blanks {
                // Old logic:
                //   if leading_break empty => emit trailing_breaks (already emitted now)
                //   else if trailing_breaks empty => emit ' '
                //   else emit trailing_breaks (already emitted now)
                if has_leading_break && !has_trailing_breaks {
                    match buf {
                        FlowScalarBuf::Owned(ref mut string) => string.push(' '),
                        FlowScalarBuf::Borrowed { .. } => {
                            self.promote_flow_scalar_buf_to_owned(&start_mark, &mut buf)?;
                            let Some(string) = buf.as_owned_mut() else {
                                unreachable!()
                            };
                            string.push(' ');
                        }
                    }
                }
            }
            // else: trailing blanks are already appended to `string`
        } // loop

        // Eat the right quote.
        self.skip_non_blank();

        // Ensure there is no invalid trailing content.
        self.skip_ws_to_eol(SkipTabs::Yes)?;
        match self.input.peek() {
            // These can be encountered in flow sequences or mappings.
            ',' | '}' | ']' if self.flow_level > 0 => {}
            // An end-of-line / end-of-stream is fine. No trailing content.
            c if is_breakz(c) => {}
            // ':' can be encountered if our scalar is a key.
            // Outside of flow contexts, keys cannot span multiple lines
            ':' if self.flow_level == 0 && start_mark.line == self.mark.line => {}
            // Inside a flow context, this is allowed.
            ':' if self.flow_level > 0 => {}
            _ => {
                return Err(ScanError::new_str(
                    self.mark,
                    "invalid trailing content after double-quoted scalar",
                ));
            }
        }

        let style = if single {
            ScalarStyle::SingleQuoted
        } else {
            ScalarStyle::DoubleQuoted
        };

        let contents = match buf {
            FlowScalarBuf::Owned(string) => Cow::Owned(string),
            FlowScalarBuf::Borrowed {
                start,
                mut end,
                pending_ws_start,
                pending_ws_end,
            } => {
                // If we ended after a whitespace run, it is part of the output (no break followed).
                if pending_ws_start.is_some() {
                    end = pending_ws_end;
                }
                if let Some(slice) = self.try_borrow_slice(start, end) {
                    Cow::Borrowed(slice)
                } else {
                    let slice = self.input.slice_bytes(start, end).ok_or_else(|| {
                        ScanError::new_str(
                            start_mark,
                            "internal error: input advertised offsets but did not provide a slice",
                        )
                    })?;
                    Cow::Owned(slice.to_owned())
                }
            }
        };

        Ok(Token(
            Span::new(start_mark, self.mark),
            TokenType::Scalar(style, contents),
        ))
    }

    /// Consume successive non-whitespace characters from a flow scalar.
    ///
    /// This function resolves escape sequences and stops upon encountering a whitespace, the end
    /// of the stream or the closing character for the scalar (`'` for single quoted scalars, `"`
    /// for double quoted scalars).
    ///
    /// # Errors
    /// Return an error if an invalid escape sequence is found.
    fn consume_flow_scalar_non_whitespace_chars(
        &mut self,
        single: bool,
        buf: &mut FlowScalarBuf,
        leading_blanks: &mut bool,
        start_mark: &Marker,
    ) -> Result<(), ScanError> {
        self.input.lookahead(2);
        while !is_blank_or_breakz(self.input.peek()) {
            match self.input.peek() {
                // Check for an escaped single quote.
                '\'' if self.input.peek_nth(1) == '\'' && single => {
                    if matches!(buf, FlowScalarBuf::Borrowed { .. }) {
                        buf.commit_pending_ws();
                        self.promote_flow_scalar_buf_to_owned(start_mark, buf)?;
                    }
                    let Some(string) = buf.as_owned_mut() else {
                        unreachable!()
                    };
                    string.push('\'');
                    self.skip_n_non_blank(2);
                }
                // Check for the right quote.
                '\'' if single => break,
                '"' if !single => break,
                // Check for an escaped line break.
                '\\' if !single && is_break(self.input.peek_nth(1)) => {
                    self.input.lookahead(3);
                    if matches!(buf, FlowScalarBuf::Borrowed { .. }) {
                        buf.commit_pending_ws();
                        self.promote_flow_scalar_buf_to_owned(start_mark, buf)?;
                    }
                    self.skip_non_blank();
                    self.skip_linebreak();
                    *leading_blanks = true;
                    break;
                }
                // Check for an escape sequence.
                '\\' if !single => {
                    if matches!(buf, FlowScalarBuf::Borrowed { .. }) {
                        buf.commit_pending_ws();
                        self.promote_flow_scalar_buf_to_owned(start_mark, buf)?;
                    }
                    let Some(string) = buf.as_owned_mut() else {
                        unreachable!()
                    };
                    string.push(self.resolve_flow_scalar_escape_sequence(start_mark)?);
                }
                c => {
                    match buf {
                        FlowScalarBuf::Owned(ref mut string) => {
                            string.push(c);
                        }
                        FlowScalarBuf::Borrowed { .. } => {
                            buf.commit_pending_ws();
                        }
                    }
                    self.skip_non_blank();

                    if let Some(new_end) = self.input.byte_offset() {
                        if let FlowScalarBuf::Borrowed { end, .. } = buf {
                            *end = new_end;
                        }
                    }
                }
            }
            self.input.lookahead(2);
        }
        Ok(())
    }

    /// Escape the sequence we encounter in a flow scalar.
    ///
    /// `self.input.peek()` must point to the `\` starting the escape sequence.
    ///
    /// # Errors
    /// Return an error if an invalid escape sequence is found.
    fn resolve_flow_scalar_escape_sequence(
        &mut self,
        start_mark: &Marker,
    ) -> Result<char, ScanError> {
        let mut code_length = 0usize;
        let mut ret = '\0';

        match self.input.peek_nth(1) {
            '0' => ret = '\0',
            'a' => ret = '\x07',
            'b' => ret = '\x08',
            't' | '\t' => ret = '\t',
            'n' => ret = '\n',
            'v' => ret = '\x0b',
            'f' => ret = '\x0c',
            'r' => ret = '\x0d',
            'e' => ret = '\x1b',
            ' ' => ret = '\x20',
            '"' => ret = '"',
            '/' => ret = '/',
            '\\' => ret = '\\',
            // Unicode next line (#x85)
            'N' => ret = char::from_u32(0x85).unwrap(),
            // Unicode non-breaking space (#xA0)
            '_' => ret = char::from_u32(0xA0).unwrap(),
            // Unicode line separator (#x2028)
            'L' => ret = char::from_u32(0x2028).unwrap(),
            // Unicode paragraph separator (#x2029)
            'P' => ret = char::from_u32(0x2029).unwrap(),
            'x' => code_length = 2,
            'u' => code_length = 4,
            'U' => code_length = 8,
            _ => {
                return Err(ScanError::new_str(
                    *start_mark,
                    "while parsing a quoted scalar, found unknown escape character",
                ))
            }
        }
        self.skip_n_non_blank(2);

        // Consume an arbitrary escape code.
        if code_length > 0 {
            self.input.lookahead(code_length);
            let mut value = 0u32;
            for i in 0..code_length {
                let c = self.input.peek_nth(i);
                if !is_hex(c) {
                    return Err(ScanError::new_str(
                        *start_mark,
                        "while parsing a quoted scalar, did not find expected hexadecimal number",
                    ));
                }
                value = (value << 4) + as_hex(c);
            }

            self.skip_n_non_blank(code_length);

            // Handle JSON surrogate pairs: high surrogate followed by low surrogate
            if code_length == 4 && (0xD800..=0xDBFF).contains(&value) {
                self.input.lookahead(2);
                if self.input.peek() == '\\' && self.input.peek_nth(1) == 'u' {
                    self.skip_n_non_blank(2);
                    self.input.lookahead(4);
                    let mut low_value = 0u32;
                    for i in 0..4 {
                        let c = self.input.peek_nth(i);
                        if !is_hex(c) {
                            return Err(ScanError::new_str(
                                *start_mark,
                                "while parsing a quoted scalar, did not find expected hexadecimal number for low surrogate",
                            ));
                        }
                        low_value = (low_value << 4) + as_hex(c);
                    }
                    if (0xDC00..=0xDFFF).contains(&low_value) {
                        value = 0x10000 + (((value - 0xD800) << 10) | (low_value - 0xDC00));
                        self.skip_n_non_blank(4);
                    } else {
                        return Err(ScanError::new_str(
                            *start_mark,
                            "while parsing a quoted scalar, found invalid low surrogate",
                        ));
                    }
                } else {
                    return Err(ScanError::new_str(
                        *start_mark,
                        "while parsing a quoted scalar, found high surrogate without following low surrogate",
                    ));
                }
            } else if code_length == 4 && (0xDC00..=0xDFFF).contains(&value) {
                return Err(ScanError::new_str(
                    *start_mark,
                    "while parsing a quoted scalar, found unpaired low surrogate",
                ));
            }

            let Some(ch) = char::from_u32(value) else {
                return Err(ScanError::new_str(
                    *start_mark,
                    "while parsing a quoted scalar, found invalid Unicode character escape code",
                ));
            };
            ret = ch;
        }
        Ok(ret)
    }

    fn fetch_plain_scalar(&mut self) -> ScanResult {
        self.save_simple_key();
        self.disallow_simple_key();

        let tok = self.scan_plain_scalar()?;

        self.tokens.push_back(tok);
        Ok(())
    }

    /// Scan for a plain scalar.
    ///
    /// Plain scalars are the most readable but restricted style. They may span multiple lines in
    /// some contexts.
    #[allow(clippy::too_many_lines)]
    fn scan_plain_scalar(&mut self) -> Result<Token<'input>, ScanError> {
        self.unroll_non_block_indents();
        let indent = self.indent + 1;
        let start_mark = self.mark;

        if self.flow_level > 0 && (start_mark.col as isize) < indent {
            return Err(ScanError::new_str(
                start_mark,
                "invalid indentation in flow construct",
            ));
        }

        let mut string = String::with_capacity(32);
        self.buf_whitespaces.clear();
        self.buf_leading_break.clear();
        self.buf_trailing_breaks.clear();
        let mut end_mark = self.mark;

        loop {
            self.input.lookahead(4);
            if (self.mark.col == 0 && self.input.next_is_document_indicator())
                || self.input.peek() == '#'
            {
                // BS4K: If a `#` starts a comment after some separation spaces following content
                // of a plain scalar in block context, and there is potential continuation on the
                // next line, this is invalid. We cannot decide yet if there will be continuation,
                // so record that a comment interrupted a plain scalar.
                if self.input.peek() == '#'
                    && !string.is_empty()
                    && !self.buf_whitespaces.is_empty()
                    && self.flow_level == 0
                {
                    self.interrupted_plain_by_comment = Some(self.mark);
                }
                break;
            }

            if self.flow_level > 0 && self.input.peek() == '-' && is_flow(self.input.peek_nth(1)) {
                return Err(ScanError::new_str(
                    self.mark,
                    "plain scalar cannot start with '-' followed by ,[]{}",
                ));
            }

            if !self.input.next_is_blank_or_breakz()
                && self.input.next_can_be_plain_scalar(self.flow_level > 0)
            {
                if self.leading_whitespace {
                    if self.buf_leading_break.is_empty() {
                        string.push_str(&self.buf_leading_break);
                        string.push_str(&self.buf_trailing_breaks);
                        self.buf_trailing_breaks.clear();
                        self.buf_leading_break.clear();
                    } else {
                        if self.buf_trailing_breaks.is_empty() {
                            string.push(' ');
                        } else {
                            string.push_str(&self.buf_trailing_breaks);
                            self.buf_trailing_breaks.clear();
                        }
                        self.buf_leading_break.clear();
                    }
                    self.leading_whitespace = false;
                } else if !self.buf_whitespaces.is_empty() {
                    string.push_str(&self.buf_whitespaces);
                    self.buf_whitespaces.clear();
                }

                // We can unroll the first iteration of the loop.
                string.push(self.input.peek());
                self.skip_non_blank();
                string.reserve(self.input.bufmaxlen());

                // Add content non-blank characters to the scalar.
                let mut end = false;
                while !end {
                    // Fill the buffer once and process all characters in the buffer until the next
                    // fetch. Note that `next_can_be_plain_scalar` needs 2 lookahead characters,
                    // hence the `for` loop looping `self.input.bufmaxlen() - 1` times.
                    self.input.lookahead(self.input.bufmaxlen());
                    let (stop, chars_consumed) = self.input.fetch_plain_scalar_chunk(
                        &mut string,
                        self.input.bufmaxlen() - 1,
                        self.flow_level > 0,
                    );
                    end = stop;
                    self.mark.offsets.chars += chars_consumed;
                    self.mark.col += chars_consumed;
                    self.mark.offsets.bytes = self.input.byte_offset();
                }
                end_mark = self.mark;
            }

            // We may reach the end of a plain scalar if:
            //  - We reach eof
            //  - We reach ": "
            //  - We find a flow character in a flow context
            if !(self.input.next_is_blank() || self.input.next_is_break()) {
                break;
            }

            // Process blank characters.
            self.input.lookahead(2);
            while self.input.next_is_blank_or_break() {
                if self.input.next_is_blank() {
                    if !self.leading_whitespace {
                        self.buf_whitespaces.push(self.input.peek());
                        self.skip_blank();
                    } else if (self.mark.col as isize) < indent && self.input.peek() == '\t' {
                        // Tabs in an indentation columns are allowed if and only if the line is
                        // empty. Skip to the end of the line.
                        self.skip_ws_to_eol(SkipTabs::Yes)?;
                        if !self.input.next_is_breakz() {
                            return Err(ScanError::new_str(
                                start_mark,
                                "while scanning a plain scalar, found a tab",
                            ));
                        }
                    } else {
                        self.skip_blank();
                    }
                } else {
                    // Check if it is a first line break
                    if self.leading_whitespace {
                        self.skip_break();
                        self.buf_trailing_breaks.push('\n');
                    } else {
                        self.buf_whitespaces.clear();
                        self.skip_break();
                        self.buf_leading_break.push('\n');
                        self.leading_whitespace = true;
                    }
                }
                self.input.lookahead(2);
            }

            // check indentation level
            if self.flow_level == 0 && (self.mark.col as isize) < indent {
                break;
            }
        }

        if self.leading_whitespace {
            self.allow_simple_key();
        }

        if string.is_empty() {
            // `fetch_plain_scalar` must absolutely consume at least one byte. Otherwise,
            // `fetch_next_token` will never stop calling it. An empty plain scalar may happen with
            // erroneous inputs such as "{...".
            Err(ScanError::new_str(
                start_mark,
                "unexpected end of plain scalar",
            ))
        } else {
            let contents = if let (Some(start), Some(end)) =
                (start_mark.byte_offset(), end_mark.byte_offset())
            {
                match self.try_borrow_slice(start, end) {
                    Some(slice) if slice == string => Cow::Borrowed(slice),
                    _ => Cow::Owned(string),
                }
            } else {
                Cow::Owned(string)
            };

            Ok(Token(
                Span::new(start_mark, end_mark),
                TokenType::Scalar(ScalarStyle::Plain, contents),
            ))
        }
    }

    fn fetch_key(&mut self) -> ScanResult {
        let start_mark = self.mark;
        if self.flow_level == 0 {
            // Check if we are allowed to start a new key (not necessarily simple).
            if !self.simple_key_allowed {
                return Err(ScanError::new_str(
                    self.mark,
                    "mapping keys are not allowed in this context",
                ));
            }
            self.roll_indent(
                start_mark.col,
                None,
                TokenType::BlockMappingStart,
                start_mark,
            );
        } else {
            // The scanner, upon emitting a `Key`, will prepend a `MappingStart` event.
            self.flow_mapping_started = true;
        }

        self.remove_simple_key()?;

        if self.flow_level == 0 {
            self.allow_simple_key();
        } else {
            self.disallow_simple_key();
        }

        self.skip_non_blank();
        self.skip_yaml_whitespace()?;
        if self.input.peek() == '\t' {
            return Err(ScanError::new_str(
                self.mark(),
                "tabs disallowed in this context",
            ));
        }
        self.tokens
            .push_back(Token(Span::new(start_mark, self.mark), TokenType::Key));
        Ok(())
    }

    /// Fetch a value in a mapping inside of a flow collection.
    ///
    /// This must not be called if [`self.flow_level`] is 0. This ensures the rules surrounding
    /// values in flow collections are respected prior to calling [`fetch_value`].
    ///
    /// [`self.flow_level`]: Self::flow_level
    /// [`fetch_value`]: Self::fetch_value
    fn fetch_flow_value(&mut self) -> ScanResult {
        let nc = self.input.peek_nth(1);

        // If we encounter a ':' inside a flow collection and it is not immediately
        // followed by a blank or breakz:
        //   - We must check whether an adjacent value is allowed
        //     `["a":[]]` is valid. If the key is double-quoted, no need for a space. This
        //     is needed for JSON compatibility.
        //   - If not, we must ensure there is a space after the ':' and before its value.
        //     `[a: []]` is valid while `[a:[]]` isn't. `[a:b]` is treated as `["a:b"]`.
        //   - But if the value is empty (null), then it's okay.
        // The last line is for YAMLs like `[a:]`. The ':' is followed by a ']' (which is a
        // flow character), but the ']' is not the value. The value is an invisible empty
        // space which is represented as null ('~').
        if self.mark.index() != self.adjacent_value_allowed_at && (nc == '[' || nc == '{') {
            return Err(ScanError::new_str(
                self.mark,
                "':' may not precede any of `[{` in flow mapping",
            ));
        }

        self.fetch_value()
    }

    /// Fetch a value from a mapping (after a `:`).
    fn fetch_value(&mut self) -> ScanResult {
        let sk = self.simple_keys.last().unwrap().clone();
        let start_mark = self.mark;
        let is_implicit_flow_mapping =
            !self.implicit_flow_mapping_states.is_empty() && !self.flow_mapping_started;
        if is_implicit_flow_mapping {
            *self.implicit_flow_mapping_states.last_mut().unwrap() =
                ImplicitMappingState::Inside(self.flow_level);
        }

        // Skip over ':'.
        self.skip_non_blank();
        // Error detection: if ':' is followed by tab(s) without any space, and then what looks
        // like a value, emit a helpful error. The check for '-' or alphanumeric is an intentional
        // heuristic that catches common cases (e.g., `key:\tvalue`, `key:\t-item`) without
        // rejecting valid YAML like `key:\t|` (block scalar) or `key:\t"quoted"`.
        // Note: This heuristic won't catch Unicode value starters like `key:\täöü`, but such
        // cases will still fail to parse correctly (just with a less specific error message).
        if self.input.look_ch() == '\t'
            && !self.skip_ws_to_eol(SkipTabs::Yes)?.has_valid_yaml_ws()
            && (self.input.peek() == '-' || self.input.next_is_alpha())
        {
            return Err(ScanError::new_str(
                self.mark,
                "':' must be followed by a valid YAML whitespace",
            ));
        }

        if sk.possible {
            // insert simple key
            let tok = Token(Span::empty(sk.mark), TokenType::Key);
            self.insert_token(sk.token_number - self.tokens_parsed, tok);
            if is_implicit_flow_mapping {
                if sk.mark.line < start_mark.line {
                    return Err(ScanError::new_str(
                        start_mark,
                        "illegal placement of ':' indicator",
                    ));
                }
                self.insert_token(
                    sk.token_number - self.tokens_parsed,
                    Token(Span::empty(sk.mark), TokenType::FlowMappingStart),
                );
            }

            // Add the BLOCK-MAPPING-START token if needed.
            self.roll_indent(
                sk.mark.col,
                Some(sk.token_number),
                TokenType::BlockMappingStart,
                sk.mark,
            );
            self.roll_one_col_indent();

            self.simple_keys.last_mut().unwrap().possible = false;
            self.disallow_simple_key();
        } else {
            if is_implicit_flow_mapping {
                self.tokens
                    .push_back(Token(Span::empty(start_mark), TokenType::FlowMappingStart));
            }
            // The ':' indicator follows a complex key.
            if self.flow_level == 0 {
                if !self.simple_key_allowed {
                    return Err(ScanError::new_str(
                        start_mark,
                        "mapping values are not allowed in this context",
                    ));
                }

                self.roll_indent(
                    start_mark.col,
                    None,
                    TokenType::BlockMappingStart,
                    start_mark,
                );
            }
            self.roll_one_col_indent();

            if self.flow_level == 0 {
                self.allow_simple_key();
            } else {
                self.disallow_simple_key();
            }
        }
        self.tokens
            .push_back(Token(Span::empty(start_mark), TokenType::Value));

        Ok(())
    }

    /// Add an indentation level to the stack with the given block token, if needed.
    ///
    /// An indentation level is added only if:
    ///   - We are not in a flow-style construct (which don't have indentation per-se).
    ///   - The current column is further indented than the last indent we have registered.
    fn roll_indent(
        &mut self,
        col: usize,
        number: Option<usize>,
        tok: TokenType<'input>,
        mark: Marker,
    ) {
        if self.flow_level > 0 {
            return;
        }

        // If the last indent was a non-block indent, remove it.
        // This means that we prepared an indent that we thought we wouldn't use, but realized just
        // now that it is a block indent.
        if self.indent <= col as isize {
            if let Some(indent) = self.indents.last() {
                if !indent.needs_block_end {
                    self.indent = indent.indent;
                    self.indents.pop();
                }
            }
        }

        if self.indent < col as isize {
            self.indents.push(Indent {
                indent: self.indent,
                needs_block_end: true,
            });
            self.indent = col as isize;
            let tokens_parsed = self.tokens_parsed;
            match number {
                Some(n) => self.insert_token(n - tokens_parsed, Token(Span::empty(mark), tok)),
                None => self.tokens.push_back(Token(Span::empty(mark), tok)),
            }
        }
    }

    /// Pop indentation levels from the stack as much as needed.
    ///
    /// Indentation levels are popped from the stack while they are further indented than `col`.
    /// If we are in a flow-style construct (which don't have indentation per-se), this function
    /// does nothing.
    fn unroll_indent(&mut self, col: isize) {
        if self.flow_level > 0 {
            return;
        }
        while self.indent > col {
            let indent = self.indents.pop().unwrap();
            self.indent = indent.indent;
            if indent.needs_block_end {
                self.tokens
                    .push_back(Token(Span::empty(self.mark), TokenType::BlockEnd));
            }
        }
    }

    /// Add an indentation level of 1 column that does not start a block.
    ///
    /// See the documentation of [`Indent::needs_block_end`] for more details.
    /// An indentation is not added if we are inside a flow level or if the last indent is already
    /// a non-block indent.
    fn roll_one_col_indent(&mut self) {
        if self.flow_level == 0 && self.indents.last().is_some_and(|x| x.needs_block_end) {
            self.indents.push(Indent {
                indent: self.indent,
                needs_block_end: false,
            });
            self.indent += 1;
        }
    }

    /// Unroll all last indents created with [`Self::roll_one_col_indent`].
    fn unroll_non_block_indents(&mut self) {
        while let Some(indent) = self.indents.last() {
            if indent.needs_block_end {
                break;
            }
            self.indent = indent.indent;
            self.indents.pop();
        }
    }

    /// Mark the next token to be inserted as a potential simple key.
    fn save_simple_key(&mut self) {
        if self.simple_key_allowed {
            let required = self.flow_level == 0
                && self.indent == (self.mark.col as isize)
                && self.indents.last().unwrap().needs_block_end;

            if let Some(last) = self.simple_keys.last_mut() {
                *last = SimpleKey {
                    mark: self.mark,
                    possible: true,
                    required,
                    token_number: self.tokens_parsed + self.tokens.len(),
                };
            }
        }
    }

    fn remove_simple_key(&mut self) -> ScanResult {
        let last = self.simple_keys.last_mut().unwrap();
        if last.possible && last.required {
            return Err(self.simple_key_expected());
        }

        last.possible = false;
        Ok(())
    }

    /// Return whether the scanner is inside a block but outside of a flow sequence.
    fn is_within_block(&self) -> bool {
        !self.indents.is_empty()
    }

    /// If an implicit mapping had started, end it.
    ///
    /// This function does not pop the state in [`implicit_flow_mapping_states`].
    ///
    /// [`implicit_flow_mapping_states`]: Self::implicit_flow_mapping_states
    fn end_implicit_mapping(&mut self, mark: Marker, flow_level: u8) {
        if let Some(implicit_mapping) = self.implicit_flow_mapping_states.last_mut() {
            if *implicit_mapping == ImplicitMappingState::Inside(flow_level) {
                self.flow_mapping_started = false;
                *implicit_mapping = ImplicitMappingState::Possible;
                self.tokens
                    .push_back(Token(Span::empty(mark), TokenType::FlowMappingEnd));
            }
        }
    }
}

/// Chomping, how final line breaks and trailing empty lines are interpreted.
///
/// See YAML spec 8.1.1.2.
#[derive(PartialEq, Eq)]
pub enum Chomping {
    /// The final line break and any trailing empty lines are excluded.
    Strip,
    /// The final line break is preserved, but trailing empty lines are excluded.
    Clip,
    /// The final line break and trailing empty lines are included.
    Keep,
}

#[cfg(test)]
mod test {
    use alloc::borrow::Cow;

    use crate::{
        input::str::StrInput,
        scanner::{Scanner, TokenType},
    };

    #[test]
    fn test_is_anchor_char() {
        use super::is_anchor_char;
        assert!(is_anchor_char('x'));
    }

    /// Ensure anchors scanned from `StrInput` are returned as `Cow::Borrowed`.
    #[test]
    fn anchor_name_is_borrowed_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("&anch\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Anchor(name) = tok.1 {
                assert!(matches!(name, Cow::Borrowed("anch")));
                break;
            }
        }
    }

    /// Ensure aliases scanned from `StrInput` are returned as `Cow::Borrowed`.
    #[test]
    fn alias_name_is_borrowed_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("*anch\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Alias(name) = tok.1 {
                assert!(matches!(name, Cow::Borrowed("anch")));
                break;
            }
        }
    }

    /// Ensure `%TAG` directive handle and prefix are borrowed when they are verbatim (no escapes).
    #[test]
    fn tag_directive_parts_are_borrowed_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("%TAG !e! tag:example.com,2000:app/\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::TagDirective(handle, prefix) = tok.1 {
                assert!(matches!(handle, Cow::Borrowed("!e!")));
                assert!(matches!(prefix, Cow::Borrowed("tag:example.com,2000:app/")));
                break;
            }
        }
    }

    #[test]
    fn plain_scalar_is_borrowed_when_whitespace_free_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("foo\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Scalar(_, value) = tok.1 {
                assert!(matches!(value, Cow::Borrowed("foo")));
                break;
            }
        }
    }

    #[test]
    fn plain_scalar_is_borrowed_when_whitespace_present_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("foo bar\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Scalar(_, value) = tok.1 {
                assert!(matches!(value, Cow::Borrowed("foo bar")));
                break;
            }
        }
    }

    #[test]
    fn single_quoted_scalar_is_borrowed_when_verbatim_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("'foo bar'\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Scalar(_, value) = tok.1 {
                assert!(matches!(value, Cow::Borrowed("foo bar")));
                break;
            }
        }
    }

    #[test]
    fn single_quoted_scalar_is_owned_when_quote_is_escaped_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("'foo''bar'\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Scalar(_, value) = tok.1 {
                assert!(matches!(value, Cow::Owned(_)));
                assert_eq!(&*value, "foo'bar");
                break;
            }
        }
    }

    #[test]
    fn double_quoted_scalar_is_borrowed_when_verbatim_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("\"foo bar\"\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Scalar(_, value) = tok.1 {
                assert!(matches!(value, Cow::Borrowed("foo bar")));
                break;
            }
        }
    }

    #[test]
    fn double_quoted_scalar_is_owned_when_escape_sequence_present_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("\"foo\\nbar\"\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Scalar(_, value) = tok.1 {
                assert!(matches!(value, Cow::Owned(_)));
                assert_eq!(&*value, "foo\nbar");
                break;
            }
        }
    }

    #[test]
    fn plain_key_is_borrowed_for_str_input() {
        // Keys are just scalars in a key position; they should also be borrowed.
        let mut scanner = Scanner::new(StrInput::new("mykey: value\n"));

        let mut found_key = false;
        let mut key_value: Option<Cow<'_, str>> = None;

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors");
            let Some(tok) = tok else { break };

            if matches!(tok.1, TokenType::Key) {
                found_key = true;
            } else if found_key {
                if let TokenType::Scalar(_, value) = tok.1 {
                    key_value = Some(value);
                    break;
                }
            }
        }

        assert!(found_key, "expected to find a Key token");
        let key_value = key_value.expect("expected to find a scalar after Key token");
        assert!(
            matches!(key_value, Cow::Borrowed("mykey")),
            "key should be borrowed, got: {key_value:?}"
        );
    }

    #[test]
    fn quoted_key_is_borrowed_when_verbatim_for_str_input() {
        let mut scanner = Scanner::new(StrInput::new("\"mykey\": value\n"));

        let mut found_key = false;
        let mut key_value: Option<Cow<'_, str>> = None;

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors");
            let Some(tok) = tok else { break };

            if matches!(tok.1, TokenType::Key) {
                found_key = true;
            } else if found_key {
                if let TokenType::Scalar(_, value) = tok.1 {
                    key_value = Some(value);
                    break;
                }
            }
        }

        assert!(found_key, "expected to find a Key token");
        let key_value = key_value.expect("expected to find a scalar after Key token");
        assert!(
            matches!(key_value, Cow::Borrowed("mykey")),
            "quoted key should be borrowed when verbatim, got: {key_value:?}"
        );
    }

    #[test]
    fn tag_handle_and_suffix_are_borrowed_for_str_input() {
        // Test a tag like !!str which should have handle="!!" and suffix="str"
        let mut scanner = Scanner::new(StrInput::new("!!str foo\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Tag(handle, suffix) = tok.1 {
                assert!(
                    matches!(handle, Cow::Borrowed("!!")),
                    "tag handle should be borrowed, got: {handle:?}"
                );
                assert!(
                    matches!(suffix, Cow::Borrowed("str")),
                    "tag suffix should be borrowed, got: {suffix:?}"
                );
                break;
            }
        }
    }

    #[test]
    fn local_tag_suffix_is_borrowed_for_str_input() {
        // Test a local tag like !mytag which should have handle="!" and suffix="mytag"
        let mut scanner = Scanner::new(StrInput::new("!mytag foo\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Tag(handle, suffix) = tok.1 {
                assert!(
                    matches!(handle, Cow::Borrowed("!")),
                    "local tag handle should be '!', got: {handle:?}"
                );
                assert!(
                    matches!(suffix, Cow::Borrowed("mytag")),
                    "local tag suffix should be borrowed, got: {suffix:?}"
                );
                break;
            }
        }
    }

    #[test]
    fn tag_with_uri_escape_is_owned_for_str_input() {
        // Test a tag with URI escape like !my%20tag - suffix must be owned due to decoding
        let mut scanner = Scanner::new(StrInput::new("!!my%20tag foo\n"));

        loop {
            let tok = scanner
                .next_token()
                .expect("valid YAML must scan without errors")
                .expect("scanner must eventually produce a token");
            if let TokenType::Tag(handle, suffix) = tok.1 {
                assert!(
                    matches!(handle, Cow::Borrowed("!!")),
                    "tag handle should still be borrowed, got: {handle:?}"
                );
                assert!(
                    matches!(suffix, Cow::Owned(_)),
                    "tag suffix with URI escape should be owned, got: {suffix:?}"
                );
                assert_eq!(&*suffix, "my tag");
                break;
            }
        }
    }
}
