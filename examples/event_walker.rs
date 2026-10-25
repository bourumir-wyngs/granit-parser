//! Follow YAML node roles without building a document tree.
//!
//! Run with `cargo run --example event_walker`. The same example also works with
//! `--no-default-features`; comment events are then absent from the event stream.
//!
//! A mapping alternates between key and value *nodes*, not between scalar events.
//! Collections and aliases are nodes too. In particular, opening a collection
//! consumes its parent's slot immediately; closing it must not consume another.

use granit_parser::{Event, Options, Parser, StructureStyle};

/// A node's role describes its relationship to its immediate parent.
/// YAML permits a collection or an alias wherever a mapping key can appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Root,
    MappingKey,
    MappingValue,
    SequenceElement,
}

/// Only open collections need stack entries. No scalar values are retained.
#[derive(Debug, PartialEq, Eq)]
enum Frame {
    Sequence,
    Mapping { next_is_key: bool },
}

#[derive(Default)]
struct Walker {
    frames: Vec<Frame>,
    root_seen: bool,
}

impl Walker {
    /// Consume exactly one node position and return its role and depth.
    /// The root is at depth zero; each containing collection adds one level.
    fn take_node_slot(&mut self) -> (Role, usize) {
        let depth = self.frames.len();
        let role = match self.frames.last_mut() {
            Some(Frame::Sequence) => Role::SequenceElement,
            Some(Frame::Mapping { next_is_key }) => {
                let role = if *next_is_key {
                    Role::MappingKey
                } else {
                    Role::MappingValue
                };
                *next_is_key = !*next_is_key;
                role
            }
            None => {
                // These assertions check our bookkeeping against the parser's
                // structural guarantees. Syntax errors come from the parser.
                assert!(!self.root_seen, "a document has only one root node");
                self.root_seen = true;
                Role::Root
            }
        };
        (role, depth)
    }

