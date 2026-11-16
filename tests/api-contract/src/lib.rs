//! An independent downstream consumer of the public parser API.
//!
//! This package does not inherit the library's test-only feature activation. Its doctests
//! therefore check the comparison contract seen by ordinary dependencies.

#[cfg_attr(
    not(feature = "test-equality"),
    doc = r"
Default consumers classify variants with patterns rather than equality.

```compile_fail,E0369
use granit_parser::ScalarStyle;
fn equal(left: ScalarStyle, right: ScalarStyle) -> bool { left == right }
```

```compile_fail,E0369
use granit_parser::Event;
fn equal(left: Event<'static>, right: Event<'static>) -> bool { left == right }
```

```compile_fail,E0369
use granit_parser::TokenType;
fn equal(left: TokenType<'static>, right: TokenType<'static>) -> bool { left == right }
```

```compile_fail,E0369
use granit_parser::Token;
fn equal(left: Token<'static>, right: Token<'static>) -> bool { left == right }
```

```compile_fail,E0369
use granit_parser::BlockScalarSyntax;
fn equal(left: BlockScalarSyntax, right: BlockScalarSyntax) -> bool { left == right }
```

"
)]
#[doc = r"
Hashing and ordering remain unavailable even when test equality is enabled.

```compile_fail,E0277
use granit_parser::ScalarStyle;
fn require_trait<T: core::hash::Hash>() {}
require_trait::<ScalarStyle>();
```

```compile_fail,E0277
use granit_parser::Event;
fn require_trait<T: core::hash::Hash>() {}
require_trait::<Event<'static>>();
```

```compile_fail,E0277
use granit_parser::TokenType;
fn require_trait<T: core::hash::Hash>() {}
require_trait::<TokenType<'static>>();
```

```compile_fail,E0277
use granit_parser::Token;
fn require_trait<T: core::hash::Hash>() {}
require_trait::<Token<'static>>();
```

```compile_fail,E0277
use granit_parser::BlockScalarSyntax;
fn require_trait<T: core::hash::Hash>() {}
require_trait::<BlockScalarSyntax>();
```

```compile_fail,E0277
use granit_parser::ScalarStyle;
fn require_trait<T: Ord>() {}
require_trait::<ScalarStyle>();
```

```compile_fail,E0277
use granit_parser::Event;
fn require_trait<T: Ord>() {}
require_trait::<Event<'static>>();
```

```compile_fail,E0277
use granit_parser::TokenType;
fn require_trait<T: Ord>() {}
require_trait::<TokenType<'static>>();
```

```compile_fail,E0277
use granit_parser::Token;
fn require_trait<T: Ord>() {}
require_trait::<Token<'static>>();
```

```compile_fail,E0277
use granit_parser::BlockScalarSyntax;
fn require_trait<T: Ord>() {}
require_trait::<BlockScalarSyntax>();
```

```compile_fail,E0277
use granit_parser::ScalarStyle;
fn require_trait<T: PartialOrd>() {}
require_trait::<ScalarStyle>();
```

```compile_fail,E0277
use granit_parser::Event;
fn require_trait<T: PartialOrd>() {}
require_trait::<Event<'static>>();
```

```compile_fail,E0277
use granit_parser::TokenType;
fn require_trait<T: PartialOrd>() {}
require_trait::<TokenType<'static>>();
```

```compile_fail,E0277
use granit_parser::Token;
fn require_trait<T: PartialOrd>() {}
require_trait::<Token<'static>>();
```

```compile_fail,E0277
use granit_parser::BlockScalarSyntax;
fn require_trait<T: PartialOrd>() {}
require_trait::<BlockScalarSyntax>();
```

