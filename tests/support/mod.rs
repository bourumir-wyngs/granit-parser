#![allow(dead_code)]

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
