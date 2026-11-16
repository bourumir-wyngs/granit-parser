//! Explicit test projections of parser output.
//!
//! Production enums are inspected through patterns, not compared as whole values. Tests that
//! compare complete traces deliberately project every currently supported field here. These
//! expectations are independent test data, not constructors for production parser output.
#![allow(dead_code)]

use std::{borrow::Cow, fmt::Debug};

use granit_parser::{
    BlockScalarSyntax, Chomping, ErrorKind, Event, InputIoError, Marker, ScalarKind, ScalarStyle,
    ScanError, Span, StructureStyle, Tag, Token, TokenType, YamlVersion,
};
#[cfg(feature = "comments")]
use granit_parser::{Comment, Placement};

/// A deliberate comparable projection used only by tests and fuzz oracles.
pub trait Snapshot {
    type Value: Debug + PartialEq;

    fn snapshot(&self) -> <Self as Snapshot>::Value;
}

/// Project a value, including collections and tuples containing parser output.
pub fn snapshot<T: Snapshot + ?Sized>(value: &T) -> T::Value {
    value.snapshot()
}

macro_rules! clone_snapshot {
    ($($ty:ty),* $(,)?) => {
        $(impl Snapshot for $ty {
            type Value = Self;

            fn snapshot(&self) -> <Self as Snapshot>::Value {
                self.clone()
            }
        })*
    };
}

macro_rules! copy_snapshot {
    ($($ty:ty),* $(,)?) => {
        $(impl Snapshot for $ty {
            type Value = Self;

            fn snapshot(&self) -> <Self as Snapshot>::Value {
                *self
            }
        })*
    };
}

copy_snapshot!(
    (),
    bool,
    char,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    Marker,
    Span,
    BlockScalarSyntax,
    Chomping,
    ScalarKind,
    StructureStyle,
    YamlVersion,
);
clone_snapshot!(String, ScanError, ErrorKind, InputIoError, Tag);
#[cfg(feature = "comments")]
copy_snapshot!(Placement);

impl Snapshot for str {
    type Value = String;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.to_owned()
    }
}

impl<T: Snapshot + ?Sized> Snapshot for &T {
    type Value = T::Value;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        T::snapshot(self)
    }
}

impl<T: Snapshot + ?Sized> Snapshot for Box<T> {
    type Value = T::Value;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.as_ref().snapshot()
    }
}

impl<T: Snapshot + ToOwned + ?Sized> Snapshot for Cow<'_, T> {
    type Value = T::Value;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.as_ref().snapshot()
    }
}

impl<T: Snapshot> Snapshot for [T] {
    type Value = Vec<T::Value>;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.iter().map(Snapshot::snapshot).collect()
    }
}

impl<T: Snapshot, const N: usize> Snapshot for [T; N] {
    type Value = Vec<T::Value>;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.as_slice().snapshot()
    }
}

impl<T: Snapshot> Snapshot for Vec<T> {
    type Value = Vec<T::Value>;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.as_slice().snapshot()
    }
}

impl<T: Snapshot> Snapshot for Option<T> {
    type Value = Option<T::Value>;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.as_ref().map(Snapshot::snapshot)
    }
}

impl<T: Snapshot, E: Snapshot> Snapshot for Result<T, E> {
    type Value = Result<T::Value, E::Value>;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        match self {
            Ok(value) => Ok(value.snapshot()),
            Err(error) => Err(error.snapshot()),
        }
    }
}

impl<T: Snapshot> Snapshot for std::ops::Range<T> {
    type Value = std::ops::Range<T::Value>;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.start.snapshot()..self.end.snapshot()
    }
}

impl<T: Snapshot> Snapshot for std::ops::RangeInclusive<T> {
    type Value = std::ops::RangeInclusive<T::Value>;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.start().snapshot()..=self.end().snapshot()
    }
}

macro_rules! tuple_snapshot {
    ($($ty:ident:$index:tt),+ $(,)?) => {
        impl<$($ty: Snapshot),+> Snapshot for ($($ty,)+) {
            type Value = ($($ty::Value,)+);

            fn snapshot(&self) -> <Self as Snapshot>::Value {
                ($(self.$index.snapshot(),)+)
            }
        }
    };
}

tuple_snapshot!(A:0);
tuple_snapshot!(A:0, B:1);
tuple_snapshot!(A:0, B:1, C:2);
tuple_snapshot!(A:0, B:1, C:2, D:3);
tuple_snapshot!(A:0, B:1, C:2, D:3, E:4);
tuple_snapshot!(A:0, B:1, C:2, D:3, E:4, F:5);

