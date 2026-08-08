/// Options controlling parser and scanner behavior and resource usage.
///
/// Construct this type with [`crate::options!`] so that code remains compatible when new options
/// are added in future releases.
///
/// # Examples
///
/// ```rust
/// let options = granit_parser::options! {
///     max_buffered_comment_events: 64,
///     emit_comments: false,
///     flow_nesting_limit: 512,
/// };
///
/// assert_eq!(options.max_buffered_comment_events, 64);
/// assert!(!options.emit_comments);
/// assert_eq!(options.flow_nesting_limit, 512);
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// Whether scanners emit comment tokens and parsers emit comment events.
    ///
    /// The default is `true`. When this is `false`, comments are still recognized and validated
    /// as YAML syntax, but their text is not captured and no comment tokens or events are emitted.
    /// Comment bytes are still consumed, so this is not an input-size or processing-time limit.
    /// [`Self::max_buffered_comment_events`] has no effect while comment emission is disabled.
    pub emit_comments: bool,
    /// Maximum number of consecutive comment events buffered while resolving an ambiguous
    /// collection entry.
    ///
    /// The default is 96. A value of zero rejects the first comment that would need buffering.
    pub max_buffered_comment_events: usize,
    /// Maximum number of characters inspected while resolving a simple key.
    ///
    /// The default is 1024, matching YAML's simple-key length restriction. A key at exactly this
    /// limit is accepted. Lower values impose a stricter resource limit; higher values relax that
    /// YAML restriction.
    pub simple_key_max_lookahead: usize,
    /// Maximum number of simultaneously nested flow collections.
    ///
    /// The default is 255. A value of zero rejects the first flow collection opener.
    pub flow_nesting_limit: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            emit_comments: true,
            max_buffered_comment_events: 96,
            simple_key_max_lookahead: 1024,
            flow_nesting_limit: 255,
        }
    }
}
