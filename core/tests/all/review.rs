//! A second schema, in the tests only: a REVIEW, whose one register is a
//! machine ordered by "further along", `Draft < Review < Merged` and
//! `Draft < Closed`, declared inflationary, with no valuation. It shows that
//! a schema needs no FPL: it is stored, folded and verified through the same
//! store and fold as the todo's, and the append path refuses a move back
//! (design §3.1, law 4).

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{mk_created, parse_envelope, Actor, Hash, TodoEvent};
use prodrome::fold::{
    maximal, read, Frontier, Inflationary, Order, Product, Register, RegisterType,
};
use prodrome::literal::{Call, Datetime, ProdromeError, Signature, Table, Value, Vocabulary};
use prodrome::payload::{datetime_field, string_field};
use prodrome::policy::Everything;
use prodrome::reference::Todo;
use prodrome::registers::Stamp;
use prodrome::schema::Schema;
use prodrome::store::EventStore;
use proptest::prelude::*;

/// Further along is greater; `Merged` and `Closed` are the one incomparable
/// pair, so a merge on one replica and a close on another is the one real
/// conflict. No `PartialOrd`: [`Order`] is the only order it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Draft,
    Review,
    Merged,
    Closed,
}

pub const PHASES: [Phase; 4] = [Phase::Draft, Phase::Review, Phase::Merged, Phase::Closed];

impl Order for Phase {
    fn le(&self, other: &Self) -> bool {
        use Phase::*;
        matches!(
            (self, other),
            (Draft, _) | (Review, Review | Merged) | (Merged, Merged) | (Closed, Closed)
        )
    }
}

impl Inflationary for Phase {}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Draft => "draft",
            Phase::Review => "review",
            Phase::Merged => "merged",
            Phase::Closed => "closed",
        }
    }

    fn parse(text: &str) -> Result<Phase, ProdromeError> {
        PHASES
            .into_iter()
            .find(|phase| phase.as_str() == text)
            .ok_or_else(|| ProdromeError::invalid(format!("Moved.phase: no phase {text:?}")))
    }
}

/// A review's name: `^[a-z0-9-]+$`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pr(String);

impl Pr {
    pub fn new(text: &str) -> Result<Pr, ProdromeError> {
        let named = !text.is_empty()
            && text
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if named {
            Ok(Pr(text.to_owned()))
        } else {
            Err(ProdromeError::invalid(format!(
                "Pr must match ^[a-z0-9-]+$, got {text:?}"
            )))
        }
    }
}

/// The review schema's events: a review opened, and a review moved to a
/// phase. Nothing else is a value of the type, so nothing else parses.
#[derive(Debug, Clone, PartialEq)]
pub enum Review {
    Opened {
        pr: Pr,
        at: Datetime,
        actor: Actor,
        title: String,
    },
    Moved {
        pr: Pr,
        at: Datetime,
        actor: Actor,
        phase: Phase,
    },
}

pub fn opened(pr: &str, at: Datetime, actor: &str, title: &str) -> Review {
    Review::Opened {
        pr: Pr::new(pr).expect("a review name"),
        at,
        actor: Actor::new(actor).expect("an actor"),
        title: title.to_owned(),
    }
}

pub fn moved(pr: &str, at: Datetime, actor: &str, phase: Phase) -> Review {
    Review::Moved {
        pr: Pr::new(pr).expect("a review name"),
        at,
        actor: Actor::new(actor).expect("an actor"),
        phase,
    }
}

const REVIEW_SIGNATURES: &[(&str, &[&str])] = &[
    ("Opened", &["pr", "at", "actor", "title"]),
    ("Moved", &["pr", "at", "actor", "phase"]),
];

#[derive(Default)]
pub struct ReviewVocabulary;

impl Vocabulary for ReviewVocabulary {
    fn signature(&self, name: &str) -> Option<Signature<'_>> {
        Table(REVIEW_SIGNATURES).find(name)
    }
}

/// The one register a write supersedes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    Phase,
}

/// The phase register's type: a [`Phase`], under its order.
pub struct PhaseRegister;

impl RegisterType<Review> for PhaseRegister {
    type Value<'e> = Phase;

    fn value(event: &Review) -> Option<Phase> {
        match event {
            Review::Moved { phase, .. } => Some(*phase),
            Review::Opened { .. } => None,
        }
    }
}

/// A review's registers: a product of one, its phase.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Phases<'a>(Frontier<'a, Review>);

impl Phases<'_> {
    /// The reading: the maximal phases of the frontier.
    pub fn phases(&self) -> HashSet<Phase> {
        self.0
            .reading::<PhaseRegister>()
            .into_iter()
            .filter_map(|stamp| PhaseRegister::value(&stamp.event))
            .collect()
    }
}

