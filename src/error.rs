//! Parser and scanner error types.

use alloc::string::{String, ToString};
use core::fmt;

use crate::scanner::Marker;

/// Machine-readable category for a [`ScanError`].
#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Too many consecutive comments were buffered before a collection entry.
    TooManyComments,
    /// Input ended while parsing a flow sequence.
    UnexpectedEofFlowSequence,
    /// Input ended while parsing a flow mapping.
    UnexpectedEofFlowMapping,
    /// Input ended while parsing an implicit flow mapping.
    UnexpectedEofImplicitFlowMapping,
    /// Input ended while parsing a block sequence.
    UnexpectedEofBlockSequence,
    /// Input ended while parsing a block mapping.
    UnexpectedEofBlockMapping,
    /// Input ended unexpectedly in another parser state.
    UnexpectedEof,
    /// A stream-start token was expected.
    ExpectedStreamStart,
    /// More than one YAML version directive was found for a document.
    DuplicateVersionDirective,
    /// The YAML major version is unsupported.
    UnsupportedYamlMajorVersion,
    /// A tag directive handle was declared more than once for a document.
    DuplicateTagDirective,
    /// A document-start token was expected.
    ExpectedDocumentStart,
    /// A directive followed an implicit document without an explicit document end.
    MissingDocumentEndBeforeDirective,
    /// The parser ran out of representable anchor identifiers.
    AnchorCountOverflow,
    /// An alias referred to an unknown anchor.
    UnknownAnchor,
    /// The parser did not find expected node content.
    ExpectedNodeContent,
    /// A block mapping key was expected.
    ExpectedBlockMappingKey,
    /// A flow mapping separator or closing brace was expected.
    ExpectedFlowMappingSeparator,
    /// A flow sequence separator or closing bracket was expected.
    ExpectedFlowSequenceSeparator,
    /// A block sequence entry indicator was expected.
    ExpectedBlockSequenceEntry,
    /// A tag used a handle that was not declared.
    UndeclaredTagHandle,
    /// No include resolver was configured for a parser stack.
    MissingIncludeResolver,
    /// A parser-stack entry contained multiple documents where only one is supported.
    MultipleDocumentsUnsupported,
    /// An input advertised byte offsets but did not provide the requested slice.
    InputOffsetsWithoutSlice,
    /// An input advertised slicing but did not provide the requested slice.
    InputSlicingUnavailable,
    /// A tag did not begin with the expected exclamation mark.
    ExpectedTagBang,
    /// A tag directive handle did not end with the expected exclamation mark.
    ExpectedTagDirectiveBang,
    /// A global tag started with an invalid character.
    InvalidGlobalTagCharacter,
    /// A required simple key was not followed by a value indicator.
    SimpleKeyExpected,
    /// A previously saved simple key was no longer valid.
    InvalidSimpleKey,
    /// Invalid content followed a document-end marker.
    InvalidDocumentEnd,
    /// Indentation was invalid for the current parser context.
    InvalidIndentation,
    /// A byte-order mark appeared inside a document.
    BomInsideDocument,
    /// An unexpected reserved character was encountered.
    UnexpectedCharacter {
        /// The character that was encountered.
        character: char,
    },
    /// The scanner could not produce the next token.
    MissingNextToken,
    /// A tab was used in a context where it is not allowed.
    TabNotAllowed,
    /// A tab was used in block indentation.
    TabInBlockIndentation,
    /// A comment interrupted a multiline plain scalar.
    CommentInterceptedScalar,
    /// Required whitespace was not found.
    ExpectedWhitespace,
    /// A comment was not separated from the preceding token by whitespace.
    CommentNotSeparated,
    /// A directive did not end with a comment or line break.
    InvalidDirectiveTerminator,
    /// A YAML version directive did not contain the expected digit or dot.
    MissingYamlVersionSeparator,
    /// A directive name was missing.
    MissingDirectiveName,
    /// A directive name contained an invalid character.
    InvalidDirectiveName,
    /// A YAML version component exceeded the supported length.
    YamlVersionTooLong,
    /// A YAML version component was missing.
    MissingYamlVersion,
    /// A tag directive did not end with whitespace or a line break.
    InvalidTagDirectiveTerminator,
    /// A tag token did not end with valid separation whitespace.
    InvalidTagTerminator,
    /// A tag URI was missing.
    MissingTagUri,
    /// A verbatim tag was missing its closing angle bracket.
    UnclosedVerbatimTag,
    /// A tag contained an invalid percent escape.
    InvalidTagEscape,
    /// A tag escape started with an invalid UTF-8 byte.
    InvalidTagUtf8LeadingByte,
    /// A tag escape contained an invalid trailing UTF-8 byte.
    InvalidTagUtf8TrailingByte,
    /// A tag escape did not decode to one valid Unicode scalar value.
    InvalidTagUtf8,
    /// An anchor or alias name was missing.
    MissingAnchorOrAliasName,
    /// A flow collection closing bracket was misplaced.
    MisplacedFlowCollectionEnd,
    /// A flow collection was closed with the wrong bracket type.
    MismatchedFlowCollectionEnd {
        /// The bracket that opened the flow collection.
        open: char,
        /// The bracket that closed the flow collection.
        close: char,
    },
    /// A flow collection was not closed.
    UnclosedFlowCollection {
        /// The bracket that opened the flow collection.
        open: char,
    },
    /// The supported flow nesting limit was exceeded.
    RecursionLimitExceeded,
    /// A block entry indicator appeared inside a flow collection.
    BlockEntryInFlowCollection,
    /// A block sequence entry appeared in a context that does not allow it.
    BlockSequenceEntryNotAllowed,
    /// A block entry indicator was followed by invalid whitespace.
    InvalidBlockEntryWhitespace,
    /// A block scalar used an indentation indicator of zero.
    ZeroBlockScalarIndent,
    /// A block scalar header did not end with a comment or line break.
    InvalidBlockScalarHeader,
    /// Block scalar content began with a tab.
    TabAtBlockScalarStart,
    /// A block scalar content line had invalid indentation.
    InvalidBlockScalarIndent,
    /// A document indicator appeared inside a quoted scalar.
    DocumentIndicatorInQuotedScalar,
    /// A quoted scalar was not closed.
    UnclosedQuotedScalar,
    /// A tab was used as indentation.
    TabInIndentation,
    /// A multiline quoted scalar had invalid indentation.
    InvalidQuotedScalarIndent,
    /// Invalid content followed a single-quoted scalar.
    InvalidTrailingSingleQuotedScalar,
    /// Invalid content followed a double-quoted scalar.
    InvalidTrailingDoubleQuotedScalar,
    /// A quoted scalar contained an unknown escape character.
    UnknownQuotedScalarEscape,
    /// A quoted scalar escape did not contain the expected hexadecimal digits.
    InvalidQuotedScalarHexEscape,
    /// A low-surrogate escape did not contain the expected hexadecimal digits.
    InvalidLowSurrogateHexEscape,
    /// A surrogate pair contained an invalid low surrogate.
    InvalidLowSurrogate,
    /// A high surrogate was not followed by a low surrogate.
    MissingLowSurrogate,
    /// A low surrogate appeared without a preceding high surrogate.
    UnpairedLowSurrogate,
    /// A quoted scalar escape did not represent a valid Unicode scalar value.
    InvalidUnicodeEscape,
    /// A flow scalar started at invalid indentation.
    InvalidFlowScalarIndent,
    /// A plain scalar began with a dash followed by a flow indicator.
    PlainScalarStartsWithDashFlowIndicator,
    /// A tab appeared where a plain scalar could not accept it.
    TabInPlainScalar,
    /// A plain scalar ended before consuming any content.
    UnexpectedEndOfPlainScalar,
    /// A mapping key appeared in a context that does not allow one.
    MappingKeyNotAllowed,
    /// A flow mapping value indicator was adjacent to a collection start.
    FlowMappingValueAdjacentCollection,
    /// A mapping value indicator was followed by invalid whitespace.
    InvalidMappingValueWhitespace,
    /// A value indicator was placed illegally in an implicit flow mapping.
    InvalidColonPlacement,
    /// A mapping value appeared in a context that does not allow one.
    MappingValueNotAllowed,
}

