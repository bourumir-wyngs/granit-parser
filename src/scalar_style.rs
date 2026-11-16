//! Scalar notation and compact block-header source metadata.

use core::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    ops::Range,
};

use crate::Span;

/// How a block scalar handles its final line break and trailing empty lines.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[non_exhaustive]
pub enum Chomping {
    /// No sign was written: preserve the final line break, but not trailing empty lines.
    Clip,
    /// A `-` was written: remove the final line break and trailing empty lines.
    Strip,
    /// A `+` was written: preserve the final line break and trailing empty lines.
    Keep,
}

impl Default for Chomping {
    fn default() -> Self {
        Self::Clip
    }
}

/// Source syntax attached to a literal or folded block scalar.
///
/// Coordinates are relative to the **start of the accompanying scalar span**. Keep that span
/// together with this value: rebasing or replacing its start invalidates these distances.
/// The ordinary scalar span retains its existing meaning, which need not include the header.
/// This type does not store another [`crate::Marker`] or retain borrowed source text.
///
/// The header ranges cover `|` or `>` and its adjacent indicators, excluding whitespace,
/// comments, and the header line break. Character and byte distances differ when, for example,
/// a header comment contains Unicode. Byte distances are an optional coordinate enhancement,
/// like byte offsets on [`crate::Marker`], and do not participate in equality, ordering, or
/// hashing. Use [`Self::header_byte_range`] to inspect them when available.
/// Indicator offsets are computed from the chomping mode, indentation digit, and their order.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct BlockScalarSyntax {
    /// Character distance backwards from the scalar span's start to `|` or `>`.
    pub header_chars_before_span: usize,
    /// Byte distance backwards from the scalar span's start to `|` or `>`.
    ///
    /// Meaningful only when the accompanying span's start has a byte offset. Otherwise zero.
    pub header_bytes_before_span: usize,
    /// Chomping mode selected by the header.
    pub chomping: Chomping,
    /// Explicit indentation digit (`1..=9`), or `None` when indentation was inferred.
    pub indentation_indicator: Option<u8>,
    /// Whether the indentation digit precedes the chomping sign when both are present.
    ///
    /// Ignored, including for equality, ordering, and hashing, when either indicator is absent.
    /// Parsed headers set this to `false` in that case.
    pub digit_first: bool,
    /// Effective number of leading indentation spaces removed from content lines.
    ///
    /// This is an absolute content indentation level, not the explicit indentation digit.
    /// For an inferred header ending at EOF before its line break, no content line exists:
    /// this reports the minimum level required by the enclosing block, not the header column.
    pub content_indent: usize,
}

impl BlockScalarSyntax {
    /// Construct synthetic syntax whose header begins at the accompanying span's start.
    ///
    /// * `chomping` selects clipping, stripping, or keeping trailing line breaks.
    /// * `indentation_indicator` is the optional explicit digit, which must be in `1..=9`.
    /// * `content_indent` is the effective absolute content indentation level.
    ///
    /// Returns `None` for an invalid digit. Otherwise the sign precedes the digit when both
    /// are present, and both backwards distances are zero. Set the public fields to describe
    /// a different indicator order or a header preceding the scalar span.
    #[must_use]
    pub const fn new(
        chomping: Chomping,
        indentation_indicator: Option<u8>,
        content_indent: usize,
    ) -> Option<Self> {
        if matches!(indentation_indicator, Some(0 | 10..=u8::MAX)) {
            return None;
        }
        Some(Self {
            header_chars_before_span: 0,
            header_bytes_before_span: 0,
            chomping,
            indentation_indicator,
            digit_first: false,
            content_indent,
        })
    }

    /// Return the header-indicator length in both characters and bytes, excluding whitespace.
    ///
    /// Parsed headers contain one to three ASCII characters.
    #[must_use]
    pub const fn header_len(&self) -> usize {
        1 + !matches!(self.chomping, Chomping::Clip) as usize
            + self.indentation_indicator.is_some() as usize
    }

    /// Return the chomping sign's offset from the header start, or `None` for clipping.
    ///
    /// Offsets are the same in characters and bytes: a block header contains only ASCII.
    #[must_use]
    pub const fn chomping_offset(&self) -> Option<u8> {
        match self.chomping {
            Chomping::Clip => None,
            Chomping::Strip | Chomping::Keep => {
                if self.digit_first && self.indentation_indicator.is_some() {
                    Some(2)
                } else {
                    Some(1)
                }
            }
        }
    }

