/// Limits controlling parser and scanner resource usage.
///
/// Construct this type with [`crate::options!`] so that code remains compatible when new options
/// are added in future releases.
///
/// # Examples
///
/// ```rust
/// let options = granit_parser::options! {
///     max_buffered_comment_events: 64,
///     flow_nesting_limit: 512,
/// };
///
/// assert_eq!(options.max_buffered_comment_events, 64);
/// assert_eq!(options.flow_nesting_limit, 512);
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// Maximum number of consecutive comment events buffered while resolving an ambiguous
    /// collection entry.
    ///
    /// The default is 32. A value of zero rejects the first comment that would need buffering.
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
            max_buffered_comment_events: 32,
            simple_key_max_lookahead: 1024,
            flow_nesting_limit: 255,
        }
    }
}