#[cfg(feature = "error_messages")]
impl fmt::Display for ErrorKind {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyComments => {
                f.write_str("too many consecutive comments before resolving collection entry")
            }
            Self::UnexpectedEofFlowSequence => {
                f.write_str("unexpected EOF while parsing a flow sequence")
            }
            Self::UnexpectedEofFlowMapping => {
                f.write_str("unexpected EOF while parsing a flow mapping")
            }
            Self::UnexpectedEofImplicitFlowMapping => {
                f.write_str("unexpected EOF while parsing an implicit flow mapping")
            }
            Self::UnexpectedEofBlockSequence => {
                f.write_str("unexpected EOF while parsing a block sequence")
            }
            Self::UnexpectedEofBlockMapping => {
                f.write_str("unexpected EOF while parsing a block mapping")
            }
            Self::UnexpectedEof => f.write_str("unexpected eof"),
            Self::ExpectedStreamStart => f.write_str("did not find expected <stream-start>"),
            Self::DuplicateVersionDirective => f.write_str("duplicate version directive"),
            Self::UnsupportedYamlMajorVersion => {
                f.write_str("unsupported YAML major version")
            }
            Self::DuplicateTagDirective => f.write_str(
                "the TAG directive must only be given at most once per handle in the same document",
            ),
            Self::ExpectedDocumentStart => {
                f.write_str("did not find expected <document start>")
            }
            Self::MissingDocumentEndBeforeDirective => {
                f.write_str("missing explicit document end marker before directive")
            }
            Self::AnchorCountOverflow => {
                f.write_str("while parsing anchor, anchor count exceeded supported limit")
            }
            Self::UnknownAnchor => f.write_str("while parsing node, found unknown anchor"),
            Self::ExpectedNodeContent => {
                f.write_str("while parsing a node, did not find expected node content")
            }
            Self::ExpectedBlockMappingKey => {
                f.write_str("while parsing a block mapping, did not find expected key")
            }
            Self::ExpectedFlowMappingSeparator => {
                f.write_str("while parsing a flow mapping, did not find expected ',' or '}'")
            }
            Self::ExpectedFlowSequenceSeparator => {
                f.write_str("while parsing a flow sequence, expected ',' or ']'")
            }
            Self::ExpectedBlockSequenceEntry => f.write_str(
                "while parsing a block collection, did not find expected '-' indicator",
            ),
            Self::UndeclaredTagHandle => f.write_str("the handle wasn't declared"),
            Self::MissingIncludeResolver => {
                f.write_str("No include resolver set for parser stack.")
            }
            Self::MultipleDocumentsUnsupported => {
                f.write_str("multiple documents not supported here")
            }
            Self::InputOffsetsWithoutSlice => f.write_str(
                "internal error: input advertised offsets but did not provide a slice",
            ),
            Self::InputSlicingUnavailable => f.write_str(
                "internal error: input advertised slicing but did not provide a slice",
            ),
            Self::ExpectedTagBang => {
                f.write_str("while scanning a tag, did not find expected '!'")
            }
            Self::ExpectedTagDirectiveBang => {
                f.write_str("while parsing a tag directive, did not find expected '!'")
            }
            Self::InvalidGlobalTagCharacter => f.write_str("invalid global tag character"),
            Self::SimpleKeyExpected => f.write_str("simple key expected ':'"),
            Self::InvalidSimpleKey => f.write_str("simple key is no longer valid"),
            Self::InvalidDocumentEnd => {
                f.write_str("invalid content after document end marker")
            }
            Self::InvalidIndentation => f.write_str("invalid indentation"),
            Self::BomInsideDocument => {
                f.write_str("a BOM must not appear inside a document")
            }
            Self::UnexpectedCharacter { character } => {
                write!(f, "unexpected character: `{character}'")
            }
            Self::MissingNextToken => f.write_str("did not find expected next token"),
            Self::TabNotAllowed => f.write_str("tabs disallowed in this context"),
            Self::TabInBlockIndentation => {
                f.write_str("tabs disallowed within this context (block indentation)")
            }
            Self::CommentInterceptedScalar => {
                f.write_str("comment intercepting the multiline text")
            }
            Self::ExpectedWhitespace => f.write_str("expected whitespace"),
            Self::CommentNotSeparated => {
                f.write_str("comments must be separated from other tokens by whitespace")
            }
            Self::InvalidDirectiveTerminator => f.write_str(
                "while scanning a directive, did not find expected comment or line break",
            ),
            Self::MissingYamlVersionSeparator => f.write_str(
                "while scanning a YAML directive, did not find expected digit or '.' character",
            ),
            Self::MissingDirectiveName => f.write_str(
                "while scanning a directive, could not find expected directive name",
            ),
            Self::InvalidDirectiveName => f.write_str(
                "while scanning a directive, found unexpected non-alphabetical character",
            ),
            Self::YamlVersionTooLong => {
                f.write_str("while scanning a YAML directive, found extremely long version number")
            }
            Self::MissingYamlVersion => f.write_str(
                "while scanning a YAML directive, did not find expected version number",
            ),
            Self::InvalidTagDirectiveTerminator => {
                f.write_str("while scanning TAG, did not find expected whitespace or line break")
            }
            Self::InvalidTagTerminator => f.write_str(
                "while scanning a tag, did not find expected whitespace or line break",
            ),
            Self::MissingTagUri => {
                f.write_str("while parsing a tag, did not find expected tag URI")
            }
            Self::UnclosedVerbatimTag => {
                f.write_str("while scanning a verbatim tag, did not find the expected '>'")
            }
            Self::InvalidTagEscape => {
                f.write_str("while parsing a tag, found an invalid escape sequence")
            }
            Self::InvalidTagUtf8LeadingByte => {
                f.write_str("while parsing a tag, found an incorrect leading UTF-8 byte")
            }
            Self::InvalidTagUtf8TrailingByte => {
                f.write_str("while parsing a tag, found an incorrect trailing UTF-8 byte")
            }
            Self::InvalidTagUtf8 => {
                f.write_str("while parsing a tag, found an invalid UTF-8 codepoint")
            }
            Self::MissingAnchorOrAliasName => f.write_str(
                "while scanning an anchor or alias, did not find expected alphabetic or numeric character",
            ),
            Self::MisplacedFlowCollectionEnd => f.write_str("misplaced bracket"),
            Self::MismatchedFlowCollectionEnd { open, close } => {
                write!(f, "mismatched bracket '{open}' closed by '{close}'")
            }
            Self::UnclosedFlowCollection { open } => {
                write!(f, "unclosed bracket '{open}'")
            }
            Self::RecursionLimitExceeded => f.write_str("recursion limit exceeded"),
            Self::BlockEntryInFlowCollection => {
                f.write_str(r#""-" is only valid inside a block"#)
            }
            Self::BlockSequenceEntryNotAllowed => {
                f.write_str("block sequence entries are not allowed in this context")
            }
            Self::InvalidBlockEntryWhitespace => {
                f.write_str("'-' must be followed by a valid YAML whitespace")
            }
            Self::ZeroBlockScalarIndent => f.write_str(
                "while scanning a block scalar, found an indentation indicator equal to 0",
            ),
            Self::InvalidBlockScalarHeader => f.write_str(
                "while scanning a block scalar, did not find expected comment or line break",
            ),
            Self::TabAtBlockScalarStart => {
                f.write_str("a block scalar content cannot start with a tab")
            }
            Self::InvalidBlockScalarIndent => {
                f.write_str("wrongly indented line in block scalar")
            }
            Self::DocumentIndicatorInQuotedScalar => f.write_str(
                "while scanning a quoted scalar, found unexpected document indicator",
            ),
            Self::UnclosedQuotedScalar => f.write_str("unclosed quote"),
            Self::TabInIndentation => f.write_str("tab cannot be used as indentation"),
            Self::InvalidQuotedScalarIndent => {
                f.write_str("invalid indentation in multiline quoted scalar")
            }
            Self::InvalidTrailingSingleQuotedScalar => {
                f.write_str("invalid trailing content after single-quoted scalar")
            }
            Self::InvalidTrailingDoubleQuotedScalar => {
                f.write_str("invalid trailing content after double-quoted scalar")
            }
            Self::UnknownQuotedScalarEscape => {
                f.write_str("while parsing a quoted scalar, found unknown escape character")
            }
            Self::InvalidQuotedScalarHexEscape => f.write_str(
                "while parsing a quoted scalar, did not find expected hexadecimal number",
            ),
            Self::InvalidLowSurrogateHexEscape => f.write_str(
                "while parsing a quoted scalar, did not find expected hexadecimal number for low surrogate",
            ),
            Self::InvalidLowSurrogate => {
                f.write_str("while parsing a quoted scalar, found invalid low surrogate")
            }
            Self::MissingLowSurrogate => f.write_str(
                "while parsing a quoted scalar, found high surrogate without following low surrogate",
            ),
            Self::UnpairedLowSurrogate => {
                f.write_str("while parsing a quoted scalar, found unpaired low surrogate")
            }
            Self::InvalidUnicodeEscape => f.write_str(
                "while parsing a quoted scalar, found invalid Unicode character escape code",
            ),
            Self::InvalidFlowScalarIndent => {
                f.write_str("invalid indentation in flow construct")
            }
            Self::PlainScalarStartsWithDashFlowIndicator => {
                f.write_str("plain scalar cannot start with '-' followed by ,[]{}")
            }
            Self::TabInPlainScalar => {
                f.write_str("while scanning a plain scalar, found a tab")
            }
            Self::UnexpectedEndOfPlainScalar => f.write_str("unexpected end of plain scalar"),
            Self::MappingKeyNotAllowed => {
                f.write_str("mapping keys are not allowed in this context")
            }
            Self::FlowMappingValueAdjacentCollection => {
                f.write_str("':' may not precede any of `[{` in flow mapping")
            }
            Self::InvalidMappingValueWhitespace => {
                f.write_str("':' must be followed by a valid YAML whitespace")
            }
            Self::InvalidColonPlacement => f.write_str("illegal placement of ':' indicator"),
            Self::MappingValueNotAllowed => {
                f.write_str("mapping values are not allowed in this context")
            }
        }
    }
}

