use std::borrow::Cow;

use granit_parser::{Event, Parser, ScalarStyle, Tag};

#[test]
fn ownership_conversions_preserve_parsed_fields_after_the_source_is_dropped() {
    let retained = {
        let source = String::from(concat!(
            "%YAML 1.2\n%TAG !e! tag:example.com,2026:\n---\n",
            "mapping: &mapping !e!mapping\n",
            "  scalar: &scalar !e!scalar \"escaped\\nvalue\"\n",
            "  sequence: &sequence !e!sequence [plain, *scalar, *missing]\n",
            "  omitted:\n",
            "  literal: |2-\n",
            "    contents\n",
            "# final comment\n...\n",
        ));
        let options = granit_parser::options! {
            emit_anchor_events: true,
            emit_unresolved_aliases: true,
        };
        Parser::new_from_str_with_options(&source, options)
            .map(|result| {
                let (event, _) = result.unwrap();
                // Whole-event equality deliberately ignores source metadata. Debug includes
                // those fields, allowing a lossless-conversion check in every feature set.
                (format!("{event:?}"), event.into_owned())
            })
            .collect::<Vec<_>>()
    };

    for (expected, owned) in retained {
        assert_eq!(format!("{owned:?}"), expected);
        let borrowed = owned.as_borrowed();
        assert_eq!(format!("{borrowed:?}"), expected);
        match &borrowed {
            Event::Scalar { value, tag, .. } => {
                assert!(matches!(value, Cow::Borrowed(_)));
                assert!(tag
                    .as_ref()
                    .map_or(true, |tag| matches!(tag, Cow::Borrowed(_))));
            }
            Event::SequenceStart { tag, .. } | Event::MappingStart { tag, .. } => {
                assert!(tag
                    .as_ref()
                    .map_or(true, |tag| matches!(tag, Cow::Borrowed(_))));
            }
            Event::Anchor { name, .. } | Event::UnresolvedAlias { name, .. } => {
                assert!(matches!(name, Cow::Borrowed(_)));
            }
            #[cfg(feature = "comments")]
            Event::Comment { value, .. } => assert!(matches!(value, Cow::Borrowed(_))),
            _ => {}
        }
        assert_eq!(format!("{:?}", borrowed.into_owned()), expected);
    }
}

#[test]
fn into_owned_moves_existing_string_and_tag_allocations() {
    let value = String::from("owned scalar");
    let value_ptr = value.as_ptr();
    let tag = Tag::with_original_handle("tag:example.com,2026:", "value", "!e!");
    let tag_ptr = tag.handle().as_ptr();
    let event = Event::new_scalar(
        Cow::Owned(value),
        ScalarStyle::implicit_plain(),
        7,
        Some(Cow::Owned(tag)),
    );

    let owned = event.into_owned();
    assert!(matches!(
        &owned,
        Event::Scalar {
            value: Cow::Owned(value),
            tag: Some(Cow::Owned(tag)),
            style,
            anchor_id: 7,
            ..
        } if value.as_ptr() == value_ptr && tag.handle().as_ptr() == tag_ptr && style.is_omitted()
    ));
    assert_eq!(owned.tag().unwrap().original_handle(), "!e!");
}

#[test]
fn borrowed_view_points_to_owned_payloads_and_can_be_retained_independently() {
    let retained = {
        let original = Event::new_scalar(
            Cow::Owned(String::from("owned scalar")),
            ScalarStyle::plain(),
            5,
            Some(Cow::Owned(Tag::new("!", "custom"))),
        );
        let borrowed = original.as_borrowed();
        assert!(std::ptr::eq(
            borrowed.scalar().unwrap().0.as_ptr(),
            original.scalar().unwrap().0.as_ptr(),
        ));
        assert!(std::ptr::eq(
            borrowed.tag().unwrap(),
            original.tag().unwrap()
        ));
        borrowed.into_owned()
    };
    assert_eq!(retained.scalar().unwrap().0, "owned scalar");
    assert_eq!(retained.anchor_id(), Some(5));
    assert_eq!(retained.tag().unwrap().suffix(), "custom");
}

#[cfg(feature = "extended_details")]
#[test]
fn ownership_conversions_keep_explicit_document_ends() {
    let end = Event::document_end().with_explicit_end(true);
    assert!(matches!(
        end.as_borrowed().into_owned(),
        Event::DocumentEnd { explicit: true, .. }
    ));
    assert!(matches!(
        end.into_owned(),
        Event::DocumentEnd { explicit: true, .. }
    ));
}
