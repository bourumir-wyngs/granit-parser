//! Parser and scanner error types.

use alloc::{borrow::ToOwned, string::String};
use core::fmt;

use crate::scanner::Marker;

/// An error that occurred while scanning or parsing YAML.
#[derive(Clone, PartialEq, Debug, Eq)]
pub struct ScanError {
    /// The position at which the error happened in the source.
    mark: Marker,
    /// Human-readable details about the error.
    info: String,
}

impl ScanError {
    /// Create a new error from a location and an error string.
    #[must_use]
    #[cold]
    pub fn new(loc: Marker, info: String) -> ScanError {
        ScanError { mark: loc, info }
    }

    /// Convenience alias for string slices.
    #[must_use]
    #[cold]
    pub fn new_str(loc: Marker, info: &str) -> ScanError {
        ScanError {
            mark: loc,
            info: info.to_owned(),
        }
    }

    #[cold]
    pub(crate) fn into_result<T>(self) -> Result<T, ScanError> {
        Err(self)
    }

    /// Return the marker pointing to the error in the source.
    #[must_use]
    pub fn marker(&self) -> &Marker {
        &self.mark
    }

    /// Return the information string describing the error that happened.
    #[must_use]
    pub fn info(&self) -> &str {
        self.info.as_ref()
    }
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at char {} line {} column {}",
            self.info,
            self.mark.index(),
            self.mark.line(),
            self.mark.col() + 1
        )
    }
}

impl core::error::Error for ScanError {}
