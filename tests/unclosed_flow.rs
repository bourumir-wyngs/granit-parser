use granit_parser::{Event, EventReceiver, Parser};

struct Collector(Vec<Event<'static>>);
impl EventReceiver<'static> for Collector {
    fn on_event(&mut self, ev: Event<'static>) {
        self.0.push(ev);
    }
}

#[test]
fn test_unclosed_flow_sequence_at_eof() {
    let input = "  [";
    let mut parser = Parser::new_from_str(input);
    let mut collector = Collector(Vec::new());
    let res = parser.load(&mut collector, false);

    println!("Events: {:?}", collector.0);
    println!("Result: {:?}", res);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.info(), "unclosed bracket '['");
    assert_eq!(err.marker().index(), 2);
}

#[test]
fn test_unclosed_flow_mapping_at_eof() {
    let input = "   {";
    let mut parser = Parser::new_from_str(input);
    let mut collector = Collector(Vec::new());
    let res = parser.load(&mut collector, false);

    println!("Events: {:?}", collector.0);
    println!("Result: {:?}", res);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.info(), "unclosed bracket '{'");
    assert_eq!(err.marker().index(), 3);
}

#[test]
fn test_unclosed_implicit_flow_mapping_at_eof() {
    let input = " [ a:";
    let mut parser = Parser::new_from_str(input);
    let mut collector = Collector(Vec::new());
    let res = parser.load(&mut collector, false);

    println!("Events: {:?}", collector.0);
    println!("Result: {:?}", res);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.info(), "unclosed bracket '['");
    assert_eq!(err.marker().index(), 1);
}

#[test]
fn test_unclosed_quoted_scalar_at_eof() {
    let input = "     \"abc";
    let mut parser = Parser::new_from_str(input);
    let mut collector = Collector(Vec::new());
    let res = parser.load(&mut collector, false);

    println!("Events: {:?}", collector.0);
    println!("Result: {:?}", res);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.info(), "unclosed quote");
    assert_eq!(err.marker().index(), 5);
}