    /// Return the indentation digit's offset from the header start, or `None` if inferred.
    ///
    /// Offsets are the same in characters and bytes: a block header contains only ASCII.
    #[must_use]
    pub const fn indentation_offset(&self) -> Option<u8> {
        match self.indentation_indicator {
            None => None,
            Some(_) => {
                if !self.digit_first && !matches!(self.chomping, Chomping::Clip) {
                    Some(2)
                } else {
                    Some(1)
                }
            }
        }
    }

    /// Return the header's character-index range using its accompanying scalar `span`.
    ///
    /// Returns `None` if the coordinates underflow or overflow. The caller must supply the
    /// original matching span; arithmetic checks cannot detect every mismatched span.
    #[must_use]
    pub fn header_char_range(&self, span: &Span) -> Option<Range<usize>> {
        let start = span
            .start
            .index()
            .checked_sub(self.header_chars_before_span)?;
        Some(start..start.checked_add(self.header_len())?)
    }

    /// Return the header's byte range using its accompanying scalar `span`.
    ///
    /// Returns `None` for inputs without byte offsets or if coordinates underflow or overflow.
    /// The caller must supply the original matching span.
    #[must_use]
    pub fn header_byte_range(&self, span: &Span) -> Option<Range<usize>> {
        let start = span
            .start
            .byte_offset()?
            .checked_sub(self.header_bytes_before_span)?;
        Some(start..start.checked_add(self.header_len())?)
    }

    /// Return the explicit chomping sign's character range, or `None` if absent/unavailable.
    #[must_use]
    pub fn chomping_char_range(&self, span: &Span) -> Option<Range<usize>> {
        Self::indicator_range(self.header_char_range(span)?, self.chomping_offset()?)
    }

    /// Return the explicit chomping sign's byte range, or `None` if absent/unavailable.
    #[must_use]
    pub fn chomping_byte_range(&self, span: &Span) -> Option<Range<usize>> {
        Self::indicator_range(self.header_byte_range(span)?, self.chomping_offset()?)
    }

    /// Return the explicit indentation digit's character range, or `None` if absent/unavailable.
    #[must_use]
    pub fn indentation_char_range(&self, span: &Span) -> Option<Range<usize>> {
        Self::indicator_range(self.header_char_range(span)?, self.indentation_offset()?)
    }

    /// Return the explicit indentation digit's byte range, or `None` if absent/unavailable.
    #[must_use]
    pub fn indentation_byte_range(&self, span: &Span) -> Option<Range<usize>> {
        Self::indicator_range(self.header_byte_range(span)?, self.indentation_offset()?)
    }

    /// Return the raw header indicators from the original source.
    ///
    /// * `span` is the original accompanying scalar span.
    /// * `source` is the complete original UTF-8 source, using the span's coordinate origin.
    ///
    /// Returns `None` when byte positions are unavailable or outside valid source boundaries.
    #[must_use]
    pub fn header_slice<'source>(&self, span: &Span, source: &'source str) -> Option<&'source str> {
        source.get(self.header_byte_range(span)?)
    }

    fn indicator_range(header: Range<usize>, offset: u8) -> Option<Range<usize>> {
        if offset == 0 {
            return None;
        }
        let start = header.start.checked_add(usize::from(offset))?;
        let end = start.checked_add(1)?;
        (end <= header.end).then_some(start..end)
    }

    fn identity(&self) -> (usize, Chomping, Option<u8>, bool, usize) {
        (
            self.header_chars_before_span,
            self.chomping,
            self.indentation_indicator,
            self.chomping_offset() == Some(2),
            self.content_indent,
        )
    }
}

impl PartialEq for BlockScalarSyntax {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}

impl Eq for BlockScalarSyntax {}

impl PartialOrd for BlockScalarSyntax {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BlockScalarSyntax {
    fn cmp(&self, other: &Self) -> Ordering {
        self.identity().cmp(&other.identity())
    }
}

impl Hash for BlockScalarSyntax {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity().hash(state);
    }
}

/// A scalar's notation category, independent of source positions and header metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[non_exhaustive]
pub enum ScalarKind {
    /// Plain notation.
    Plain,
    /// Single-quoted notation.
    SingleQuoted,
    /// Double-quoted notation.
    DoubleQuoted,
    /// Literal block notation (`|`).
    Literal,
    /// Folded block notation (`>`).
    Folded,
}

