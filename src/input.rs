//! Utilities to create a source of input to the parser.
//!
//! [`Input`] must be implemented for the parser to fetch input. Make sure your needs aren't
//! covered by the [`BufferedInput`].

use alloc::string::String;

pub(crate) mod buffered;
pub(crate) mod str;

#[allow(clippy::module_name_repetitions)]
pub use buffered::BufferedInput;

/// A trait for inputs that can provide borrowed slices with a specific lifetime.
///
/// This trait enables zero-copy (`Cow::Borrowed`) token values for inputs that keep a stable
/// backing string. The key difference from [`Input::slice_bytes`] is that this method returns
/// a slice with the input's original lifetime `'a`, not tied to `&self`.
///
/// For inputs that support zero-copy (like [`str::StrInput`]), this returns `Some(&'a str)`.
/// For streaming inputs that don't have stable backing storage, this returns `None`.
pub trait BorrowedInput<'a>: Input {
    /// Return a borrowed slice of the underlying source between two byte offsets.
    ///
    /// Unlike [`Input::slice_bytes`], this returns a slice with the input's lifetime `'a`,
    /// allowing the slice to outlive the borrow of `&self`.
    ///
    /// `start` and `end` are byte offsets as returned by [`Input::byte_offset`]. The interval is
    /// half-open: `[start, end)`.
    ///
    /// Returns `None` if the input does not support zero-copy slicing.
    #[must_use]
    fn slice_borrowed(&self, start: usize, end: usize) -> Option<&'a str>;
}

pub use crate::char_traits::{
    is_alpha, is_blank, is_blank_or_breakz, is_break, is_breakz, is_digit, is_flow, is_z,
};

/// Interface for a source of characters.
///
/// Hiding the input's implementation behind this trait allows mostly:
///  * For input-specific optimizations (for instance, using `str` methods instead of manually
///    transferring one `char` at a time to a buffer).
///  * To return `&str`s referencing the input string, thus avoiding potentially costly
///    allocations. Should users need an owned version of the data, they can always `.to_owned()`
///    their YAML object.
pub trait Input {
    /// A hint to the input source that we will need to read `count` characters.
    ///
    /// If the input is exhausted, `\0` can be used to pad the last characters and later returned.
    /// The characters must not be consumed, but may be placed in an internal buffer.
    ///
    /// This method may be a no-op if buffering yields no performance improvement.
    ///
    /// Implementers of [`Input`] must _not_ load more than `count` characters into the buffer. The
    /// parser tracks how many characters are loaded in the buffer and acts accordingly.
    fn lookahead(&mut self, count: usize);

    /// Return the number of buffered characters in `self`.
    #[must_use]
    fn buflen(&self) -> usize;

    /// Return the capacity of the buffer in `self`.
    #[must_use]
    fn bufmaxlen(&self) -> usize;

    /// Return whether the buffer (!= stream) is empty.
    #[inline]
    #[must_use]
    fn buf_is_empty(&self) -> bool {
        self.buflen() == 0
    }

    /// Read a character from the input stream and return it directly.
    ///
    /// The internal buffer (if any) is bypassed.
    #[must_use]
    fn raw_read_ch(&mut self) -> char;

    /// Read a non-breakz a character from the input stream and return it directly.
    ///
    /// The internal buffer (if any) is bypassed.
    ///
    /// If the next character is a breakz, it is either not consumed or placed into the buffer (if
    /// any).
    #[must_use]
    fn raw_read_non_breakz_ch(&mut self) -> Option<char>;

    /// Consume the next character.
    fn skip(&mut self);

    /// Consume the next `count` character.
    fn skip_n(&mut self, count: usize);

    /// Return the next character, without consuming it.
    ///
    /// Users of the [`Input`] must make sure that the character has been loaded through a prior
    /// call to [`Input::lookahead`]. Implementors of [`Input`] may assume that a valid call to
    /// [`Input::lookahead`] has been made beforehand.
    ///
    /// # Return
    /// If the input source is not exhausted, returns the next character to be fed into the
    /// scanner. Otherwise, returns `\0`.
    #[must_use]
    fn peek(&self) -> char;

    /// Return the `n`-th character in the buffer, without consuming it.
    ///
    /// This function assumes that the n-th character in the input has already been fetched through
    /// [`Input::lookahead`].
    #[must_use]
    fn peek_nth(&self, n: usize) -> char;