    /// A node event gets a context; structural and comment events get `None`.
    fn observe(&mut self, event: &Event<'_>) -> Result<Option<(Role, usize)>, &'static str> {
        match event {
            Event::Scalar(..) | Event::Alias(_) | Event::UnresolvedAlias(_) => {
                // Both resolved and unresolved aliases occupy one slot. Neither
                // causes the target's children to be visited by this walker.
                Ok(Some(self.take_node_slot()))
            }
            Event::SequenceStart(..) => {
                // Update the parent BEFORE pushing the children's context.
                let context = self.take_node_slot();
                self.frames.push(Frame::Sequence);
                Ok(Some(context))
            }
            Event::MappingStart(..) => {
                let context = self.take_node_slot();
                self.frames.push(Frame::Mapping { next_is_key: true });
                Ok(Some(context))
            }
            Event::SequenceEnd => {
                assert_eq!(self.frames.pop(), Some(Frame::Sequence));
                // Closing a collection only restores its parent's context.
                // Its parent already advanced when the collection started.
                Ok(None)
            }
            Event::MappingEnd => {
                assert_eq!(
                    self.frames.pop(),
                    Some(Frame::Mapping { next_is_key: true }),
                    "every mapping key must have a value node"
                );
                Ok(None)
            }
            Event::DocumentStart(..) => {
                // Comparing the stack itself shows any unfinished collections on failure.
                assert_eq!(self.frames, [] as [Frame; 0]);
                self.root_seen = false;
                // Anchors are document-scoped too. If your application stores
                // an anchor-ID lookup table, clear it here, even if numeric IDs
                // happen to continue increasing across document boundaries.
                Ok(None)
            }
            Event::DocumentEnd => {
                assert_eq!(self.frames, [] as [Frame; 0]);
                assert!(self.root_seen);
                Ok(None)
            }
            Event::StreamStart | Event::StreamEnd => Ok(None),
            #[cfg(feature = "comments")]
            Event::Comment(..) => {
                // Comments are presentation metadata, so they never advance a
                // mapping or add depth. Their placement is a best-effort hint;
                // event order alone does not determine comment ownership.
                Ok(None)
            }
            // Event is non-exhaustive. A newly introduced event might consume a
            // node slot or change nesting: silently ignoring it could corrupt
            // all subsequent roles. Fail explicitly until this walker supports
            // its meaning. This branch does not handle ordinary YAML errors.
            _ => Err("unrecognized parser event: update the event walker"),
        }
    }
}

/// Short labels keep the expected results below readable.
fn node_label<'event>(event: &'event Event<'_>) -> &'event str {
    match event {
        Event::Scalar(value, ..) => value.as_ref(),
        // Collection notation is already available as StructureStyle. There is
        // no need to infer it from indentation, spans, or source punctuation.
        Event::SequenceStart(StructureStyle::Block, ..) => "block sequence",
        Event::SequenceStart(StructureStyle::Flow, ..) => "flow sequence",
        Event::MappingStart(StructureStyle::Block, ..) => "block mapping",
        Event::MappingStart(StructureStyle::Flow, ..) => "flow mapping",
        Event::Alias(_) => "alias",
        Event::UnresolvedAlias(_) => "unresolved alias",
        _ => unreachable!("observe only returns a context for known node events"),
    }
}

/// Print the input, then each node as it arrives, checking its context against a teaching table.
/// The tables are fixtures, not a collected event stream or a document tree.
fn walk(
    title: &str,
    source: &str,
    options: Options,
    expected: &[(&str, Role, usize)],
) -> Result<(), Box<dyn std::error::Error>> {
    // Show each fixture before its interpretation so readers can follow the node roles.
    println!("\nYAML:\n{source}\n{title}");
    let mut walker = Walker::default();
    let mut expected_nodes = expected.iter();

    for next in Parser::new_from_str_with_options(source, options) {
        // Parse errors propagate to the caller. Keeping role bookkeeping
        // separate makes the same Walker usable with another parser input type.
        let (event, _span) = next?;
        let Some((role, depth)) = walker.observe(&event)? else {
            continue;
        };

        let label = node_label(&event);
        assert_eq!(
            Some(&(label, role, depth)),
            expected_nodes.next(),
            "unexpected role or depth for {event:?}"
        );
        println!("  depth={depth} role={role:?}: {event:?}");

        // Properties belong to the node event; they are not additional nodes.
        // anchor_id() hides the raw zero-means-no-anchor convention for us.
        if let Some(id) = event.anchor_id() {
            println!("    defines anchor ID {id}");
        }
        if let Some(id) = event.alias_id() {
            println!("    references anchor ID {id}");
        }
    }

    assert!(expected_nodes.next().is_none(), "expected more node events");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use Role::{MappingKey, MappingValue, Root, SequenceElement};

    // The small case that catches the common bug: after the sequence value of
    // `a`, `b` must still be a key. A walker that only advances on scalars gets
    // this wrong. The comment must not affect the next node's role either.
    walk(
        "A collection consumes one parent slot",
        "a: [x] # a comment is not a node\nb: y\n",
        Options::default(),
        &[
            ("block mapping", Root, 0),
            ("a", MappingKey, 1),
            ("flow sequence", MappingValue, 1),
            ("x", SequenceElement, 2),
            ("b", MappingKey, 1),
            ("y", MappingValue, 1),
        ],
    )?;

    // A sequence can itself be a mapping key. Its elements have the sequence's
    // context, while the following nested mapping is the outer mapping's value.
    // The deeper block sequence and flow mapping exercise the same rule again.
    walk(
        "Collection keys and nested collection values",
        "? [region, service]\n: \n  enabled: true\n  replicas:\n    - {name: first}\n",
        Options::default(),
        &[
            ("block mapping", Root, 0),
            ("flow sequence", MappingKey, 1),
            ("region", SequenceElement, 2),
            ("service", SequenceElement, 2),
            ("block mapping", MappingValue, 1),
            ("enabled", MappingKey, 2),
            ("true", MappingValue, 2),
            ("replicas", MappingKey, 2),
            ("block sequence", MappingValue, 2),
            ("flow mapping", SequenceElement, 3),
            ("name", MappingKey, 4),
            ("first", MappingValue, 4),
        ],
    )?;

    // YAML aliases describe a graph. The alias used as a mapping key occupies
    // one key slot; the self-reference inside &loop occupies one sequence slot.
    // We print references without expanding them, so cycles are harmless here.
    // The space before `:` separates that indicator from the alias name.
    // A tree-building consumer needs its own policy for cycles, unsupported key
    // shapes, and expansion budgets: parser limits do not bound copied trees.
    walk(
        "Aliases are nodes, including keys and self-references",
        "original: &key name\n*key : value\nloop: &loop [*loop]\n",
        Options::default(),
        &[
            ("block mapping", Root, 0),
            ("original", MappingKey, 1),
            ("name", MappingValue, 1),
            ("alias", MappingKey, 1),
            ("value", MappingValue, 1),
            ("loop", MappingKey, 1),
            ("flow sequence", MappingValue, 1),
            ("alias", SequenceElement, 2),
        ],
    )?;

    // Tooling can opt into unresolved alias events. This keeps structural
    // parsing going but does NOT make these invalid references valid YAML.
    // They still consume slots, even in key position. A later definition does
    // not retroactively resolve an earlier use. The final *key is unresolved
    // because the preceding document's anchors do not carry into the next one.
    walk(
        "Unresolved references and document boundaries",
        "---\n*missing : *later\nafter: &later value\nknown: &key name\n---\n*key\n",
        granit_parser::options! { emit_unresolved_aliases: true },
        &[
            ("block mapping", Root, 0),
            ("unresolved alias", MappingKey, 1),
            ("unresolved alias", MappingValue, 1),
            ("after", MappingKey, 1),
            ("value", MappingValue, 1),
            ("known", MappingKey, 1),
            ("name", MappingValue, 1),
            ("unresolved alias", Root, 0),
        ],
    )?;

    Ok(())
}