/// All currently observed scalar syntax, including omitted-node metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedStyle {
    Plain { implicit: bool },
    SingleQuoted,
    DoubleQuoted,
    Literal { syntax: BlockScalarSyntax },
    Folded { syntax: BlockScalarSyntax },
}

impl ExpectedStyle {
    pub const fn plain() -> Self {
        Self::Plain { implicit: false }
    }

    pub const fn implicit_plain() -> Self {
        Self::Plain { implicit: true }
    }

    pub const fn literal(syntax: BlockScalarSyntax) -> Self {
        Self::Literal { syntax }
    }

    pub const fn folded(syntax: BlockScalarSyntax) -> Self {
        Self::Folded { syntax }
    }
}

impl From<&ScalarStyle> for ExpectedStyle {
    fn from(style: &ScalarStyle) -> Self {
        match style {
            ScalarStyle::Plain { implicit, .. } => Self::Plain {
                implicit: *implicit,
            },
            ScalarStyle::SingleQuoted { .. } => Self::SingleQuoted,
            ScalarStyle::DoubleQuoted { .. } => Self::DoubleQuoted,
            ScalarStyle::Literal { syntax, .. } => Self::Literal { syntax: *syntax },
            ScalarStyle::Folded { syntax, .. } => Self::Folded { syntax: *syntax },
            _ => panic!("add the new scalar style to the test projection"),
        }
    }
}

impl From<ScalarStyle> for ExpectedStyle {
    fn from(style: ScalarStyle) -> Self {
        Self::from(&style)
    }
}

impl Snapshot for ScalarStyle {
    type Value = ExpectedStyle;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.into()
    }
}

/// Every current event field, represented as independently constructible test data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpectedEvent {
    StreamStart,
    StreamEnd,
    DocumentStart {
        explicit: bool,
        version: Option<YamlVersion>,
    },
    DocumentEnd {
        explicit: bool,
    },
    Alias {
        anchor_id: usize,
    },
    Anchor {
        anchor_id: usize,
        name: String,
    },
    UnresolvedAlias {
        name: String,
    },
    #[cfg(feature = "comments")]
    Comment {
        value: String,
        placement: Placement,
    },
    Scalar {
        value: String,
        style: ExpectedStyle,
        anchor_id: usize,
        tag: Option<Tag>,
    },
    SequenceStart {
        style: StructureStyle,
        anchor_id: usize,
        tag: Option<Tag>,
    },
    SequenceEnd,
    MappingStart {
        style: StructureStyle,
        anchor_id: usize,
        tag: Option<Tag>,
    },
    MappingEnd,
}

impl ExpectedEvent {
    pub const fn is_comment(&self) -> bool {
        #[cfg(feature = "comments")]
        {
            matches!(self, Self::Comment { .. })
        }
        #[cfg(not(feature = "comments"))]
        {
            let _ = self;
            false
        }
    }

    pub const fn document_start(explicit: bool, version: Option<YamlVersion>) -> Self {
        Self::DocumentStart { explicit, version }
    }

    pub const fn document_end(explicit: bool) -> Self {
        Self::DocumentEnd { explicit }
    }

    pub const fn alias(anchor_id: usize) -> Self {
        Self::Alias { anchor_id }
    }

    pub fn anchor(anchor_id: usize, name: Cow<'_, str>) -> Self {
        Self::Anchor {
            anchor_id,
            name: name.into_owned(),
        }
    }

    pub fn unresolved_alias(name: Cow<'_, str>) -> Self {
        Self::UnresolvedAlias {
            name: name.into_owned(),
        }
    }

    #[cfg(feature = "comments")]
    pub fn comment(value: Cow<'_, str>, placement: Placement) -> Self {
        Self::Comment {
            value: value.into_owned(),
            placement,
        }
    }

    /// Construct scalar expectation data.
    ///
    /// * `value`: decoded text expected from the parser.
    /// * `style`: expected notation and block metadata.
    /// * `implicit`: expected omission flag for plain notation.
    /// * `anchor_id`: expected anchor ID, or zero when absent.
    /// * `tag`: expected resolved tag, if present.
    ///
    /// Returns a comparable expectation; it does not construct a parser event.
    pub fn new_scalar(
        value: Cow<'_, str>,
        style: impl Into<ExpectedStyle>,
        implicit: bool,
        anchor_id: usize,
        tag: Option<Cow<'_, Tag>>,
    ) -> Self {
        let mut style = style.into();
        if let ExpectedStyle::Plain { implicit: omitted } = &mut style {
            *omitted = implicit;
        } else {
            assert!(!implicit, "only plain scalars can be omitted");
        }
        Self::Scalar {
            value: value.into_owned(),
            style,
            anchor_id,
            tag: tag.map(Cow::into_owned),
        }
    }

