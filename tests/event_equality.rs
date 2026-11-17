#![cfg(feature = "test_equality")]

use std::{borrow::Cow, fmt::Debug};

#[cfg(feature = "comments")]
use granit_parser::Placement;
use granit_parser::{Event, Parser, ScalarStyle, StructureStyle, Tag, YamlVersion};

fn assert_pairwise_distinct<T: Debug + Eq>(values: &[T]) {
    for (left_index, left) in values.iter().enumerate() {
        for (right_index, right) in values.iter().enumerate() {
            assert_eq!(
                left == right,
                left_index == right_index,
                "unexpected equality between {left:?} and {right:?}",
            );
        }
    }
}

#[test]
fn event_variants_remain_distinct_even_with_similar_payloads() {
    assert_pairwise_distinct(&[
        Event::stream_start(),
        Event::stream_end(),
        Event::document_start(false, None),
        Event::document_end(),
        Event::alias(1),
        Event::anchor(1, "value".into()),
        Event::unresolved_alias("value".into()),
        #[cfg(feature = "comments")]
        Event::comment("value".into(), Placement::Free),
        Event::new_scalar("value".into(), ScalarStyle::plain(), 1, None),
        Event::sequence_start(StructureStyle::Block, 1, None),
        Event::sequence_end(),
        Event::mapping_start(StructureStyle::Block, 1, None),
        Event::mapping_end(),
    ]);
}

#[test]
fn document_versions_and_anchor_identity_are_compared() {
    assert_pairwise_distinct(&[
        Event::document_start(false, None),
        Event::document_start(true, None),
        Event::document_start(true, Some(YamlVersion::new(1, 1))),
        Event::document_start(true, Some(YamlVersion::new(1, 2))),
        Event::document_start(true, Some(YamlVersion::new(2, 2))),
    ]);
    assert_pairwise_distinct(&[Event::alias(1), Event::alias(2)]);
    assert_pairwise_distinct(&[
        Event::anchor(1, "name".into()),
        Event::anchor(2, "name".into()),
        Event::anchor(1, "renamed".into()),
    ]);
    assert_pairwise_distinct(&[
        Event::unresolved_alias("name".into()),
        Event::unresolved_alias("renamed".into()),
    ]);
}

#[test]
fn scalar_values_notation_and_properties_are_compared() {
    let styles = [
        ScalarStyle::plain(),
        ScalarStyle::single_quoted(),
        ScalarStyle::double_quoted(),
        ScalarStyle::literal(),
        ScalarStyle::folded(),
    ];
    assert_pairwise_distinct(&styles);
    let styled_events: Vec<_> = styles
        .into_iter()
        .map(|style| Event::new_scalar("value".into(), style, 0, None))
        .collect();
    assert_pairwise_distinct(&styled_events);

    let plain = ScalarStyle::plain();
    assert_pairwise_distinct(&[
        Event::new_scalar("value".into(), plain, 0, None),
        Event::new_scalar("other".into(), plain, 0, None),
        Event::new_scalar("value".into(), plain, 1, None),
        Event::new_scalar(
            "value".into(),
            plain,
            0,
            Some(Cow::Owned(Tag::new("!", "first"))),
        ),
        Event::new_scalar(
            "value".into(),
            plain,
            0,
            Some(Cow::Owned(Tag::new("!", "second"))),
        ),
    ]);
}

#[test]
fn collection_notation_anchor_and_tag_properties_are_compared() {
    for construct in [Event::sequence_start, Event::mapping_start] {
        assert_pairwise_distinct(&[
            construct(StructureStyle::Block, 0, None),
            construct(StructureStyle::Flow, 0, None),
            construct(StructureStyle::Block, 1, None),
            construct(
                StructureStyle::Block,
                0,
                Some(Cow::Owned(Tag::new("!", "first"))),
            ),
            construct(
                StructureStyle::Block,
                0,
                Some(Cow::Owned(Tag::new("!", "second"))),
            ),
        ]);
    }
}

#[cfg(feature = "comments")]
#[test]
fn comment_text_and_placement_are_compared() {
    assert_pairwise_distinct(&[
        Event::comment("same".into(), Placement::Above),
        Event::comment("other".into(), Placement::Above),
        Event::comment("same".into(), Placement::Right),
        Event::comment("same".into(), Placement::Free),
        Event::comment("same".into(), Placement::Last),
    ]);
}

