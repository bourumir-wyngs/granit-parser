//! Public macro for constructing [`Options`](crate::Options) without struct literal syntax.
//!
//! This macro keeps call sites ergonomic while allowing the option struct to gain fields without a
//! breaking change.

/// Construct [`Options`](crate::Options) from its defaults and a list of field assignments.
///
/// # Examples
///
/// ```rust
/// let options = granit_parser::options! {
///     max_buffered_comment_events: 64,
///     emit_comments: false,
///     simple_key_max_lookahead: 2048,
/// };
///
/// assert_eq!(options.max_buffered_comment_events, 64);
/// assert!(!options.emit_comments);
/// assert_eq!(options.simple_key_max_lookahead, 2048);
/// assert_eq!(options.flow_nesting_limit, 255);
/// ```
#[macro_export]
macro_rules! options {
    ($($field:ident : $value:expr),* $(,)?) => {{
        let mut options = $crate::Options::default();
        $(options.$field = $value;)*
        options
    }};
}