    /// Construct a sequence-start expectation from `style`, `anchor_id`, and optional `tag`.
    /// Returns independently comparable expectation data.
    pub fn sequence_start(
        style: StructureStyle,
        anchor_id: usize,
        tag: Option<Cow<'_, Tag>>,
    ) -> Self {
        Self::SequenceStart {
            style,
            anchor_id,
            tag: tag.map(Cow::into_owned),
        }
    }

    /// Construct a mapping-start expectation from `style`, `anchor_id`, and optional `tag`.
    /// Returns independently comparable expectation data.
    pub fn mapping_start(
        style: StructureStyle,
        anchor_id: usize,
        tag: Option<Cow<'_, Tag>>,
    ) -> Self {
        Self::MappingStart {
            style,
            anchor_id,
            tag: tag.map(Cow::into_owned),
        }
    }
}

impl From<&Event<'_>> for ExpectedEvent {
    fn from(event: &Event<'_>) -> Self {
        match event {
            Event::StreamStart { .. } => Self::StreamStart,
            Event::StreamEnd { .. } => Self::StreamEnd,
            Event::DocumentStart {
                explicit, version, ..
            } => Self::DocumentStart {
                explicit: *explicit,
                version: *version,
            },
            Event::DocumentEnd { explicit, .. } => Self::DocumentEnd {
                explicit: *explicit,
            },
            Event::Alias { anchor_id, .. } => Self::Alias {
                anchor_id: *anchor_id,
            },
            Event::Anchor {
                anchor_id, name, ..
            } => Self::Anchor {
                anchor_id: *anchor_id,
                name: name.to_string(),
            },
            Event::UnresolvedAlias { name, .. } => Self::UnresolvedAlias {
                name: name.to_string(),
            },
            #[cfg(feature = "comments")]
            Event::Comment {
                value, placement, ..
            } => Self::Comment {
                value: value.to_string(),
                placement: *placement,
            },
            Event::Scalar {
                value,
                style,
                anchor_id,
                tag,
                ..
            } => Self::Scalar {
                value: value.to_string(),
                style: style.into(),
                anchor_id: *anchor_id,
                tag: tag.as_deref().cloned(),
            },
            Event::SequenceStart {
                style,
                anchor_id,
                tag,
                ..
            } => Self::SequenceStart {
                style: *style,
                anchor_id: *anchor_id,
                tag: tag.as_deref().cloned(),
            },
            Event::SequenceEnd { .. } => Self::SequenceEnd,
            Event::MappingStart {
                style,
                anchor_id,
                tag,
                ..
            } => Self::MappingStart {
                style: *style,
                anchor_id: *anchor_id,
                tag: tag.as_deref().cloned(),
            },
            Event::MappingEnd { .. } => Self::MappingEnd,
            _ => panic!("add the new event variant to the test projection"),
        }
    }
}

impl Snapshot for Event<'_> {
    type Value = ExpectedEvent;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.into()
    }
}

#[cfg(feature = "comments")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedComment {
    pub text: String,
    pub placement: Placement,
}

#[cfg(feature = "comments")]
impl From<&Comment<'_>> for ExpectedComment {
    fn from(comment: &Comment<'_>) -> Self {
        Self {
            text: comment.text().to_owned(),
            placement: comment.placement(),
        }
    }
}

#[cfg(feature = "comments")]
impl Snapshot for Comment<'_> {
    type Value = ExpectedComment;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.into()
    }
}

/// Every field currently exposed by a scanner token's payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpectedTokenType {
    StreamStart,
    StreamEnd,
    VersionDirective {
        major: u32,
        minor: u32,
    },
    TagDirective {
        handle: String,
        prefix: String,
    },
    DocumentStart,
    DocumentEnd,
    BlockSequenceStart,
    BlockMappingStart,
    BlockEnd,
    FlowSequenceStart,
    FlowSequenceEnd,
    FlowMappingStart,
    FlowMappingEnd,
    BlockEntry,
    FlowEntry,
    Key,
    Value,
    Alias {
        name: String,
    },
    Anchor {
        name: String,
    },
    Tag {
        handle: String,
        suffix: String,
    },
    Scalar {
        style: ExpectedStyle,
        value: String,
    },
    #[cfg(feature = "comments")]
    Comment {
        comment: ExpectedComment,
    },
    ReservedDirective {
        name: String,
        parameters: Vec<String>,
    },
}

impl ExpectedTokenType {
    pub const fn version_directive(major: u32, minor: u32) -> Self {
        Self::VersionDirective { major, minor }
    }

    pub fn tag_directive(handle: Cow<'_, str>, prefix: Cow<'_, str>) -> Self {
        Self::TagDirective {
            handle: handle.into_owned(),
            prefix: prefix.into_owned(),
        }
    }

