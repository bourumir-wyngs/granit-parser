use crate::{
    input::{str::StrInput, BorrowedInput, BufferedInput},
    parser::{Event, ParseResult, Parser, ParserTrait, SpannedEventReceiver},
    scanner::{ScanError, Span},
};
use alloc::{boxed::Box, string::String, vec::Vec};

/// A lightweight parser that replays a pre-collected event stream.
pub struct ReplayParser<'input> {
    events: Vec<(Event<'input>, Span)>,
    index: usize,
    anchor_offset: usize,
}

impl<'input> ReplayParser<'input> {
    /// Create a parser that replays `events` and starts anchor allocation at `anchor_offset`.
    #[must_use]
    pub fn new(events: Vec<(Event<'input>, Span)>, anchor_offset: usize) -> Self {
        Self {
            events,
            index: 0,
            anchor_offset,
        }
    }

    /// Return the next anchor ID that should be assigned after replayed events.
    #[must_use]
    pub fn get_anchor_offset(&self) -> usize {
        self.anchor_offset
    }

    /// Set the next anchor ID that should be assigned after replayed events.
    pub fn set_anchor_offset(&mut self, offset: usize) {
        self.anchor_offset = offset;
    }

    fn advance_anchor_offset(&mut self, event: &Event<'input>) {
        let anchor_id = match event {
            Event::Scalar(_, _, anchor_id, _)
            | Event::SequenceStart(_, anchor_id, _)
            | Event::MappingStart(_, anchor_id, _) => *anchor_id,
            _ => 0,
        };

        if anchor_id > 0 {
            self.anchor_offset = self.anchor_offset.max(anchor_id.saturating_add(1));
        }
    }
}

impl<'input> ParserTrait<'input> for ReplayParser<'input> {
    fn peek(&mut self) -> Option<Result<&(Event<'input>, Span), ScanError>> {
        self.events.get(self.index).map(Ok)
    }

    fn next_event(&mut self) -> Option<ParseResult<'input>> {
        let event = self.events.get(self.index).cloned()?;
        self.index += 1;
        self.advance_anchor_offset(&event.0);
        Some(Ok(event))
    }

    fn load<R: SpannedEventReceiver<'input>>(
        &mut self,
        recv: &mut R,
        multi: bool,
    ) -> Result<(), ScanError> {
        while let Some(res) = self.next_event() {
            let (ev, span) = res?;
            let is_doc_end = matches!(ev, Event::DocumentEnd);
            let is_stream_end = matches!(ev, Event::StreamEnd);
            recv.on_event(ev, span);
            if is_stream_end {
                break;
            }
            if !multi && is_doc_end {
                break;
            }
        }
        Ok(())
    }
}

/// A wrapper for different types of parsers.
pub enum AnyParser<'input, I, T>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'input>,
{
    /// A parser over borrowed string input.
    String {
        /// Parser currently producing events for this stack entry.
        parser: Parser<'input, StrInput<'input>>,
        /// Human-readable source name returned by [`ParserStack::stack`].
        name: String,
    },
    /// A parser over an iterator of characters.
    Iter {
        /// Parser currently producing events for this stack entry.
        parser: Parser<'static, BufferedInput<I>>,
        /// Human-readable source name returned by [`ParserStack::stack`].
        name: String,
    },
    /// A parser over a custom input.
    Custom {
        /// Parser currently producing events for this stack entry.
        parser: Parser<'input, T>,
        /// Human-readable source name returned by [`ParserStack::stack`].
        name: String,
    },
    /// A parser over a replayed event stream.
    Replay {
        /// Replay parser currently producing pre-collected events for this stack entry.
        parser: ReplayParser<'input>,
        /// Human-readable source name returned by [`ParserStack::stack`].
        name: String,
    },
}

impl<'input, I, T> AnyParser<'input, I, T>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'input>,
{
    fn get_anchor_offset(&self) -> usize {
        match self {
            AnyParser::String { parser, .. } => parser.get_anchor_offset(),
            AnyParser::Iter { parser, .. } => parser.get_anchor_offset(),
            AnyParser::Custom { parser, .. } => parser.get_anchor_offset(),
            AnyParser::Replay { parser, .. } => parser.get_anchor_offset(),
        }
    }

