#![allow(dead_code)]

pub mod metadata;

// Keep the borrowing and optional failure context of assert_eq! while checking metadata too.
#[allow(unused_macros)]
macro_rules! assert_events_eq_macro {
    ($left:expr, $right:expr $(,)?) => {
        $crate::support::assert_events_eq(&$left, &$right)
    };
    ($left:expr, $right:expr, $($message:tt)+) => {
        $crate::support::assert_events_eq_with_context(
            &$left, &$right, format_args!($($message)+)
        )
    };
}

#[allow(unused_imports)]
pub(crate) use assert_events_eq_macro as assert_events_eq;

#[allow(unused_macros)]
macro_rules! assert_spanned_events_eq {
    ($left:expr, $right:expr $(,)?) => {
        $crate::support::assert_spanned_events_eq_with_context(&$left, &$right, format_args!(""))
    };
    ($left:expr, $right:expr, $($message:tt)+) => {
        $crate::support::assert_spanned_events_eq_with_context(
            &$left, &$right, format_args!($($message)+)
        )
    };
}

#[allow(unused_imports)]
pub(crate) use assert_spanned_events_eq;

/// Assert both the stable event payload and the optional source details.
#[track_caller]
pub fn assert_events_eq(left: &[granit_parser::Event<'_>], right: &[granit_parser::Event<'_>]) {
    assert_events_eq_with_context(left, right, format_args!(""));
}

/// Assert event payloads and optional details, retaining the caller's failure context.
///
/// * `left`: Actual events.
/// * `right`: Expected events, including any enabled metadata.
/// * `context`: Additional diagnostic text, such as the YAML input under test.
///
/// Returns `()` on success; panics with the context and event index on a mismatch.
#[track_caller]
pub fn assert_events_eq_with_context(
    left: &[granit_parser::Event<'_>],
    right: &[granit_parser::Event<'_>],
    context: core::fmt::Arguments<'_>,
) {
    assert_eq!(left, right, "{context}");
    for (index, (left, right)) in left.iter().zip(right).enumerate() {
        metadata::assert_event_details_eq_with_context(
            left,
            right,
            format_args!("{context}; event {index}"),
        );
    }
}

/// Assert event payloads, spans, and optional details with failure context.
///
/// * `left`: Actual events and spans.
/// * `right`: Expected events and spans, including any enabled metadata.
/// * `context`: Additional diagnostic text, such as the YAML input under test.
///
/// Returns `()` on success; panics with the context and event index on a mismatch.
#[track_caller]
pub fn assert_spanned_events_eq_with_context(
    left: &[(granit_parser::Event<'_>, granit_parser::Span)],
    right: &[(granit_parser::Event<'_>, granit_parser::Span)],
    context: core::fmt::Arguments<'_>,
) {
    assert_eq!(left, right, "{context}");
    for (index, ((left, _), (right, _))) in left.iter().zip(right).enumerate() {
        metadata::assert_event_details_eq_with_context(
            left,
            right,
            format_args!("{context}; event {index}"),
        );
    }
}

pub fn is_comment_event(event: &granit_parser::Event<'_>) -> bool {
    #[cfg(feature = "comments")]
    {
        matches!(event, granit_parser::Event::Comment { .. })
    }
    #[cfg(not(feature = "comments"))]
    {
        let _ = event;
        false
    }
}

pub fn is_comment_token(token: &granit_parser::TokenType<'_>) -> bool {
    #[cfg(feature = "comments")]
    {
        matches!(token, granit_parser::TokenType::Comment { .. })
    }
    #[cfg(not(feature = "comments"))]
    {
        let _ = token;
        false
    }
}

pub fn comment_modes() -> &'static [bool] {
    #[cfg(feature = "comments")]
    {
        &[true, false]
    }
    #[cfg(not(feature = "comments"))]
    {
        &[false]
    }
}

/// Construct the omitted plain style, retaining its metadata when enabled.
pub fn implicit_plain() -> granit_parser::ScalarStyle {
    #[cfg(feature = "extended_details")]
    {
        granit_parser::ScalarStyle::plain().with_implicit(true)
    }
    #[cfg(not(feature = "extended_details"))]
    {
        granit_parser::ScalarStyle::plain()
    }
}

/// Construct a document end and retain its explicit-marker flag when enabled.
pub fn document_end(explicit: bool) -> granit_parser::Event<'static> {
    #[cfg(feature = "extended_details")]
    {
        granit_parser::Event::document_end().with_explicit_end(explicit)
    }
    #[cfg(not(feature = "extended_details"))]
    {
        let _ = explicit;
        granit_parser::Event::document_end()
    }
}
