//! An independent downstream consumer of the public parser API.
//!
//! This package does not inherit the library's test-only feature activation. Its doctests
//! therefore check the comparison contract seen by ordinary dependencies.

#[cfg_attr(
    not(feature = "test_equality"),
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


"]
pub struct ComparisonContract;

#[cfg(feature = "extended_details")]
#[cfg_attr(
    not(feature = "test_equality"),
    doc = r"
```compile_fail,E0369
use granit_parser::BlockScalarSyntax;
fn equal(left: BlockScalarSyntax, right: BlockScalarSyntax) -> bool { left == right }
```
"
)]
#[doc = r"
```compile_fail,E0277
use granit_parser::BlockScalarSyntax;
fn require_trait<T: core::hash::Hash>() {}
require_trait::<BlockScalarSyntax>();
```

```compile_fail,E0277
use granit_parser::BlockScalarSyntax;
fn require_trait<T: Ord>() {}
require_trait::<BlockScalarSyntax>();
```

```compile_fail,E0277
use granit_parser::BlockScalarSyntax;
fn require_trait<T: PartialOrd>() {}
require_trait::<BlockScalarSyntax>();
```
"]
pub struct ExtendedComparisonContract;

#[cfg(not(feature = "extended_details"))]
#[doc = r"
Metadata types, fields, and builders are absent when extended details are disabled.

```compile_fail,E0432
use granit_parser::BlockScalarSyntax;
```

```compile_fail,E0432
use granit_parser::Chomping;
```

```compile_fail,E0026
use granit_parser::ScalarStyle;
fn inspect(style: ScalarStyle) {
    if let ScalarStyle::Plain { implicit, .. } = style {}
}
```

```compile_fail,E0026
use granit_parser::ScalarStyle;
fn inspect(style: ScalarStyle) {
    if let ScalarStyle::Literal { syntax, .. } = style {}
}
```

```compile_fail,E0026
use granit_parser::ScalarStyle;
fn inspect(style: ScalarStyle) {
    if let ScalarStyle::Folded { syntax, .. } = style {}
}
```

```compile_fail,E0026
use granit_parser::Event;
fn inspect(event: Event<'_>) {
    if let Event::DocumentEnd { explicit, .. } = event {}
}
```

```compile_fail,E0599
use granit_parser::ScalarStyle;
let _ = ScalarStyle::plain().with_implicit(true);
```

```compile_fail,E0599
use granit_parser::Event;
let _ = Event::document_end().with_explicit_end(true);
```