/// The source notation and applicable syntax metadata of a YAML scalar.
///
/// Match variants carrying data with `..`, and use their constructors for synthetic values.
/// Plain and quoted styles are ordinary unit variants and can be used directly. Equality
/// includes block-header metadata; use [`Self::kind`] or the category predicates to compare
/// notation alone. Whether a scalar was omitted is recorded on [`crate::Event::Scalar`].
///
/// ```
/// use granit_parser::ScalarStyle;
/// let style = ScalarStyle::Plain;
/// assert_eq!(style, ScalarStyle::Plain);
/// assert!(style.is_plain());
/// assert_eq!(ScalarStyle::DoubleQuoted.kind(), granit_parser::ScalarKind::DoubleQuoted);
/// ```
///
/// External callers cannot construct variants carrying data directly:
///
/// ```compile_fail,E0639
/// use granit_parser::{BlockScalarSyntax, ScalarStyle};
/// let style = ScalarStyle::Literal { syntax: BlockScalarSyntax::default() };
/// ```
///
/// Variant patterns must allow future fields:
///
/// ```compile_fail,E0638
/// use granit_parser::{BlockScalarSyntax, ScalarStyle};
/// let style = ScalarStyle::literal(BlockScalarSyntax::default());
/// if let ScalarStyle::Literal { syntax } = style {}
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[non_exhaustive]
pub enum ScalarStyle {
    /// A plain scalar or a synthesized empty node.
    Plain,
    /// A single-quoted scalar.
    SingleQuoted,
    /// A double-quoted scalar.
    DoubleQuoted,
    /// A literal block, with no escape processing or line folding.
    #[non_exhaustive]
    Literal {
        /// Header syntax relative to the accompanying scalar span.
        syntax: BlockScalarSyntax,
    },
    /// A folded block, whose line breaks are subject to YAML line folding.
    #[non_exhaustive]
    Folded {
        /// Header syntax relative to the accompanying scalar span.
        syntax: BlockScalarSyntax,
    },
}

impl ScalarStyle {
    /// Return literal block style carrying `syntax`.
    #[must_use]
    pub const fn literal(syntax: BlockScalarSyntax) -> Self {
        Self::Literal { syntax }
    }

    /// Return folded block style carrying `syntax`.
    #[must_use]
    pub const fn folded(syntax: BlockScalarSyntax) -> Self {
        Self::Folded { syntax }
    }

    /// Return the notation category, ignoring source positions and all other metadata.
    #[must_use]
    pub const fn kind(&self) -> ScalarKind {
        match self {
            Self::Plain => ScalarKind::Plain,
            Self::SingleQuoted => ScalarKind::SingleQuoted,
            Self::DoubleQuoted => ScalarKind::DoubleQuoted,
            Self::Literal { .. } => ScalarKind::Literal,
            Self::Folded { .. } => ScalarKind::Folded,
        }
    }

    /// Return whether this is plain notation, including synthesized empty nodes.
    ///
    /// Equivalent to `self == ScalarStyle::Plain`; omission is metadata on the scalar event.
    #[must_use]
    pub const fn is_plain(&self) -> bool {
        matches!(self, Self::Plain)
    }

    /// Return whether this is single-quoted notation.
    #[must_use]
    pub const fn is_single_quoted(&self) -> bool {
        matches!(self, Self::SingleQuoted)
    }

    /// Return whether this is double-quoted notation.
    #[must_use]
    pub const fn is_double_quoted(&self) -> bool {
        matches!(self, Self::DoubleQuoted)
    }

    /// Return whether this is literal block notation.
    #[must_use]
    pub const fn is_literal(&self) -> bool {
        matches!(self, Self::Literal { .. })
    }

    /// Return whether this is folded block notation.
    #[must_use]
    pub const fn is_folded(&self) -> bool {
        matches!(self, Self::Folded { .. })
    }

    /// Return block-header syntax for literal and folded scalars, or `None` otherwise.
    #[must_use]
    pub const fn block_syntax(&self) -> Option<&BlockScalarSyntax> {
        match self {
            Self::Literal { syntax } | Self::Folded { syntax } => Some(syntax),
            _ => None,
        }
    }
}