    fn set_anchor_offset(&mut self, offset: usize) {
        match self {
            AnyParser::String { parser, .. } => parser.set_anchor_offset(offset),
            AnyParser::Iter { parser, .. } => parser.set_anchor_offset(offset),
            AnyParser::Custom { parser, .. } => parser.set_anchor_offset(offset),
            AnyParser::Replay { parser, .. } => parser.set_anchor_offset(offset),
        }
    }
}

/// A parser implementation that uses a stack for include-style parsing.
///
/// Note: `ParserStack` deliberately suppresses nested [`Event::StreamStart`] /
/// [`Event::DocumentStart`] events when more than one parser is stacked, and the tests assert
/// outputs where a nested parser starts directly with [`Event::MappingStart`] before the parent
/// stream/document wrapper appears.
///
/// That is exactly what we want for `!include`-style subtree injection.
///
/// Included parser events, including [`Event::Comment`] events, are replayed through the same
/// event stream as parent events. Their [`Span`] values remain local to the included source, just
/// like every other event span from an included parser. `ParserStack` does not attach file names,
/// source IDs, or other include provenance to events or spans.
pub struct ParserStack<'input, I = core::iter::Empty<char>, T = StrInput<'input>>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'input>,
{
    parsers: Vec<AnyParser<'input, I, T>>,
    current: Option<(Event<'input>, Span)>,
    stream_end_emitted: bool,
    #[allow(clippy::type_complexity)]
    include_resolver: Option<Box<dyn FnMut(&str) -> Result<String, ScanError> + 'input>>,
}

impl<'input, I, T> ParserStack<'input, I, T>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'input>,
{
    /// Creates a new, empty parser stack.
    #[must_use]
    pub fn new() -> Self {
        Self {
            parsers: Vec::new(),
            current: None,
            stream_end_emitted: false,
            include_resolver: None,
        }
    }

    /// Set the resolver used by [`Self::resolve`] and [`Self::push_include`].
    ///
    /// The resolver receives the include name and returns the included YAML source text.
    pub fn set_resolver(
        &mut self,
        resolver: impl FnMut(&str) -> Result<String, ScanError> + 'input,
    ) {
        self.include_resolver = Some(Box::new(resolver));
    }

    /// Resolves an include string using the include resolver.
    ///
    /// Comment events from the included content are preserved. Their spans are local to the
    /// included content returned by the resolver, matching the existing behavior for all included
    /// document events.
    ///
    /// # Errors
    /// Returns `ScanError` if no resolver is configured, include resolution fails, or the
    /// included content cannot be parsed.
    pub fn resolve(&mut self, include_str: &str) -> Result<(), ScanError> {
        if let Some(resolver) = &mut self.include_resolver {
            let content = resolver(include_str)?;
            let mut parser = Parser::new_from_iter(content.chars().collect::<Vec<_>>().into_iter());
            if let Some(parent) = self.parsers.last() {
                parser.set_anchor_offset(parent.get_anchor_offset());
            }
            let mut events = Vec::new();
            while let Some(event) = parser.next_event() {
                events.push(event?);
            }

            self.push_replay_parser(
                ReplayParser::new(events, parser.get_anchor_offset()),
                include_str.into(),
            );
            Ok(())
        } else {
            Err(ScanError::new(
                crate::scanner::Marker::new(0, 1, 0),
                String::from("No include resolver set for parser stack."),
            ))
        }
    }

    /// Resolves an include by name and pushes the resulting parser onto the stack.
    ///
    /// This is an alias for [`Self::resolve`] with a name that reads naturally in
    /// include-oriented consumers: `stack.push_include("config.yaml")?`.
    /// Comment spans from the included content are local to that included source.
    ///
    /// # Errors
    /// Returns `ScanError` if no resolver is configured, include resolution fails, or the
    /// included content cannot be parsed.
    pub fn push_include(&mut self, include_name: &str) -> Result<(), ScanError> {
        self.resolve(include_name)
    }