```compile_fail,E0599
use granit_parser::ScalarStyle;
let _ = ScalarStyle::literal().block_syntax();
```
"]
pub struct DisabledDetailsContract;

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    #[cfg(feature = "extended_details")]
    use granit_parser::{BlockScalarSyntax, Chomping};
    use granit_parser::{
        Event, Marker, ReplayParser, ScalarStyle, Span, StructureStyle, Tag, Token, TokenType,
    };

    #[test]
    fn synthetic_stream_can_be_replayed_without_equality() {
        let span = Span::empty(Marker::new(0, 1, 0));
        let events = vec![
            Event::stream_start(),
            Event::document_start(true, None),
            Event::mapping_start(StructureStyle::Block, 0, None),
            Event::new_scalar("key".into(), ScalarStyle::plain(), 0, None),
            Event::new_scalar("~".into(), ScalarStyle::plain(), 7, None),
            Event::mapping_end(),
            Event::document_end(),
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
                style: ScalarStyle::Plain { .. },
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
        assert!(matches!(style, ScalarStyle::Plain { .. }));
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
            let style = ScalarStyle::literal();
            #[cfg(feature = "extended_details")]
            let style = {
                let mut syntax = BlockScalarSyntax::new(Chomping::Keep, Some(2), 4).unwrap();
                syntax.header_chars_before_span = 9;
                style.with_block_syntax(syntax)
            };
            own_scalar(Event::new_scalar(
                Cow::Borrowed(&text),
                style,
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
        assert!(matches!(style, ScalarStyle::Literal { .. }));
        #[cfg(feature = "extended_details")]
        {
            let syntax = style
                .block_syntax()
                .expect("literal syntax was not retained");
            assert_eq!(syntax.chomping, Chomping::Keep);
            assert_eq!(syntax.indentation_indicator, Some(2));
            assert_eq!(syntax.content_indent, 4);
            assert_eq!(syntax.header_chars_before_span, 9);
        }
    }

    #[test]
    fn fieldless_variant_constructors_remain_available() {
        macro_rules! check {
            ($ty:ident, $($constructor:ident => $variant:ident),+ $(,)?) => {
                $(assert!(matches!($ty::$constructor(), $ty::$variant { .. }));)+
            };
        }
        check!(Event, stream_start => StreamStart, stream_end => StreamEnd,
            sequence_end => SequenceEnd, mapping_end => MappingEnd, document_end => DocumentEnd);
        check!(ScalarStyle, plain => Plain, single_quoted => SingleQuoted,
            double_quoted => DoubleQuoted, literal => Literal, folded => Folded);
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

    #[cfg(feature = "test_equality")]
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

        #[cfg(feature = "extended_details")]
        {
            // Whole-output equality deliberately omits extended details. Check those
            // separately so this backend comparison also detects metadata regressions.
            for ((left, _), (right, _)) in string.iter().zip(&streaming) {
                match (left, right) {
                    (Event::Scalar { style: left, .. }, Event::Scalar { style: right, .. }) => {
                        assert_eq!(
                            matches!(left, ScalarStyle::Plain { implicit: true, .. }),
                            matches!(right, ScalarStyle::Plain { implicit: true, .. })
                        );
                        assert_eq!(left.block_syntax(), right.block_syntax());
                    }
                    (
                        Event::DocumentEnd { explicit: left, .. },
                        Event::DocumentEnd {
                            explicit: right, ..
                        },
                    ) => assert_eq!(left, right),
                    _ => {}
                }
            }
            for (left, right) in string_tokens.iter().zip(&streaming_tokens) {
                if let (
                    TokenType::Scalar { style: left, .. },
                    TokenType::Scalar { style: right, .. },
                ) = (left.token_type(), right.token_type())
                {
                    assert_eq!(
                        matches!(left, ScalarStyle::Plain { implicit: true, .. }),
                        matches!(right, ScalarStyle::Plain { implicit: true, .. })
                    );
                    assert_eq!(left.block_syntax(), right.block_syntax());
                }
            }
        }

        require_eq::<ScalarStyle>();
        require_eq::<Event<'static>>();
        require_eq::<TokenType<'static>>();
        require_eq::<Token<'static>>();
        #[cfg(feature = "extended_details")]
        require_eq::<BlockScalarSyntax>();
        let written = Event::new_scalar("~".into(), ScalarStyle::plain(), 0, None);
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

    #[cfg(feature = "test_equality")]
    fn last_scalar(source: &str) -> Event<'_> {
        granit_parser::Parser::new_from_str(source)
            .map(Result::unwrap)
            .map(|(event, _)| event)
            .filter(|event| matches!(event, Event::Scalar { .. }))
            .last()
            .expect("source should contain a scalar")
    }

    #[cfg(feature = "test_equality")]
    #[test]
    fn constructor_equality_is_independent_of_extended_details() {
        let omitted = last_scalar("key:");
        let written = last_scalar("~");
        let synthetic = Event::new_scalar("~".into(), ScalarStyle::plain(), 0, None);
        assert_eq!(synthetic, omitted);
        assert_eq!(omitted, synthetic);
        assert_eq!(synthetic, written);
        assert_eq!(written, omitted);
        #[cfg(feature = "extended_details")]
        assert!(matches!(
            omitted,
            Event::Scalar {
                style: ScalarStyle::Plain { implicit: true, .. },
                ..
            }
        ));

        for (source, style) in [
            ("|\n  value\n", ScalarStyle::literal()),
            (">\n  value\n", ScalarStyle::folded()),
        ] {
            let parsed = last_scalar(source);
            assert_eq!(parsed, Event::new_scalar("value\n".into(), style, 0, None));
            #[cfg(feature = "extended_details")]
            assert!(
                matches!(parsed, Event::Scalar { style, .. } if style.block_syntax().is_some())
            );

            let token = granit_parser::Scanner::new(granit_parser::StrInput::new(source))
                .map(Result::unwrap)
                .find(|token| matches!(token.token_type(), TokenType::Scalar { .. }))
                .unwrap();
            let synthetic_type = TokenType::scalar(style, "value\n".into());
            assert_eq!(token.token_type(), &synthetic_type);
            assert_eq!(token, Token::new(token.span(), synthetic_type));
        }

        let explicit_end = granit_parser::Parser::new_from_str("value\n...\n")
            .map(Result::unwrap)
            .map(|(event, _)| event)
            .find(|event| matches!(event, Event::DocumentEnd { .. }))
            .unwrap();
        assert_eq!(explicit_end, Event::document_end());
        #[cfg(feature = "extended_details")]
        assert!(matches!(
            explicit_end,
            Event::DocumentEnd { explicit: true, .. }
        ));
    }

    #[cfg(feature = "test_equality")]
    #[test]
    fn equality_retains_core_payload_and_notation_distinctions() {
        let scalar = Event::new_scalar("~".into(), ScalarStyle::plain(), 0, None);
        for different in [
            Event::new_scalar("null".into(), ScalarStyle::plain(), 0, None),
            Event::new_scalar("~".into(), ScalarStyle::single_quoted(), 0, None),
            Event::new_scalar("~".into(), ScalarStyle::plain(), 1, None),
            Event::new_scalar(
                "~".into(),
                ScalarStyle::plain(),
                0,
                Some(Cow::Owned(Tag::new("!", "tag"))),
            ),
            Event::alias(0),
        ] {
            assert_ne!(scalar, different);
            assert_ne!(different, scalar);
        }
        assert_ne!(ScalarStyle::literal(), ScalarStyle::folded());
        assert_ne!(
            Event::document_start(false, None),
            Event::document_start(true, None)
        );
        let token_type = TokenType::scalar(ScalarStyle::plain(), "~".into());
        assert_ne!(
            token_type,
            TokenType::scalar(ScalarStyle::plain(), "null".into())
        );
        assert_ne!(
            token_type,
            TokenType::scalar(ScalarStyle::double_quoted(), "~".into())
        );
        assert_ne!(
            Token::new(Span::empty(Marker::new(0, 1, 0)), token_type.clone()),
            Token::new(Span::empty(Marker::new(1, 1, 1)), token_type)
        );
    }

    #[cfg(all(feature = "test_equality", feature = "extended_details"))]
    #[test]
    fn optional_details_do_not_turn_default_values_into_equality_wildcards() {
        let clipped = BlockScalarSyntax::new(Chomping::Clip, None, 2).unwrap();
        let kept = BlockScalarSyntax::new(Chomping::Keep, Some(2), 4).unwrap();
        assert_ne!(clipped, kept);
        let span = Span::empty(Marker::new(0, 1, 0));
        for style in [ScalarStyle::literal(), ScalarStyle::folded()] {
            let styles = [
                style,
                style.with_block_syntax(clipped),
                style.with_block_syntax(kept),
            ];
            // All pairs compare equal, including two different non-default metadata values.
            // This checks symmetry and transitivity through the metadata-free constructor.
            for left in styles {
                for right in styles {
                    assert_eq!(left, right);
                    assert_eq!(
                        Event::new_scalar("value".into(), left, 0, None),
                        Event::new_scalar("value".into(), right, 0, None)
                    );
                    let left = TokenType::scalar(left, "value".into());
                    let right = TokenType::scalar(right, "value".into());
                    assert_eq!(left, right);
                    assert_eq!(Token::new(span, left), Token::new(span, right));
                }
            }
        }
    }

    #[cfg(all(feature = "test_equality", feature = "extended_details"))]
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
    #[test]
    fn basic_constructor_signatures_are_feature_independent() {
        let plain: fn() -> ScalarStyle = ScalarStyle::plain;
        let literal: fn() -> ScalarStyle = ScalarStyle::literal;
        let folded: fn() -> ScalarStyle = ScalarStyle::folded;
        let end: fn() -> Event<'static> = Event::document_end;
        assert!(matches!(plain(), ScalarStyle::Plain { .. }));
        assert!(matches!(literal(), ScalarStyle::Literal { .. }));
        assert!(matches!(folded(), ScalarStyle::Folded { .. }));
        assert!(matches!(end(), Event::DocumentEnd { .. }));
    }

    #[cfg(feature = "extended_details")]
    #[test]
    fn synthetic_metadata_defaults_are_explicit_and_builders_supply_details() {
        assert!(matches!(
            ScalarStyle::plain(),
            ScalarStyle::Plain {
                implicit: false,
                ..
            }
        ));
        assert!(matches!(
            Event::document_end(),
            Event::DocumentEnd {
                explicit: false,
                ..
            }
        ));
        for style in [ScalarStyle::literal(), ScalarStyle::folded()] {
            assert!(style.block_syntax().is_none());
            let syntax = BlockScalarSyntax::new(Chomping::Strip, Some(2), 4).unwrap();
            let detailed = style.with_block_syntax(syntax);
            let actual = detailed.block_syntax().unwrap();
            assert_eq!(actual.chomping, Chomping::Strip);
            assert_eq!(actual.indentation_indicator, Some(2));
            assert_eq!(actual.content_indent, 4);
        }
        assert!(matches!(
            ScalarStyle::plain().with_implicit(true),
            ScalarStyle::Plain { implicit: true, .. }
        ));
        assert!(matches!(
            Event::document_end().with_explicit_end(true),
            Event::DocumentEnd { explicit: true, .. }
        ));
    }

    #[cfg(feature = "extended_details")]
    #[test]
    fn compact_metadata_keeps_the_existing_constructor_signature() {
        const ZERO: Option<BlockScalarSyntax> = BlockScalarSyntax::new(Chomping::Clip, None, 0);
        let construct: fn(Chomping, Option<u8>, usize) -> Option<BlockScalarSyntax> =
            BlockScalarSyntax::new;
        assert_eq!(ZERO.unwrap().content_indent, 0);
        let mut syntax = construct(Chomping::Clip, None, 2).unwrap();
        // These assignments and reads verify the public field types, not only their values.
        let distance: u32 = 5;
        syntax.header_chars_before_span = distance;
        syntax.header_bytes_before_span = distance;
        let indentation: u32 = syntax.content_indent;
        assert_eq!(indentation, 2);
        let span = Span::empty(Marker::new(10, 2, 2).with_byte_offset(Some(10)));
        assert_eq!(syntax.header_char_range(&span), Some(5..6));
        assert_eq!(syntax.header_byte_range(&span), Some(5..6));
    }

    #[cfg(feature = "extended_details")]
    #[test]
    fn parsed_events_retain_metadata_with_extended_details() {
        let events = granit_parser::Parser::new_from_str("literal: |-\n  value\nomitted:\n...\n")
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(events.iter().any(|(event, _)| matches!(event,
            Event::Scalar { style: ScalarStyle::Literal { syntax: Some(syntax), .. }, .. }
                if matches!(syntax.chomping, Chomping::Strip)
        )));
        assert!(events.iter().any(|(event, _)| matches!(
            event,
            Event::Scalar {
                style: ScalarStyle::Plain { implicit: true, .. },
                ..
            }
        )));
        assert!(events
            .iter()
            .any(|(event, _)| matches!(event, Event::DocumentEnd { explicit: true, .. })));
    }
}
