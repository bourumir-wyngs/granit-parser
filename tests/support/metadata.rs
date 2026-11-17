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
pub fn assert_event_details_eq(left: &Event<'_>, right: &Event<'_>) {
    #[cfg(feature = "extended_details")]
    match (left, right) {
        (
            Event::DocumentEnd { explicit: left, .. },
            Event::DocumentEnd {
                explicit: right, ..
            },
        ) => assert_eq!(left, right, "document-end markers differ"),
        (Event::Scalar { style: left, .. }, Event::Scalar { style: right, .. }) => {
            assert_scalar_details_eq(*left, *right);
        }
        _ => {}
    }
    #[cfg(not(feature = "extended_details"))]
    let _ = (left, right);
}

/// Compare optional details separately from the feature-independent output equality.
pub fn assert_scanner_details_eq(left: &[Token<'_>], right: &[Token<'_>]) {
    assert_eq!(left.len(), right.len());
    #[cfg(feature = "extended_details")]
    for (left, right) in left.iter().zip(right) {
        if let (TokenType::Scalar { style: left, .. }, TokenType::Scalar { style: right, .. }) =
            (left.token_type(), right.token_type())
        {
            assert_scalar_details_eq(*left, *right);
        }
    }
}

#[cfg(feature = "extended_details")]
fn assert_scalar_details_eq(left: ScalarStyle, right: ScalarStyle) {
    match (left, right) {
        (
            ScalarStyle::Plain { implicit: left, .. },
            ScalarStyle::Plain {
                implicit: right, ..
            },
        ) => assert_eq!(left, right, "omitted-node flags differ"),
        (
            ScalarStyle::Literal { syntax: left, .. } | ScalarStyle::Folded { syntax: left, .. },
            ScalarStyle::Literal { syntax: right, .. } | ScalarStyle::Folded { syntax: right, .. },
        ) => assert_eq!(left, right, "block-scalar syntax differs"),
        _ => {}
    }
}