    /// Push a string parser onto the stack.
    ///
    /// The pushed parser inherits the current anchor offset so anchors remain unique across stacked
    /// sources. `name` is returned by [`Self::stack`] for diagnostics.
    pub fn push_str_parser(&mut self, mut parser: Parser<'input, StrInput<'input>>, name: String) {
        if let Some(parent) = self.parsers.last() {
            parser.set_anchor_offset(parent.get_anchor_offset());
        }
        self.parsers.push(AnyParser::String { parser, name });
    }

    /// Push an iterator-backed parser onto the stack.
    ///
    /// The pushed parser inherits the current anchor offset so anchors remain unique across stacked
    /// sources. `name` is returned by [`Self::stack`] for diagnostics.
    pub fn push_iter_parser(
        &mut self,
        mut parser: Parser<'static, BufferedInput<I>>,
        name: String,
    ) {
        if let Some(parent) = self.parsers.last() {
            parser.set_anchor_offset(parent.get_anchor_offset());
        }
        self.parsers.push(AnyParser::Iter { parser, name });
    }

    /// Push a custom-input parser onto the stack.
    ///
    /// The pushed parser inherits the current anchor offset so anchors remain unique across stacked
    /// sources. `name` is returned by [`Self::stack`] for diagnostics.
    pub fn push_custom_parser(&mut self, mut parser: Parser<'input, T>, name: String) {
        if let Some(parent) = self.parsers.last() {
            parser.set_anchor_offset(parent.get_anchor_offset());
        }
        self.parsers.push(AnyParser::Custom { parser, name });
    }

    /// Push a replay parser onto the stack.
    ///
    /// Replay parsers are used for included content that has already been parsed into events.
    /// `name` is returned by [`Self::stack`] for diagnostics.
    pub fn push_replay_parser(&mut self, mut parser: ReplayParser<'input>, name: String) {
        if let Some(parent) = self.parsers.last() {
            let inherited = parent.get_anchor_offset();
            parser.set_anchor_offset(parser.get_anchor_offset().max(inherited));
        }

        self.parsers.push(AnyParser::Replay { parser, name });
    }