    pub fn alias(name: Cow<'_, str>) -> Self {
        Self::Alias {
            name: name.into_owned(),
        }
    }

    pub fn anchor(name: Cow<'_, str>) -> Self {
        Self::Anchor {
            name: name.into_owned(),
        }
    }

    pub fn tag(handle: Cow<'_, str>, suffix: Cow<'_, str>) -> Self {
        Self::Tag {
            handle: handle.into_owned(),
            suffix: suffix.into_owned(),
        }
    }

    pub fn scalar(style: impl Into<ExpectedStyle>, value: Cow<'_, str>) -> Self {
        Self::Scalar {
            style: style.into(),
            value: value.into_owned(),
        }
    }

    #[cfg(feature = "comments")]
    pub fn comment(comment: Comment<'_>) -> Self {
        let placement = comment.placement();
        Self::Comment {
            comment: ExpectedComment {
                text: comment.into_text().into_owned(),
                placement,
            },
        }
    }

    pub fn reserved_directive(name: String, parameters: Vec<String>) -> Self {
        Self::ReservedDirective { name, parameters }
    }
}

impl From<&TokenType<'_>> for ExpectedTokenType {
    fn from(token: &TokenType<'_>) -> Self {
        match token {
            TokenType::StreamStart { .. } => Self::StreamStart,
            TokenType::StreamEnd { .. } => Self::StreamEnd,
            TokenType::VersionDirective { major, minor, .. } => Self::VersionDirective {
                major: *major,
                minor: *minor,
            },
            TokenType::TagDirective { handle, prefix, .. } => Self::TagDirective {
                handle: handle.to_string(),
                prefix: prefix.to_string(),
            },
            TokenType::DocumentStart { .. } => Self::DocumentStart,
            TokenType::DocumentEnd { .. } => Self::DocumentEnd,
            TokenType::BlockSequenceStart { .. } => Self::BlockSequenceStart,
            TokenType::BlockMappingStart { .. } => Self::BlockMappingStart,
            TokenType::BlockEnd { .. } => Self::BlockEnd,
            TokenType::FlowSequenceStart { .. } => Self::FlowSequenceStart,
            TokenType::FlowSequenceEnd { .. } => Self::FlowSequenceEnd,
            TokenType::FlowMappingStart { .. } => Self::FlowMappingStart,
            TokenType::FlowMappingEnd { .. } => Self::FlowMappingEnd,
            TokenType::BlockEntry { .. } => Self::BlockEntry,
            TokenType::FlowEntry { .. } => Self::FlowEntry,
            TokenType::Key { .. } => Self::Key,
            TokenType::Value { .. } => Self::Value,
            TokenType::Alias { name, .. } => Self::Alias {
                name: name.to_string(),
            },
            TokenType::Anchor { name, .. } => Self::Anchor {
                name: name.to_string(),
            },
            TokenType::Tag { handle, suffix, .. } => Self::Tag {
                handle: handle.to_string(),
                suffix: suffix.to_string(),
            },
            TokenType::Scalar { style, value, .. } => Self::Scalar {
                style: style.into(),
                value: value.to_string(),
            },
            #[cfg(feature = "comments")]
            TokenType::Comment { comment, .. } => Self::Comment {
                comment: comment.into(),
            },
            TokenType::ReservedDirective {
                name, parameters, ..
            } => Self::ReservedDirective {
                name: name.clone(),
                parameters: parameters.clone(),
            },
            _ => panic!("add the new scanner token variant to the test projection"),
        }
    }
}

impl From<TokenType<'_>> for ExpectedTokenType {
    fn from(token: TokenType<'_>) -> Self {
        Self::from(&token)
    }
}

impl Snapshot for TokenType<'_> {
    type Value = ExpectedTokenType;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.into()
    }
}

/// A token expectation retaining the existing span equality contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedToken(pub Span, pub ExpectedTokenType);

impl ExpectedToken {
    pub fn new(span: Span, token_type: impl Into<ExpectedTokenType>) -> Self {
        Self(span, token_type.into())
    }
}

impl From<&Token<'_>> for ExpectedToken {
    fn from(token: &Token<'_>) -> Self {
        Self(token.span(), token.token_type().into())
    }
}

impl Snapshot for Token<'_> {
    type Value = ExpectedToken;

    fn snapshot(&self) -> <Self as Snapshot>::Value {
        self.into()
    }
}

copy_snapshot!(ExpectedStyle);
clone_snapshot!(ExpectedEvent, ExpectedTokenType, ExpectedToken);
#[cfg(feature = "comments")]
clone_snapshot!(ExpectedComment);
