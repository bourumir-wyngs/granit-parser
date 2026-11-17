#![allow(dead_code)]

use granit_parser::{Event, Span, Token};
#[cfg(feature = "extended_details")]
use granit_parser::{ScalarStyle, TokenType};

/// Compare optional details separately from the feature-independent output equality.
pub fn assert_parser_details_eq(left: &[(Event<'_>, Span)], right: &[(Event<'_>, Span)]) {
    assert_eq!(left.len(), right.len());
    for ((left, _), (right, _)) in left.iter().zip(right) {
        assert_event_details_eq(left, right);
    }
}

/// Compare optional details separately from the feature-independent output equality.
#[track_caller]
pub fn assert_event_details_eq(left: &Event<'_>, right: &Event<'_>) {
    assert_event_details_eq_with_context(left, right, format_args!(""));
}

/// Compare optional event details while retaining the caller's failure context.
///
/// * `left`: Actual event.
/// * `right`: Expected event, including any enabled metadata.
/// * `context`: Additional diagnostic text, such as the input and event index.
///
/// Returns `()` on success; panics with the context on a metadata mismatch.
#[track_caller]
pub fn assert_event_details_eq_with_context(
    left: &Event<'_>,
    right: &Event<'_>,
    context: core::fmt::Arguments<'_>,
) {
    #[cfg(feature = "extended_details")]
    match (left, right) {
        (
            Event::DocumentEnd { explicit: left, .. },
            Event::DocumentEnd {
                explicit: right, ..
            },
        ) => assert_eq!(left, right, "document-end markers differ: {context}"),
        (Event::Scalar { style: left, .. }, Event::Scalar { style: right, .. }) => {
            assert_scalar_details_eq(*left, *right, context);
        }
        _ => {}
    }
    #[cfg(not(feature = "extended_details"))]
    let _ = (left, right, context);
}

/// Compare optional details separately from the feature-independent output equality.
pub fn assert_scanner_details_eq(left: &[Token<'_>], right: &[Token<'_>]) {
    assert_eq!(left.len(), right.len());
    #[cfg(feature = "extended_details")]
    for (left, right) in left.iter().zip(right) {
        if let (TokenType::Scalar { style: left, .. }, TokenType::Scalar { style: right, .. }) =
            (left.token_type(), right.token_type())
        {
            assert_scalar_details_eq(*left, *right, format_args!(""));
        }
    }
}

#[cfg(feature = "extended_details")]
#[track_caller]
/// Check scalar metadata; `left` is actual, `right` is expected, and `context` describes
/// the input. Returns `()` on success or panics with the context on a mismatch.
fn assert_scalar_details_eq(
    left: ScalarStyle,
    right: ScalarStyle,
    context: core::fmt::Arguments<'_>,
) {
    match (left, right) {
        (
            ScalarStyle::Plain { implicit: left, .. },
            ScalarStyle::Plain {
                implicit: right, ..
            },
        ) => assert_eq!(left, right, "omitted-node flags differ: {context}"),
        (
            ScalarStyle::Literal { syntax: left, .. } | ScalarStyle::Folded { syntax: left, .. },
            ScalarStyle::Literal { syntax: right, .. } | ScalarStyle::Folded { syntax: right, .. },
        ) => assert_eq!(left, right, "block-scalar syntax differs: {context}"),
        _ => {}
    }
}
