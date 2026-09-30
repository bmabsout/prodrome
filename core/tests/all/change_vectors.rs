//! SPEC §3 over `conformance/change.py`: each case's writes, EXACTLY — every
//! answer, every object's print and name — and what the store then verifies
//! and prices. Written by hand from the laws `change.rs` checks.

use crate::common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::vectors::{boolean, each, field, integer, moment_at, strings, text_at, vectors};
use prodrome::event::{parse_event, Actor, Hash};
use prodrome::literal::Value;
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;
use prodrome::store::EventStore;
use prodrome::view;

type Store = EventStore<Todo, Untrusted>;

fn roster() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("valid")])
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn put(root: &Path, object: &Value) {
    let path = root
        .join("objects")
        .join(format!("{}.py", text_at(object, "name")));
    fs::write(path, text_at(object, "literal")).expect("writes an object");
}

fn names(store: &Store) -> BTreeSet<String> {
    let dag = store.dag().expect("reads");
    dag.objects()
        .keys()
        .map(|name| name.as_str().to_owned())
        .collect()
}

/// The case's store: its `base`, over `dag.py`'s store `dag` where it names one.
fn begun(case: &Value) -> Store {
    let root = std::env::temp_dir().join(format!(
        "prodrome-change-vectors-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("objects")).expect("creates the store");
    if let Value::Int(seed) = field(case, "dag") {
        let dags = vectors("dag.py");
        let dag = each(&dags, "dags")
            .iter()
            .find(|dag| integer(field(dag, "seed")) == seed.as_i64().expect("a seed"))
            .expect("the named dag.py store");
        for object in each(dag, "objects") {
            put(&root, object);
        }
    }
    for object in each(case, "base") {
        put(&root, object);
    }
    Store::new(root, roster())
}

#[test]
fn every_case_writes_its_objects_and_reads_its_prices() {
    let data = vectors("change.py");
    let cases = each(&data, "cases");
    assert_eq!(cases.len(), 7, "the vector file lost cases");
    for case in cases {
        let title = text_at(case, "name");
        let store = begun(case);
        let before = names(&store);
        for step in each(case, "steps") {
            let handle = match text_at(step, "genesis") {
                "" => store.clone(),
                genesis => store
                    .clone()
                    .in_genesis(Hash::new(genesis.to_owned()).expect("a name")),
            };
            let held = names(&store).len();
            let answer = match text_at(step, "op") {
                "append" => {
                    let event = parse_event(text_at(step, "event")).expect("an event");
                    handle.append(event)
                }
                "snapshot" => handle.snapshot(),
                other => panic!("{title}: unknown op {other:?}"),
            };
            match text_at(step, "answer") {
                "" => assert!(answer.is_err(), "{title}: refused"),
                expected => assert_eq!(
                    answer.expect("answers").as_str(),
                    expected,
                    "{title}: {step:?}"
                ),
            }
            let wrote = usize::from(boolean(field(step, "writes")));
            assert_eq!(names(&store).len(), held + wrote, "{title}: {step:?}");
        }

        let written: BTreeSet<String> = names(&store).difference(&before).cloned().collect();
        let expected: BTreeSet<String> = each(case, "objects")
            .iter()
            .map(|object| text_at(object, "name").to_owned())
            .collect();
        assert_eq!(written, expected, "{title}: objects");
        for object in each(case, "objects") {
            let path = store
                .root()
                .join("objects")
                .join(format!("{}.py", text_at(object, "name")));
            let bytes = fs::read_to_string(path).expect("reads");
            assert_eq!(bytes, text_at(object, "literal"), "{title}: print");
        }
        let found: Vec<String> = store.verify().iter().map(ToString::to_string).collect();
        assert_eq!(found, strings(case, "verify"), "{title}: verify");

        let nodes = store.dag().and_then(|dag| dag.nodes()).expect("reads");
        let rows = view::entries(&nodes, moment_at(case, "at"), &roster()).expect("folds");
        for price in each(case, "prices") {
            let genesis = match text_at(price, "genesis") {
                "" => None,
                named => Some(Hash::new(named.to_owned()).expect("a name")),
            };
            let todo = text_at(price, "todo");
            let row = rows
                .iter()
                .find(|row| row.genesis == genesis && row.todo.as_str() == todo)
                .unwrap_or_else(|| panic!("{title}: no row for {todo}"));
            assert_eq!(row.state(), text_at(price, "state"), "{title}: {todo}");
            match field(price, "value") {
                Value::None => {}
                Value::Str(absent) => {
                    assert_eq!(absent, "absent");
                    assert_eq!(row.value(), Ok(None), "{title}: {todo}");
                }
                value => common::reading(todo, row.value().expect("links"), value)
                    .unwrap_or_else(|e| panic!("{title}: {e}")),
            }
        }
        let _ = fs::remove_dir_all(store.root());
    }
}