impl<'a> Product<'a> for Phases<'a> {
    type Schema = Review;

    fn join(&mut self, stamp: &'a Stamp<Review>) {
        if PhaseRegister::value(&stamp.event).is_some() {
            self.0.join(stamp);
        }
    }

    fn frontier(&self, _phase: Field) -> &Frontier<'a, Review> {
        &self.0
    }

    fn frontier_mut(&mut self, _phase: Field) -> &mut Frontier<'a, Review> {
        &mut self.0
    }

    fn reading(&self, _phase: Field) -> Vec<&'a Stamp<Review>> {
        self.0.reading::<PhaseRegister>()
    }

    /// The phase is inflationary: a move must not fall below, or beside,
    /// the phase it supersedes.
    fn grows(&self, event: &Review) -> Result<(), ProdromeError> {
        self.0.grows::<PhaseRegister>(event)
    }
}

fn field(name: &str, value: Value) -> (String, Value) {
    (name.to_owned(), value)
}

impl Schema for Review {
    type Vocabulary = ReviewVocabulary;
    type Key = Pr;
    type Register = Field;

    const REGISTERS: &'static [Field] = &[Field::Phase];

    type Registers<'a> = Phases<'a>;

    fn to_value(&self) -> Value {
        match self {
            Review::Opened {
                pr,
                at,
                actor,
                title,
            } => Value::call(
                "Opened",
                vec![
                    field("pr", Value::str(pr.0.as_str())),
                    field("at", Value::Datetime(*at)),
                    field("actor", Value::str(actor.as_str())),
                    field("title", Value::str(title.as_str())),
                ],
            ),
            Review::Moved {
                pr,
                at,
                actor,
                phase,
            } => Value::call(
                "Moved",
                vec![
                    field("pr", Value::str(pr.0.as_str())),
                    field("at", Value::Datetime(*at)),
                    field("actor", Value::str(actor.as_str())),
                    field("phase", Value::str(phase.as_str())),
                ],
            ),
        }
    }

    fn from_value(value: &Value) -> Result<Review, ProdromeError> {
        let call: &Call = value
            .as_call()
            .ok_or_else(|| ProdromeError::invalid("a review event is a constructor call"))?;
        let pr = Pr::new(&string_field(call, "pr")?)?;
        let at = datetime_field(call, "at")?;
        let actor = Actor::new(string_field(call, "actor")?)?;
        match call.name.as_str() {
            "Opened" => Ok(Review::Opened {
                pr,
                at,
                actor,
                title: string_field(call, "title")?,
            }),
            "Moved" => Ok(Review::Moved {
                pr,
                at,
                actor,
                phase: Phase::parse(&string_field(call, "phase")?)?,
            }),
            other => Err(ProdromeError::invalid(format!(
                "{other} is not a review event"
            ))),
        }
    }

    fn key(&self) -> &Pr {
        match self {
            Review::Opened { pr, .. } | Review::Moved { pr, .. } => pr,
        }
    }

    fn at(&self) -> Datetime {
        match self {
            Review::Opened { at, .. } | Review::Moved { at, .. } => *at,
        }
    }

    fn actor(&self) -> &Actor {
        match self {
            Review::Opened { actor, .. } | Review::Moved { actor, .. } => actor,
        }
    }

    fn writes(&self) -> impl Iterator<Item = Field> {
        PhaseRegister::value(self).map(|_| Field::Phase).into_iter()
    }
}

pub fn day(day: u32) -> Datetime {
    Datetime::new(2026, 10, day, 12, 0, 0, 0).expect("a real instant")
}

type Store = EventStore<Review, Everything>;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A store at the review schema, removed when dropped.
struct Scratch(Store);

