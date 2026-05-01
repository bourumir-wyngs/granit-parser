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
    current: Option<(Event<'input>, Span)>,
    anchor_offset: usize,
}

impl<'input> ReplayParser<'input> {
    /// Creates a new `ReplayParser`.
    #[must_use]
    pub fn new(events: Vec<(Event<'input>, Span)>, anchor_offset: usize) -> Self {
        Self {
            events,
            index: 0,
            current: None,
            anchor_offset,
        }
    }

    /// Get the current anchor offset count.
    #[must_use]
    pub fn get_anchor_offset(&self) -> usize {
        self.anchor_offset
    }

    /// Set the current anchor offset count.
    pub fn set_anchor_offset(&mut self, offset: usize) {
        self.anchor_offset = offset;
    }

    fn advance_anchor_offset(&mut self, event: &Event<'input>) {
        let anchor_id = match event {
            Event::Scalar(_, _, anchor_id, _)
            | Event::SequenceStart(anchor_id, _)
            | Event::MappingStart(anchor_id, _) => *anchor_id,
            _ => 0,
        };

        if anchor_id > 0 {
            self.anchor_offset = self.anchor_offset.max(anchor_id.saturating_add(1));
        }
    }
}

impl<'input> ParserTrait<'input> for ReplayParser<'input> {
    fn peek(&mut self) -> Option<Result<&(Event<'input>, Span), ScanError>> {
        if self.current.is_none() {
            self.current = self.events.get(self.index).cloned();
        }
        self.current.as_ref().map(Ok)
    }

    fn next_event(&mut self) -> Option<ParseResult<'input>> {
        if let Some(current) = self.current.take() {
            self.index += 1;
            self.advance_anchor_offset(&current.0);
            return Some(Ok(current));
        }
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
    /// A parser over a string input.
    String {
        /// The parser itself.
        parser: Parser<'input, StrInput<'input>>,
        /// The name of the parser.
        name: String,
    },
    /// A parser over an iterator input.
    Iter {
        /// The parser itself.
        parser: Parser<'static, BufferedInput<I>>,
        /// The name of the parser.
        name: String,
    },
    /// A parser over a custom input.
    Custom {
        /// The parser itself.
        parser: Parser<'input, T>,
        /// The name of the parser.
        name: String,
    },
    /// A parser over a replayed event stream.
    Replay {
        /// The replay parser itself.
        parser: ReplayParser<'input>,
        /// The name of the parser.
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

/// A parser implementation that utilizes a stack for parsing.
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

    /// Sets the include resolver for this stack.
    pub fn set_resolver(
        &mut self,
        resolver: impl FnMut(&str) -> Result<String, ScanError> + 'input,
    ) {
        self.include_resolver = Some(Box::new(resolver));
    }

    /// Resolves an include string using the include resolver.
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

            self.parsers.push(AnyParser::Replay {
                parser: ReplayParser::new(events, parser.get_anchor_offset()),
                name: include_str.into(),
            });
            Ok(())
        } else {
            Err(ScanError::new(
                crate::scanner::Marker::new(0, 1, 0),
                String::from("No include resolver set for parser stack."),
            ))
        }
    }

    /// Pushes a string parser onto the stack.
    pub fn push_str_parser(&mut self, mut parser: Parser<'input, StrInput<'input>>, name: String) {
        if let Some(parent) = self.parsers.last() {
            parser.set_anchor_offset(parent.get_anchor_offset());
        }
        self.parsers.push(AnyParser::String { parser, name });
    }

    /// Pushes an iterator parser onto the stack.
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

    /// Pushes a custom parser onto the stack.
    pub fn push_custom_parser(&mut self, mut parser: Parser<'input, T>, name: String) {
        if let Some(parent) = self.parsers.last() {
            parser.set_anchor_offset(parent.get_anchor_offset());
        }
        self.parsers.push(AnyParser::Custom { parser, name });
    }

    /// Pushes a replay parser onto the stack.
    pub fn push_replay_parser(&mut self, parser: ReplayParser<'input>, name: String) {
        self.parsers.push(AnyParser::Replay { parser, name });
    }

    /// Pushes a custom parser onto the stack and primes the next event to be returned from it.
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

    /// Returns the anchor offset that a newly pushed parser should inherit.
    #[must_use]
    pub fn current_anchor_offset(&self) -> usize {
        self.parsers.last().map_or(0, AnyParser::get_anchor_offset)
    }

    /// Returns the names of the parsers currently in the stack.
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
                    if let Some(parent) = self.parsers.last_mut() {
                        parent.set_anchor_offset(popped.get_anchor_offset());
                    }
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
                    if let Some(parent) = self.parsers.last_mut() {
                        parent.set_anchor_offset(popped.get_anchor_offset());
                    }
                }
                Some(Err(e)) => {
                    let popped = self.parsers.pop().unwrap();
                    if let Some(parent) = self.parsers.last_mut() {
                        parent.set_anchor_offset(popped.get_anchor_offset());
                    }
                    return Err(e);
                }
                Some(Ok((Event::DocumentEnd, span))) => {
                    if self.parsers.len() == 1 {
                        return Ok((Event::DocumentEnd, span));
                    }

                    // Check if it has more documents
                    let peek_res = match self.parsers.last_mut().unwrap() {
                        AnyParser::String { parser, .. } => parser.peek(),
                        AnyParser::Iter { parser, .. } => parser.peek(),
                        AnyParser::Custom { parser, .. } => parser.peek(),
                        AnyParser::Replay { parser, .. } => parser.peek(),
                    };

                    match peek_res {
                        Some(Ok((Event::StreamEnd, _))) | None => {
                            let popped = self.parsers.pop().unwrap();
                            if let Some(parent) = self.parsers.last_mut() {
                                parent.set_anchor_offset(popped.get_anchor_offset());
                            }
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
                Err(e) => Some(Err(e)),
            }
        }
    }

    fn next_event(&mut self) -> Option<ParseResult<'input>> {
        if self.current.is_some() {
            return self.current.take().map(Ok);
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
            Err(e) => Some(Err(e)),
        }
    }

    fn load<R: SpannedEventReceiver<'input>>(
        &mut self,
        recv: &mut R,
        multi: bool,
    ) -> Result<(), ScanError> {
        while let Some(res) = self.next_event() {
            // Fetch the next event, which is properly synced across the stack
            let (ev, span) = res?;

            // Track if we need to stop based on `multi`
            let is_doc_end = matches!(ev, Event::DocumentEnd);
            let is_stream_end = matches!(ev, Event::StreamEnd);

            recv.on_event(ev, span);

            if is_stream_end {
                break;
            }

            // If we only want a single document and we just reached the end of one, stop
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