    /// Push a custom parser and set the first event that should be returned from it.
    ///
    /// This is used when the caller has already consumed the parser's first event before deciding
    /// to place it on the stack.
    pub fn push_custom_parser_with_current(
        &mut self,
        mut parser: Parser<'input, T>,
        name: String,
        current: (Event<'input>, Span),
    ) {
        if let Some(parent) = self.parsers.last() {
            parser.set_anchor_offset(parent.get_anchor_offset());
        }
        self.parsers.push(AnyParser::Custom { parser, name });
        self.current = Some(current);
    }

    /// Return the anchor offset that a newly pushed parser should inherit.
    #[must_use]
    pub fn current_anchor_offset(&self) -> usize {
        self.parsers.last().map_or(0, AnyParser::get_anchor_offset)
    }

    /// Return the names of the parsers currently in the stack, from bottom to top.
    #[must_use]
    pub fn stack(&self) -> Vec<String> {
        self.parsers
            .iter()
            .map(|p| match p {
                AnyParser::String { name, .. }
                | AnyParser::Iter { name, .. }
                | AnyParser::Custom { name, .. }
                | AnyParser::Replay { name, .. } => name.clone(),
            })
            .collect()
    }

    fn propagate_anchor_offset_from_popped(&mut self, popped: &AnyParser<'input, I, T>) {
        if let Some(parent) = self.parsers.last_mut() {
            let next_offset = parent.get_anchor_offset().max(popped.get_anchor_offset());
            parent.set_anchor_offset(next_offset);
        }
    }

    fn next_event_impl(&mut self) -> Result<(Event<'input>, Span), ScanError> {
        loop {
            let Some(any_parser) = self.parsers.last_mut() else {
                return Ok((
                    Event::StreamEnd,
                    Span::empty(crate::scanner::Marker::new(0, 1, 0)),
                ));
            };

            let res = match any_parser {
                AnyParser::String { parser, .. } => parser.next_event(),
                AnyParser::Iter { parser, .. } => parser.next_event(),
                AnyParser::Custom { parser, .. } => parser.next_event(),
                AnyParser::Replay { parser, .. } => parser.next_event(),
            };

            match res {
                Some(Ok((Event::StreamEnd, span))) => {
                    if self.parsers.len() == 1 {
                        self.parsers.pop();
                        return Ok((Event::StreamEnd, span));
                    }
                    let popped = self.parsers.pop().unwrap();
                    self.propagate_anchor_offset_from_popped(&popped);
                }
                None => {
                    if self.parsers.len() == 1 {
                        self.parsers.pop();
                        return Ok((
                            Event::StreamEnd,
                            Span::empty(crate::scanner::Marker::new(0, 1, 0)),
                        ));
                    }
                    let popped = self.parsers.pop().unwrap();
                    self.propagate_anchor_offset_from_popped(&popped);
                }
                Some(Err(e)) => {
                    let popped = self.parsers.pop().unwrap();
                    self.propagate_anchor_offset_from_popped(&popped);
                    return e.into_result();
                }
                Some(Ok((Event::DocumentEnd, span))) => {
                    if self.parsers.len() == 1 {
                        return Ok((Event::DocumentEnd, span));
                    }

                    // Continue the parent parser if it has more documents.
                    let peek_res = match self.parsers.last_mut().unwrap() {
                        AnyParser::String { parser, .. } => parser.peek(),
                        AnyParser::Iter { parser, .. } => parser.peek(),
                        AnyParser::Custom { parser, .. } => parser.peek(),
                        AnyParser::Replay { parser, .. } => parser.peek(),
                    };

                    match peek_res {
                        Some(Ok((Event::StreamEnd, _))) | None => {
                            let popped = self.parsers.pop().unwrap();
                            self.propagate_anchor_offset_from_popped(&popped);
                        }
                        _ => {
                            return Err(ScanError::new_str(
                                span.start,
                                "multiple documents not supported here",
                            ));
                        }
                    }
                }
                Some(Ok(event)) => {
                    if self.parsers.len() > 1
                        && matches!(event.0, Event::StreamStart | Event::DocumentStart(_))
                    {
                        continue;
                    }
                    return Ok(event);
                }
            }
        }
    }
}

impl<'input, I, T> Default for ParserStack<'input, I, T>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'input>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'input, I, T> ParserTrait<'input> for ParserStack<'input, I, T>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'input>,
{
    fn peek(&mut self) -> Option<Result<&(Event<'input>, Span), ScanError>> {
        if let Some(ref x) = self.current {
            Some(Ok(x))
        } else {
            if self.stream_end_emitted {
                return None;
            }
            match self.next_event_impl() {
                Ok(token) => {
                    self.current = Some(token);
                    Some(Ok(self.current.as_ref().unwrap()))
                }
                Err(e) => Some(e.into_result()),
            }
        }
    }

    fn next_event(&mut self) -> Option<ParseResult<'input>> {
        if let Some(token) = self.current.take() {
            if let Event::StreamEnd = token.0 {
                self.stream_end_emitted = true;
            }
            return Some(Ok(token));
        }
        if self.stream_end_emitted {
            return None;
        }
        match self.next_event_impl() {
            Ok(token) => {
                if let Event::StreamEnd = token.0 {
                    self.stream_end_emitted = true;
                }
                Some(Ok(token))
            }
            Err(e) => Some(e.into_result()),
        }
    }

    fn load<R: SpannedEventReceiver<'input>>(
        &mut self,
        recv: &mut R,
        multi: bool,
    ) -> Result<(), ScanError> {
        while let Some(res) = self.next_event() {
            // Fetch the next event from the active stack entry.
            let (ev, span) = res?;

            // Track whether to stop based on `multi`.
            let is_doc_end = matches!(ev, Event::DocumentEnd);
            let is_stream_end = matches!(ev, Event::StreamEnd);

            recv.on_event(ev, span);

            if is_stream_end {
                break;
            }

            // Stop after one document when multi-document parsing is disabled.
            if !multi && is_doc_end {
                break;
            }
        }

        Ok(())
    }
}

impl<'input, I, T> Iterator for ParserStack<'input, I, T>
where
    I: Iterator<Item = char>,
    T: BorrowedInput<'input>,
{
    type Item = Result<(Event<'input>, Span), ScanError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event()
    }
}
