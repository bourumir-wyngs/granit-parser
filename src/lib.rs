// Copyright 2015, Yuheng Chen.
// Copyright 2023, Ethiraric.
// See the LICENSE file at the top-level directory of this distribution.

//! YAML 1.2 parser implementation in pure Rust.
//!
//! `granit-parser` is a low-level event parser. It reads YAML input and yields a stream of
//! [`Event`] values paired with their source [`Span`].
//! Comments are emitted as [`Event::Comment`]. They are presentation metadata, not YAML data
//! nodes, so consumers building YAML value trees should ignore them.
//!
//! Add it to your project:
//!
//! ```sh
//! cargo add granit-parser
//! ```
//!
//! # Usage
//!
//! ```rust
//! use granit_parser::{Event, Parser, Placement};
//!
//! # fn main() -> Result<(), granit_parser::ScanError> {
//! let yaml = r#"# header
//! items: # inline
//!   - milk
//!   - bread
//! "#;
//! let mut comments = Vec::new();
//!
//! for next in Parser::new_from_str(yaml) {
//!     let (event, span) = next?;
//!     if let Event::Comment(text, placement) = event {
//!         comments.push((
//!             text.into_owned(),
//!             placement,
//!             span.slice(yaml).unwrap().to_owned(),
//!         ));
//!     }
//! }
//!
//! assert_eq!(
//!     comments,
//!     [
//!         (" header".to_owned(), Placement::Above, "# header".to_owned()),
//!         (" inline".to_owned(), Placement::Right, "# inline".to_owned()),
//!     ]
//! );
//! # Ok(())
//! # }
//! ```
//!
//! For comment events, the companion [`Span`] covers the whole source comment, including `#` and
//! excluding the line break. With [`Parser::new_from_str`], [`Span::slice`] returns that source
//! comment text.
//!
//! # Limits
//!
//! To keep streaming parsing memory bounded, syntactically ambiguous collection-entry positions
//! that require comment lookahead accept at most 32 consecutive comments before the following node
//! is resolved. Longer runs return a [`ScanError`] instead of being buffered without bound.
//!
//! # Features
//! **Note:** This crate's MSRV is `1.81.0`.
//!
//! #### `debug_prints`
//! Enables the `debug` module and usage of debug prints in the scanner and the parser. Do not
//! enable if you are consuming the crate rather than working on it as this can significantly
//! decrease performance. Output remains opt-in behind a local compile-time toggle in
//! `src/debug.rs`.
//!
//! This feature does not raise the MSRV further.
//!
//! This feature is _not_ `no_std` compatible.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![no_std]

#[macro_use]
extern crate alloc;

#[cfg(feature = "debug_prints")]
extern crate std;

mod char_traits;
#[macro_use]
mod debug;
pub mod input;
mod parser;
/// A stack-based parser implementation.
pub mod parser_stack;
mod scanner;

pub use crate::input::{str::StrInput, BorrowedInput, BufferedInput, Input};
pub use crate::parser::{
    Event, EventReceiver, Parser, ParserTrait, SpannedEventReceiver, StructureStyle, Tag,
    TryEventReceiver, TryLoadError, TrySpannedEventReceiver,
};
pub use crate::scanner::{
    Comment, Marker, Placement, ScalarStyle, ScanError, Scanner, Span, Token, TokenType,
};