#[test]
fn owned_and_borrowed_payloads_compare_by_content() {
    let value = "value".to_owned();
    let tag = Tag::new("!", "tag");
    let pairs = [
        (
            Event::anchor(1, Cow::Borrowed(&value)),
            Event::anchor(1, Cow::Owned(value.clone())),
        ),
        (
            Event::unresolved_alias(Cow::Borrowed(&value)),
            Event::unresolved_alias(Cow::Owned(value.clone())),
        ),
        (
            Event::new_scalar(
                Cow::Borrowed(&value),
                ScalarStyle::plain(),
                1,
                Some(Cow::Borrowed(&tag)),
            ),
            Event::new_scalar(
                Cow::Owned(value.clone()),
                ScalarStyle::plain(),
                1,
                Some(Cow::Owned(tag.clone())),
            ),
        ),
        (
            Event::sequence_start(StructureStyle::Flow, 1, Some(Cow::Borrowed(&tag))),
            Event::sequence_start(StructureStyle::Flow, 1, Some(Cow::Owned(tag.clone()))),
        ),
        (
            Event::mapping_start(StructureStyle::Flow, 1, Some(Cow::Borrowed(&tag))),
            Event::mapping_start(StructureStyle::Flow, 1, Some(Cow::Owned(tag.clone()))),
        ),
        #[cfg(feature = "comments")]
        (
            Event::comment(Cow::Borrowed(&value), Placement::Right),
            Event::comment(Cow::Owned(value.clone()), Placement::Right),
        ),
    ];
    for (borrowed, owned) in pairs {
        assert_eq!(borrowed, owned);
        assert_eq!(owned, borrowed);
        assert_eq!(borrowed.clone(), owned.clone());
    }
}

#[test]
fn parsed_omissions_and_document_ends_equal_synthetic_defaults_in_every_build() {
    let parsed: Vec<_> = Parser::new_from_str("key:\n...\n")
        .map(|result| result.unwrap().0)
        .collect();
    let omitted = parsed
        .iter()
        .find(|event| matches!(event, Event::Scalar { value, .. } if value == "~"))
        .unwrap();
    let synthetic = Event::new_scalar("~".into(), ScalarStyle::plain(), 0, None);
    assert_eq!(omitted, &synthetic);
    assert_eq!(&synthetic, omitted);

    let document_end = parsed
        .iter()
        .find(|event| matches!(event, Event::DocumentEnd { .. }))
        .unwrap();
    assert_eq!(document_end, &Event::document_end());
    assert_eq!(&Event::document_end(), document_end);
    #[cfg(feature = "extended_details")]
    {
        assert!(matches!(
            omitted,
            Event::Scalar {
                style: ScalarStyle::Plain { implicit: true, .. },
                ..
            }
        ));
        assert!(matches!(
            synthetic,
            Event::Scalar {
                style: ScalarStyle::Plain {
                    implicit: false,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            document_end,
            Event::DocumentEnd { explicit: true, .. }
        ));
    }
}

#[cfg(feature = "extended_details")]
#[test]
fn differing_block_headers_do_not_break_equality_transitivity() {
    use granit_parser::{BlockScalarSyntax, Chomping};

    let clipped = BlockScalarSyntax::new(Chomping::Clip, None, 2).unwrap();
    let kept = BlockScalarSyntax::new(Chomping::Keep, Some(2), 4).unwrap();
    assert_ne!(clipped, kept);
    for construct in [ScalarStyle::literal, ScalarStyle::folded] {
        let styles = [
            construct(),
            construct().with_block_syntax(clipped),
            construct().with_block_syntax(kept),
        ];
        assert!(styles[0].block_syntax().is_none());
        assert_eq!(styles[1].block_syntax(), Some(&clipped));
        assert_eq!(styles[2].block_syntax(), Some(&kept));
        for left in styles {
            for right in styles {
                assert_eq!(left, right);
                assert_eq!(
                    Event::new_scalar("value".into(), left, 0, None),
                    Event::new_scalar("value".into(), right, 0, None),
                );
            }
        }
    }
}