impl Scratch {
    fn new() -> Scratch {
        let root: PathBuf = std::env::temp_dir().join(format!(
            "prodrome-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        Scratch(Store::new(root, Everything))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.root());
    }
}

/// The phases a store reads for one review.
fn phases(store: &Store, pr: &str) -> HashSet<Phase> {
    let nodes = store.dag().expect("reads").nodes().expect("orders");
    let state = prodrome::registers::fold(&nodes);
    let pr = Pr::new(pr).expect("a review name");
    let phases = state
        .entities()
        .find(|(key, _)| **key == pr)
        .map(|(_, stream)| read(stream, None, &Everything).phases())
        .unwrap_or_default();
    phases
}

fn objects(store: &Store) -> usize {
    store.dag().expect("reads").objects().len()
}

/// Law 4 through the real append path: a move below, or beside, the phase
/// it supersedes is refused before any object exists, and a move further
/// along is written over it.
#[test]
fn the_append_refuses_a_move_back() {
    let scratch = Scratch::new();
    let store = &scratch.0;
    store.init("reviews").expect("begins");
    store
        .append(opened("pr-1", day(1), "ana", "a change"))
        .expect("an opening writes no register");
    store
        .append(moved("pr-1", day(2), "ana", Phase::Review))
        .expect("the first move supersedes nothing");
    let held = objects(store);

    let back = store.append(moved("pr-1", day(3), "ana", Phase::Draft));
    assert!(back.is_err(), "Draft is below Review");
    assert_eq!(objects(store), held, "a refused append writes nothing");

    store
        .append(moved("pr-1", day(4), "ana", Phase::Merged))
        .expect("Merged is above Review");
    let beside = store.append(moved("pr-1", day(5), "ana", Phase::Closed));
    assert!(beside.is_err(), "Closed is beside Merged, not above it");
    assert_eq!(phases(store, "pr-1"), HashSet::from([Phase::Merged]));

    store
        .append(moved("pr-2", day(5), "ana", Phase::Closed))
        .expect("another review's register is its own");
    assert!(store.verify().is_empty(), "{:?}", store.verify());
}

/// The schema is the parse: a review store reads no todo object, and a todo
/// print is not a review event.
#[test]
fn an_event_the_schema_does_not_name_is_refused_at_the_boundary() {
    let todo =
        "Created(todo='alpha', at=datetime(2026, 10, 1, 12, 0, 0), actor='ana', text='', note='')";
    assert!(prodrome::event::parse_event::<Review>(todo).is_err());
    let envelope = format!("Sealed(prev='', event={todo})");
    assert!(parse_envelope::<Review>(&envelope).is_err());
    let review =
        "Moved(pr='pr-1', at=datetime(2026, 10, 1, 12, 0, 0), actor='ana', phase='merged')";
    assert_eq!(
        prodrome::event::parse_event::<Review>(review),
        Ok(moved("pr-1", day(1), "ana", Phase::Merged))
    );
    assert_eq!(
        prodrome::event::canonical(&moved("pr-1", day(1), "ana", Phase::Merged)),
        review
    );
    let unnamed = "Moved(pr='pr-1', at=datetime(2026, 10, 1, 12, 0, 0), actor='ana', phase='lost')";
    assert!(prodrome::event::parse_event::<Review>(unnamed).is_err());

    let scratch = Scratch::new();
    let store = &scratch.0;
    store.init("reviews").expect("begins");
    let todos: EventStore<TodoEvent<Todo>, Everything> = EventStore::new(store.root(), Everything);
    todos
        .append(mk_created("alpha", day(1), "ana", "", "").expect("valid"))
        .expect("a genesis is any schema's");
    assert!(
        store.dag().is_err(),
        "a todo object is not a review store's"
    );
}

fn a_phase() -> impl Strategy<Value = Phase> {
    prop::sample::select(PHASES.to_vec())
}

/// Each move appended in turn, a refused one skipped: what a writer who
/// tries them all is left with.
fn try_all(store: &Store, from: u32, phases: &[Phase]) {
    for (i, phase) in phases.iter().enumerate() {
        let at = day(from + u32::try_from(i).expect("a few"));
        let _ = store.append(moved("pr-1", at, "ana", *phase));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Law 4, the homomorphism: two replicas that each only grew, merged by
    /// putting their objects in one directory, read the join of their
    /// readings, the maximal phases of both.
    #[test]
    fn the_reading_of_a_union_is_the_join_of_the_readings(
        shared in prop::collection::vec(a_phase(), 0..3),
        mine in prop::collection::vec(a_phase(), 0..4),
        theirs in prop::collection::vec(a_phase(), 0..4),
    ) {
        let (here, there) = (Scratch::new(), Scratch::new());
        let genesis = here.0.init("reviews").expect("begins");
        try_all(&here.0, 1, &shared);
        let tips: Vec<Hash> = here.0.tips().expect("derive").into_iter().collect();
        for tip in std::iter::once(&genesis).chain(&tips) {
            there.0.adopt(&here.0, tip).expect("adopts");
        }
        try_all(&here.0, 10, &mine);
        try_all(&there.0, 10, &theirs);
        let (a, b) = (phases(&here.0, "pr-1"), phases(&there.0, "pr-1"));
        for entry in fs::read_dir(there.0.root().join("objects")).expect("lists") {
            let path = entry.expect("an entry").path();
            fs::copy(&path, here.0.root().join("objects").join(path.file_name().expect("a name")))
                .expect("copies");
        }
        let both: Vec<Phase> = a.iter().chain(&b).copied().collect();
        let joined: HashSet<Phase> = maximal(&both, |phase| phase).into_iter().collect();
        prop_assert_eq!(phases(&here.0, "pr-1"), joined);
        prop_assert!(here.0.verify().is_empty());
    }
}