"]
pub struct ComparisonContract;

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use granit_parser::{
        BlockScalarSyntax, Chomping, Event, Marker, ReplayParser, ScalarStyle, Span,
        StructureStyle, Tag, Token, TokenType,
    };

    #[test]
    fn synthetic_stream_can_be_replayed_without_equality() {
        let span = Span::empty(Marker::new(0, 1, 0));
        let events = vec![
            Event::stream_start(),
            Event::document_start(true, None),
            Event::mapping_start(StructureStyle::Block, 0, None),
            Event::new_scalar("key".into(), ScalarStyle::plain(), 0, None),
            Event::new_scalar("~".into(), ScalarStyle::implicit_plain(), 7, None),
            Event::mapping_end(),
            Event::document_end(false),
            Event::stream_end(),
        ];
        let mut replay =
            ReplayParser::new(events.into_iter().map(|event| (event, span)).collect(), 0);
        let replayed = replay.by_ref().collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(replayed.len(), 8);
        assert!(matches!(replayed[0].0, Event::StreamStart { .. }));
        assert!(matches!(replayed[4].0,
            Event::Scalar {
                ref value,
                style: ScalarStyle::Plain { implicit: true, .. },
                anchor_id: 7,
                ..
            } if value == "~"
        ));
        assert!(matches!(replayed[7].0, Event::StreamEnd { .. }));
        assert_eq!(replay.anchor_offset(), 8);
    }

    #[test]
    fn reconstructed_budget_events_preserve_borrowed_properties() {
        // Alias budget checks rebuild events from buffered values and retain explicit tags.
        let text = String::from("<<");
        let tag = Tag::new("tag:yaml.org,2002:", "merge");
        let event = Event::new_scalar(
            Cow::Borrowed(&text),
            ScalarStyle::plain(),
            3,
            Some(Cow::Borrowed(&tag)),
        );
        let Event::Scalar {
            value,
            style,
            anchor_id,
            tag: Some(actual_tag),
            ..
        } = event
        else {
            panic!("scalar constructor returned a different variant");
        };
        assert!(matches!(value, Cow::Borrowed(value) if std::ptr::eq(value, text.as_str())));
        assert!(matches!(
            style,
            ScalarStyle::Plain {
                implicit: false,
                ..
            }
        ));
        assert_eq!(anchor_id, 3);
        assert!(matches!(actual_tag, Cow::Borrowed(value) if std::ptr::eq(value, &tag)));

        let sequence = Event::sequence_start(StructureStyle::Flow, 4, Some(Cow::Borrowed(&tag)));
        let mapping = Event::mapping_start(StructureStyle::Block, 5, Some(Cow::Borrowed(&tag)));
        assert!(matches!(
            sequence,
            Event::SequenceStart {
                style: StructureStyle::Flow,
                anchor_id: 4,
                ..
            }
        ));
        assert!(matches!(
            mapping,
            Event::MappingStart {
                style: StructureStyle::Block,
                anchor_id: 5,
                ..
            }
        ));
        assert!(std::ptr::eq(sequence.tag().unwrap(), &tag));
        assert!(std::ptr::eq(mapping.tag().unwrap(), &tag));
    }

    /// Convert a borrowed scalar event to an owned event that outlives its source buffers.
    fn own_scalar(event: Event<'_>) -> Event<'static> {
        let Event::Scalar {
            value,
            style,
            anchor_id,
            tag,
            ..
        } = event
        else {
            panic!("expected a scalar event");
        };
        Event::new_scalar(
            Cow::Owned(value.into_owned()),
            style,
            anchor_id,
            tag.map(|tag| Cow::Owned(tag.into_owned())),
        )
    }

    #[test]
    fn ownership_conversion_preserves_scalar_metadata() {
        let owned = {
            let text = String::from("borrowed value");
            let tag = Tag::new("!", "Example");
            let mut syntax = BlockScalarSyntax::new(Chomping::Keep, Some(2), 4).unwrap();
            syntax.header_chars_before_span = 9;
            own_scalar(Event::new_scalar(
                Cow::Borrowed(&text),
                ScalarStyle::literal(syntax),
                6,
                Some(Cow::Borrowed(&tag)),
            ))
        };
        let Event::Scalar {
            value,
            style,
            anchor_id,
            tag,
            ..
        } = owned
        else {
            panic!("expected an owned scalar");
        };
        assert!(matches!(value, Cow::Owned(ref value) if value == "borrowed value"));
        assert_eq!(anchor_id, 6);
        assert!(matches!(tag, Some(Cow::Owned(ref tag)) if tag.original() == "!Example"));
        let ScalarStyle::Literal { syntax, .. } = style else {
            panic!("literal style was not retained");
        };
        assert_eq!(syntax.chomping, Chomping::Keep);
        assert_eq!(syntax.indentation_indicator, Some(2));
        assert_eq!(syntax.content_indent, 4);
        assert_eq!(syntax.header_chars_before_span, 9);
    }

    #[test]
    fn fieldless_variant_constructors_remain_available() {
        macro_rules! check {
            ($ty:ident, $($constructor:ident => $variant:ident),+ $(,)?) => {
                $(assert!(matches!($ty::$constructor(), $ty::$variant { .. }));)+
            };
        }
        check!(Event, stream_start => StreamStart, stream_end => StreamEnd,
            sequence_end => SequenceEnd, mapping_end => MappingEnd);
        check!(ScalarStyle, single_quoted => SingleQuoted, double_quoted => DoubleQuoted);
        check!(TokenType,
            stream_start => StreamStart, stream_end => StreamEnd,
            document_start => DocumentStart, document_end => DocumentEnd,
            block_sequence_start => BlockSequenceStart, block_mapping_start => BlockMappingStart,
            block_end => BlockEnd, flow_sequence_start => FlowSequenceStart,
            flow_sequence_end => FlowSequenceEnd, flow_mapping_start => FlowMappingStart,
            flow_mapping_end => FlowMappingEnd, block_entry => BlockEntry,
            flow_entry => FlowEntry, key => Key, value => Value);
    }

    #[test]
    fn token_constructor_retains_span_and_payload() {
        let span = Span::empty(Marker::new(4, 2, 1));
        let token = Token::new(
            span,
            TokenType::scalar(ScalarStyle::double_quoted(), "value".into()),
        );
        let (actual_span, payload) = token.into_parts();
        assert_eq!(actual_span, span);
        assert!(matches!(payload,
            TokenType::Scalar { value, style: ScalarStyle::DoubleQuoted { .. }, .. }
                if value == "value"
        ));
    }

    #[cfg(feature = "comments")]
    #[test]
    fn comment_constructors_retain_text_and_placement() {
        use granit_parser::{Comment, Placement};
        let event = Event::comment(" payload".into(), Placement::Right);
        let token = TokenType::comment(Comment::new(" payload").with_placement(Placement::Right));
        assert!(
            matches!(event, Event::Comment { value, placement: Placement::Right, .. } if value == " payload")
        );
        assert!(matches!(token, TokenType::Comment { comment, .. }
            if comment.text() == " payload" && comment.placement() == Placement::Right));
    }

    #[cfg(feature = "test-equality")]
    #[test]
    fn opt_in_equality_compares_real_events_and_tokens() {
        use granit_parser::{BufferedInput, Parser, Scanner, StrInput};
        fn require_eq<T: Eq>() {}
        let source = "key: !note &a |2- # žąsis\n  hello\n";
        let string = Parser::new_from_str(source)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let streaming = Parser::new_from_iter(source.chars())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(string, streaming);
        let string_tokens = Scanner::new(StrInput::new(source))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let streaming_tokens = Scanner::new(BufferedInput::new(source.chars()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(string_tokens, streaming_tokens);

        assert_ne!(ScalarStyle::plain(), ScalarStyle::implicit_plain());
        let written = Event::new_scalar("~".into(), ScalarStyle::plain(), 0, None);
        let omitted = Event::new_scalar("~".into(), ScalarStyle::implicit_plain(), 0, None);
        assert_ne!(written, omitted);

        require_eq::<ScalarStyle>();
        require_eq::<Event<'static>>();
        require_eq::<TokenType<'static>>();
        require_eq::<Token<'static>>();
        require_eq::<BlockScalarSyntax>();
        assert_eq!(
            written,
            Event::new_scalar("~".into(), ScalarStyle::plain(), 0, None)
        );
        let span = Span::empty(Marker::new(0, 1, 0));
        let token = Token::new(
            span,
            TokenType::scalar(ScalarStyle::plain(), "value".into()),
        );
        assert_eq!(
            token,
            Token::new(
                span,
                TokenType::scalar(ScalarStyle::plain(), "value".into())
            )
        );
    }

    #[cfg(feature = "test-equality")]
    #[test]
    fn opt_in_syntax_equality_tracks_fields_and_normalizes_optional_coordinates() {
        let syntax = BlockScalarSyntax::new(Chomping::Keep, Some(2), 4).unwrap();
        let mut bytes = syntax;
        bytes.header_bytes_before_span = 12;
        assert_eq!(syntax, bytes);
        let mut indentation = syntax;
        indentation.content_indent = 5;
        assert_ne!(syntax, indentation);
        let mut order = syntax;
        order.digit_first = true;
        assert_ne!(syntax, order);
    }
}
