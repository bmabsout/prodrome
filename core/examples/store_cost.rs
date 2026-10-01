//! What an append and a read cost against a store of a host's size: 3,500
//! records of about 5 kB each, 17.5 MB, over 100 todos. Run BY HAND, in
//! release (`cargo run --release --example store_cost -p prodrome-core`);
//! nothing under `cargo test` runs it.
//!
//! The store is laid out by writing each object's file directly, every
//! record a change over the one before it to its todo's content: exactly
//! what appends would have written, without paying for them.
//!
//! Four numbers, each the median of `ROUNDS`:
//!
//! - a COLD read: `dag()` from a handle that has read nothing;
//! - a WARM read: `dag()` again, after another writer added one object;
//! - a COLD append: one append from a handle that has read nothing;
//! - a WARM append: one more append through a handle that has appended.

use std::fs;
use std::time::{Duration, Instant};

use prodrome::change::mk_change;
use prodrome::event::{canonical_envelope, seal_hash, Envelope, Hash, TodoEvent};
use prodrome::literal::Datetime;
use prodrome::policy::Untrusted;
use prodrome::reference::{mk_authored, Todo};
use prodrome::store::EventStore;

type Store = EventStore<TodoEvent<Todo>>;

const RECORDS: usize = 3_500;
const TODOS: usize = 100;
const BODY: usize = 4_600;
const ROUNDS: usize = 15;

fn record(todo: usize, n: usize) -> TodoEvent<Todo> {
    let at = Datetime::new(2026, 9, 1 + (n % 28) as u32, 12, 0, 0, 0).expect("a real instant");
    let body = format!("{n} {}", "x".repeat(BODY));
    mk_authored(
        &format!("todo-{todo}"),
        at,
        "writer",
        "todo",
        at,
        &body,
        None,
        vec![],
        "",
        "",
        "",
        None,
        vec![],
        vec![],
        "",
    )
    .expect("valid")
}

/// The store, laid out as `RECORDS` appends would have left it.
fn lay_out(store: &Store) -> (Hash, usize) {
    let genesis = store.init("cost").expect("begins");
    let objects = store.root().join("objects");
    let mut last: Vec<Option<Hash>> = vec![None; TODOS];
    let mut bytes = 0;
    for n in 0..RECORDS {
        let todo = n % TODOS;
        let deps = last[todo].iter().cloned().collect();
        let object =
            Envelope::Change(mk_change(genesis.clone(), deps, record(todo, n)).expect("valid"));
        let name = seal_hash(&object);
        let print = canonical_envelope(&object);
        bytes += print.len();
        fs::write(objects.join(format!("{}.py", name.as_str())), print).expect("writes");
        last[todo] = Some(name);
    }
    (genesis, bytes)
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn timed<T>(work: impl FnOnce() -> T) -> Duration {
    let start = Instant::now();
    work();
    start.elapsed()
}

fn main() {
    let root = std::env::temp_dir().join(format!("prodrome-store-cost-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (_, bytes) = lay_out(&Store::new(&root, Untrusted::none()));
    println!(
        "a store of {} objects, {:.1} MB",
        RECORDS + 1,
        bytes as f64 / 1e6
    );

    let mut n = RECORDS;
    let mut next = || {
        n += 1;
        record(n % TODOS, n)
    };

    let cold_read = median(
        (0..ROUNDS)
            .map(|_| timed(|| Store::new(&root, Untrusted::none()).dag().expect("reads")))
            .collect(),
    );
    let warm = Store::new(&root, Untrusted::none());
    warm.dag().expect("reads");
    let other = Store::new(&root, Untrusted::none());
    let warm_read = median(
        (0..ROUNDS)
            .map(|_| {
                other.append(next()).expect("appends");
                timed(|| warm.dag().expect("reads"))
            })
            .collect(),
    );
    let cold_append = median(
        (0..ROUNDS)
            .map(|_| {
                let event = next();
                timed(|| {
                    Store::new(&root, Untrusted::none())
                        .append(event)
                        .expect("appends")
                })
            })
            .collect(),
    );
    let writer = Store::new(&root, Untrusted::none());
    writer.append(next()).expect("appends");
    let warm_append = median(
        (0..ROUNDS)
            .map(|_| {
                let event = next();
                timed(|| writer.append(event).expect("appends"))
            })
            .collect(),
    );

    for (what, took) in [
        ("cold read", cold_read),
        ("warm read", warm_read),
        ("cold append", cold_append),
        ("warm append", warm_append),
    ] {
        println!("{what:>12}: {:8.2} ms", took.as_secs_f64() * 1e3);
    }
    let _ = fs::remove_dir_all(&root);
}