    /// Return the current byte offset in the underlying source, if available.
    ///
    /// This is an *optional* capability that enables zero-copy (`Cow::Borrowed`) token values
    /// for inputs that keep a stable backing string (notably [`str::StrInput`]).
    ///
    /// The returned value (when `Some`) is the number of bytes that have been consumed so far,
    /// i.e. an offset into the original source string.
    ///
    /// # Correctness contract
    /// Implementations returning `Some(_)` must satisfy all of the following:
    ///
    /// - The offset is a valid UTF-8 boundary in the underlying source.
    /// - The offset is monotonically non-decreasing as characters are consumed.
    /// - The underlying source is stable for the duration of parsing (no reallocation/mutation)
    ///   so that slices returned by [`Input::slice_bytes`] remain valid.
    ///
    /// Inputs that cannot provide stable slicing (e.g. stream/iterator inputs) must return
    /// `None`.
    #[inline]
    #[must_use]
    fn byte_offset(&self) -> Option<usize> {
        None
    }

    /// Return a borrowed slice of the underlying source between two byte offsets.
    ///
    /// This is an *optional* capability used to produce `Cow::Borrowed` values without
    /// allocating.
    ///
    /// `start` and `end` are byte offsets as returned by [`Input::byte_offset`]. The interval is
    /// half-open: `[start, end)`.
    ///
    /// # Correctness contract
    /// Implementations returning `Some(&str)` must ensure:
    ///
    /// - `start <= end`.
    /// - Both offsets are valid UTF-8 boundaries.
    /// - The returned `&str` is a view into the stable underlying source associated with this
    ///   input.
    ///
    /// Implementations that return `None` from [`Input::byte_offset`] must also return `None`
    /// here.
    #[inline]
    #[must_use]
    fn slice_bytes(&self, _start: usize, _end: usize) -> Option<&str> {
        None
    }

    /// Look for the next character and return it.
    ///
    /// The character is not consumed.
    /// Equivalent to calling [`Input::lookahead`] and [`Input::peek`].
    #[inline]
    #[must_use]
    fn look_ch(&mut self) -> char {
        self.lookahead(1);
        self.peek()
    }

    /// Return whether the next character in the input source is equal to `c`.
    ///
    /// This function assumes that the next character in the input has already been fetched through
    /// [`Input::lookahead`].
    #[inline]
    #[must_use]
    fn next_char_is(&self, c: char) -> bool {
        self.peek() == c
    }

    /// Return whether the `n`-th character in the input source is equal to `c`.
    ///
    /// This function assumes that the n-th character in the input has already been fetched through
    /// [`Input::lookahead`].
    #[inline]
    #[must_use]
    fn nth_char_is(&self, n: usize, c: char) -> bool {
        self.peek_nth(n) == c
    }

    /// Return whether the next 2 characters in the input source match the given characters.
    ///
    /// This function assumes that the next 2 characters in the input have already been fetched
    /// through [`Input::lookahead`].
    #[inline]
    #[must_use]
    fn next_2_are(&self, c1: char, c2: char) -> bool {
        assert!(self.buflen() >= 2);
        self.peek() == c1 && self.peek_nth(1) == c2
    }

    /// Return whether the next 3 characters in the input source match the given characters.
    ///
    /// This function assumes that the next 3 characters in the input have already been fetched
    /// through [`Input::lookahead`].
    #[inline]
    #[must_use]
    fn next_3_are(&self, c1: char, c2: char, c3: char) -> bool {
        assert!(self.buflen() >= 3);
        self.peek() == c1 && self.peek_nth(1) == c2 && self.peek_nth(2) == c3
    }

    /// Check whether the next characters correspond to a document indicator.
    ///
    /// This function assumes that the next 4 characters in the input has already been fetched
    /// through [`Input::lookahead`].
    #[inline]
    #[must_use]
    fn next_is_document_indicator(&self) -> bool {
        assert!(self.buflen() >= 4);
        is_blank_or_breakz(self.peek_nth(3))
            && (self.next_3_are('.', '.', '.') || self.next_3_are('-', '-', '-'))
    }

    /// Check whether the next characters correspond to a start of document.
    ///
    /// This function assumes that the next 4 characters in the input has already been fetched
    /// through [`Input::lookahead`].
    #[inline]
    #[must_use]
    fn next_is_document_start(&self) -> bool {
        assert!(self.buflen() >= 4);
        self.next_3_are('-', '-', '-') && is_blank_or_breakz(self.peek_nth(3))
    }

