// Copyright 2015, Yuheng Chen.
// Copyright 2023, Ethiraric.
// See the LICENSE file at the top-level directory of this distribution.

//! YAML 1.2 parser implementation in pure Rust.
//!
//! `granit-parser` is a low-level event parser. It reads YAML input and yields a stream of
//! [`Event`] values paired with their source [`Span`].
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
//! use granit_parser::{Event, Parser};
//!
//! # fn main() -> Result<(), granit_parser::ScanError> {
//! let yaml = "items:\n  - milk\n  - bread\n";
//!
//! for next in Parser::new_from_str(yaml) {
//!     let (event, _span) = next?;
//!     if let Event::Scalar(value, ..) = event {
//!         println!("{value}");
//!     }
//! }
//! # Ok(())
//! # }
//! ```
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
    Event, EventReceiver, Parser, ParserTrait, SpannedEventReceiver, Tag, TryEventReceiver,
    TryLoadError, TrySpannedEventReceiver,
};
pub use crate::scanner::{Marker, ScalarStyle, ScanError, Scanner, Span, Token, TokenType};