#[cfg(not(feature = "error_messages"))]
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("")
    }
}

/// An error that occurred while scanning or parsing YAML.
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct ScanError {
    /// The position at which the error happened in the source.
    mark: Marker,
    /// Machine-readable error category.
    kind: ErrorKind,
}

impl ScanError {
    /// Create a new error from a location and category.
    #[must_use]
    #[cold]
    pub(crate) fn new(loc: Marker, kind: ErrorKind) -> ScanError {
        ScanError { mark: loc, kind }
    }

    #[cold]
    pub(crate) fn into_result<T>(self) -> Result<T, ScanError> {
        Err(self)
    }

    /// Return the marker pointing to the error in the source.
    #[must_use]
    pub fn marker(&self) -> &Marker {
        &self.mark
    }

    /// Return the machine-readable error category.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Render the error category as a human-readable description.
    #[must_use]
    pub fn info(&self) -> String {
        self.kind.to_string()
    }
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at char {} line {} column {}",
            self.info(),
            self.mark.index(),
            self.mark.line(),
            self.mark.col() + 1
        )
    }
}

impl core::error::Error for ScanError {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "error_messages")]
    use alloc::format;
    use alloc::string::ToString;

    use super::{ErrorKind, ScanError};
    use crate::scanner::Marker;

    #[cfg(feature = "error_messages")]
    #[test]
    fn constructor_retains_kind_and_derives_info() {
        let marker = Marker::new(3, 2, 1);
        let error = ScanError::new(marker, ErrorKind::ExpectedWhitespace);

        assert_eq!(error.kind(), ErrorKind::ExpectedWhitespace);
        assert_eq!(error.kind().to_string(), "expected whitespace");
        assert_eq!(error.info(), "expected whitespace");
    }

    #[cfg(feature = "error_messages")]
    #[test]
    fn parameterized_kind_constructs_info() {
        let marker = Marker::new(3, 2, 1);
        let error = ScanError::new(
            marker,
            ErrorKind::MismatchedFlowCollectionEnd {
                open: '[',
                close: '}',
            },
        );

        assert_eq!(error.info(), "mismatched bracket '[' closed by '}'");
        assert_eq!(
            format!("{error}"),
            "mismatched bracket '[' closed by '}' at char 3 line 2 column 2"
        );
    }

    #[cfg(not(feature = "error_messages"))]
    #[test]
    fn disabled_error_messages_are_empty() {
        let marker = Marker::new(3, 2, 1);
        let error = ScanError::new(
            marker,
            ErrorKind::MismatchedFlowCollectionEnd {
                open: '[',
                close: '}',
            },
        );

        assert!(error.kind().to_string().is_empty());
        assert!(error.info().is_empty());
    }
}