    /// Check whether the next characters correspond to an end of document.
    ///
    /// This function assumes that the next 4 characters in the input has already been fetched
    /// through [`Input::lookahead`].
    #[inline]
    #[must_use]
    fn next_is_document_end(&self) -> bool {
        assert!(self.buflen() >= 4);
        self.next_3_are('.', '.', '.') && is_blank_or_breakz(self.peek_nth(3))
    }

    /// Skip yaml whitespace at most up to eol. Also skips comments. Advances the input.
    ///
    /// # Return
    /// Return a tuple with the number of characters that were consumed and the result of skipping
    /// whitespace. The number of characters returned can be used to advance the index and column,
    /// since no end-of-line character will be consumed.
    /// See [`SkipTabs`] For more details on the success variant.
    ///
    /// # Errors
    /// Errors if a comment is encountered but it was not preceded by a whitespace. In that event,
    /// the first tuple element will contain the number of characters consumed prior to reaching
    /// the `#`.
    fn skip_ws_to_eol(&mut self, skip_tabs: SkipTabs) -> (usize, Result<SkipTabs, &'static str>) {
        let mut encountered_tab = false;
        let mut has_yaml_ws = false;
        let mut chars_consumed = 0;
        loop {
            match self.look_ch() {
                ' ' => {
                    has_yaml_ws = true;
                    self.skip();
                }
                '\t' if skip_tabs != SkipTabs::No => {
                    encountered_tab = true;
                    self.skip();
                }
                // YAML comments must be preceded by whitespace.
                '#' if !encountered_tab && !has_yaml_ws => {
                    return (
                        chars_consumed,
                        Err("comments must be separated from other tokens by whitespace"),
                    );
                }
                '#' => {
                    self.skip(); // Skip over '#'
                    while !is_breakz(self.look_ch()) {
                        self.skip();
                        chars_consumed += 1;
                    }
                }
                _ => break,
            }
            chars_consumed += 1;
        }

        (
            chars_consumed,
            Ok(SkipTabs::Result(encountered_tab, has_yaml_ws)),
        )
    }

    /// Check whether the next characters may be part of a plain scalar.
    ///
    /// This function assumes we are not given a blankz character.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn next_can_be_plain_scalar(&self, in_flow: bool) -> bool {
        let nc = self.peek_nth(1);
        match self.peek() {
            // indicators can end a plain scalar, see 7.3.3. Plain Style
            ':' if is_blank_or_breakz(nc) || (in_flow && is_flow(nc)) => false,
            c if in_flow && is_flow(c) => false,
            _ => true,
        }
    }

    /// Check whether the next character is [a blank] or [a break].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a blank] or [a break], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a blank]: is_blank
    /// [a break]: is_break
    #[inline]
    fn next_is_blank_or_break(&self) -> bool {
        is_blank(self.peek()) || is_break(self.peek())
    }

    /// Check whether the next character is [a blank] or [a breakz].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a blank] or [a break], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a blank]: is_blank
    /// [a breakz]: is_breakz
    #[inline]
    fn next_is_blank_or_breakz(&self) -> bool {
        is_blank(self.peek()) || is_breakz(self.peek())
    }

    /// Check whether the next character is [a blank].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a blank], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a blank]: is_blank
    #[inline]
    fn next_is_blank(&self) -> bool {
        is_blank(self.peek())
    }

    /// Check whether the next character is [a break].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a break], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a break]: is_break
    #[inline]
    fn next_is_break(&self) -> bool {
        is_break(self.peek())
    }

    /// Check whether the next character is [a breakz].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a breakz], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a breakz]: is_breakz
    #[inline]
    fn next_is_breakz(&self) -> bool {
        is_breakz(self.peek())
    }

    /// Check whether the next character is [a z].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a z], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a z]: is_z
    #[inline]
    fn next_is_z(&self) -> bool {
        is_z(self.peek())
    }

    /// Check whether the next character is [a flow].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a flow], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a flow]: is_flow
    #[inline]
    fn next_is_flow(&self) -> bool {
        is_flow(self.peek())
    }

    /// Check whether the next character is [a digit].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a digit], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a digit]: is_digit
    #[inline]
    fn next_is_digit(&self) -> bool {
        is_digit(self.peek())
    }

    /// Check whether the next character is [a letter].
    ///
    /// The character must have previously been fetched through [`lookahead`]
    ///
    /// # Return
    /// Returns true if the character is [a letter], false otherwise.
    ///
    /// [`lookahead`]: Input::lookahead
    /// [a letter]: is_alpha
    #[inline]
    fn next_is_alpha(&self) -> bool {
        is_alpha(self.peek())
    }

    /// Skip characters from the input until a [breakz] is found.
    ///
    /// The characters are consumed from the input.
    ///
    /// # Return
    /// Return the number of characters that were consumed. The number of characters returned can
    /// be used to advance the index and column, since no end-of-line character will be consumed.
    ///
    /// [breakz]: is_breakz
    #[inline]
    fn skip_while_non_breakz(&mut self) -> usize {
        let mut count = 0;
        while !is_breakz(self.look_ch()) {
            count += 1;
            self.skip();
        }
        count
    }

    /// Skip characters from the input while [blanks] are found.
    ///
    /// The characters are consumed from the input.
    ///
    /// # Return
    /// Return the number of characters that were consumed. The number of characters returned can
    /// be used to advance the index and column, since no end-of-line character will be consumed.
    ///
    /// [blanks]: is_blank
    fn skip_while_blank(&mut self) -> usize {
        let mut n_bytes = 0;
        while is_blank(self.look_ch()) {
            n_bytes += self.peek().len_utf8();
            self.skip();
        }
        n_bytes
    }

    /// Fetch characters from the input while we encounter letters and store them in `out`.
    ///
    /// The characters are consumed from the input.
    ///
    /// # Return
    /// Return the number of characters that were consumed. The number of characters returned can
    /// be used to advance the index and column, since no end-of-line character will be consumed.
    fn fetch_while_is_alpha(&mut self, out: &mut String) -> usize {
        let mut n_bytes = 0;
        while is_alpha(self.look_ch()) {
            let c = self.peek();
            n_bytes += c.len_utf8();
            out.push(c);
            self.skip();
        }
        n_bytes
    }

    /// Fetch characters as long as they satisfy `is_yaml_non_space(c)`.
    ///
    /// The characters are consumed from the input.
    ///
    /// # Return
    /// Return the number of characters that were consumed. The number of characters returned can
    /// be used to advance the index and column, since no end-of-line character will be consumed.
    fn fetch_while_is_yaml_non_space(&mut self, out: &mut String) -> usize {
        let mut chars_consumed = 0;
        loop {
            let c = self.look_ch();
            if !crate::char_traits::is_yaml_non_space(c) || is_z(c) {
                break;
            }
            let c = self.peek();
            out.push(c);
            self.skip();
            chars_consumed += 1;
        }
        chars_consumed
    }

    /// Fetch a chunk of plain scalar characters.
    ///
    /// This optimization method allows the input to batch process characters.
    /// Returns (stopped, `chars_consumed`).
    /// stopped is true if the chunk ended because of a non-plain-scalar character.
    fn fetch_plain_scalar_chunk(
        &mut self,
        out: &mut String,
        count: usize,
        flow_level_gt_0: bool,
    ) -> (bool, usize) {
        let mut chars_consumed = 0;
        for _ in 0..count {
            self.lookahead(1);
            if self.next_is_blank_or_breakz() || !self.next_can_be_plain_scalar(flow_level_gt_0) {
                return (true, chars_consumed);
            }
            out.push(self.peek());
            self.skip();
            chars_consumed += 1;
        }
        (false, chars_consumed)
    }
}

/// Behavior to adopt regarding treating tabs as whitespace.
///
/// Although tab is a valid yaml whitespace, it doesn't always behave the same as a space.
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum SkipTabs {
    /// Skip all tabs as whitespace.
    Yes,
    /// Don't skip any tab. Return from the function when encountering one.
    No,
    /// Return value from the function.
    Result(
        /// Whether tabs were encountered.
        bool,
        /// Whether at least 1 valid yaml whitespace has been encountered.
        bool,
    ),
}

impl SkipTabs {
    /// Whether tabs were found while skipping whitespace.
    ///
    /// This function must be called after a call to `skip_ws_to_eol`.
    #[must_use]
    pub fn found_tabs(self) -> bool {
        matches!(self, SkipTabs::Result(true, _))
    }

    /// Whether a valid YAML whitespace has been found in skipped-over content.
    ///
    /// This function must be called after a call to `skip_ws_to_eol`.
    #[must_use]
    pub fn has_valid_yaml_ws(self) -> bool {
        matches!(self, SkipTabs::Result(_, true))
    }
}
