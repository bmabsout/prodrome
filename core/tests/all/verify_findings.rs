//! Every finding `EventStore::verify` can state, pinned as exact strings.
//!
//! A cycle cannot be written: an object's name is the hash of bytes that name
//! its parents, so no object set on disk reaches that finding.

use std::fs;
use std::path::{Path, PathBuf};

use prodrome::change::mk_change;
use prodrome::event::{
    canonical_envelope, mk_cancelled, mk_completed, mk_created, mk_reopened, mk_sealed, seal_hash,
    Actor, Envelope, Hash, TodoEvent,
};
use prodrome::genesis::mk_genesis;
use prodrome::literal::Datetime;
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;
use prodrome::snapshot::mk_snapshot;
use prodrome::store::EventStore;
use sha2::{Digest, Sha256};

type Store = EventStore<Todo, Untrusted>;

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
}

fn scratch(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("prodrome-findings-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("objects")).expect("creates the store");
    root
}

fn store(root: &Path) -> Store {
    Store::new(root, Untrusted::of([Actor::new("triage").expect("valid")]))
}

/// What `verify` says, as the sentences a reader gets.
fn findings(root: &Path) -> Vec<String> {
    store(root)
        .verify()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn name_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn put(root: &Path, name: &str, bytes: &[u8]) {
    fs::write(root.join("objects").join(format!("{name}.py")), bytes).expect("writes");
}

fn write(root: &Path, object: &Envelope<Todo>) -> String {
    let name = seal_hash(object).as_str().to_owned();
    put(root, &name, canonical_envelope(object).as_bytes());
    name
}

fn created(prev: Option<&str>, actor: &str, day: u32) -> Envelope<Todo> {
    mk_sealed(
        prev.map(|name| Hash::new(name.to_owned()).expect("a name")),
        mk_created("alpha", at(day), actor, "", "").expect("valid"),
    )
}

#[test]
fn a_healthy_store_has_no_finding() {
    let root = scratch("healthy");
    let genesis = write(&root, &created(None, "bassel", 1));
    let done = mk_completed("alpha", at(2), "bassel", "").expect("valid");
    write(
        &root,
        &mk_sealed(Some(Hash::new(genesis).expect("a name")), done),
    );
    assert_eq!(findings(&root), Vec::<String>::new());
}

#[test]
fn every_object_finding() {
    let root = scratch("objects");
    let genesis = write(&root, &created(None, "bassel", 1));
    let tampered = name_of(b"the bytes it was named for");
    put(&root, &tampered, b"other bytes");
    let binary = b"\xff\xfe not text";
    let binary_name = name_of(binary);
    put(&root, &binary_name, binary);
    let refused = b"Sealed(prev=None, event=Nonsense())";
    let refused_name = name_of(refused);
    put(&root, &refused_name, refused);
    let unreadable = name_of(b"a directory");
    fs::create_dir_all(root.join("objects").join(format!("{unreadable}.py"))).expect("mkdir");
    fs::write(root.join("objects").join("0123abcd.tmp"), "half").expect("writes");
    fs::write(root.join("HEAD"), &genesis).expect("writes");
    fs::create_dir_all(root.join("refs")).expect("mkdir");
    fs::create_dir_all(root.join("quarantine")).expect("mkdir");
    fs::write(root.join("quarantine").join(format!("{tampered}.py")), "x").expect("writes");

    let mut expected = vec![
        (
            tampered.clone(),
            format!("object {tampered} does not hash to its filename (tampered/corrupt)"),
        ),
        (
            binary_name.clone(),
            format!("object {binary_name} failed to parse: not UTF-8"),
        ),
        (
            refused_name.clone(),
            format!(
                "object {refused_name} failed to parse: parse_literal: unknown constructor \"Nonsense\""
            ),
        ),
        (
            unreadable.clone(),
            format!("object {unreadable} could not be read"),
        ),
    ];
    expected.sort();
    let mut expected: Vec<String> = expected.into_iter().map(|(_, finding)| finding).collect();
    expected.extend([
        "objects/0123abcd.tmp is not an object: a temp an interrupted write left, or a stray \
         (SPEC §3); delete it"
            .to_owned(),
        "HEAD is left from before tips were derived (SPEC §3): nothing reads it, delete it"
            .to_owned(),
        "refs/ is left from before tips were derived (SPEC §3): nothing reads it, delete it"
            .to_owned(),
        format!(
            "quarantine/{tampered}.py failed its hash and was set aside (SPEC §3): restore the \
             object from a replica, then delete this file"
        ),
    ]);
    assert_eq!(findings(&root), expected);
}

#[test]
fn every_graph_finding() {
    let root = scratch("graph");
    let absent = name_of(b"never written");
    write(&root, &created(Some(&absent), "bassel", 1));
    let tampered = name_of(b"what it was");
    put(&root, &tampered, b"what it is");
    write(&root, &created(Some(&tampered), "bassel", 2));
    let aside = name_of(b"set aside");
    fs::create_dir_all(root.join("quarantine")).expect("mkdir");
    fs::write(root.join("quarantine").join(format!("{aside}.py")), "x").expect("writes");
    write(&root, &created(Some(&aside), "bassel", 3));

    let found = findings(&root);
    let graph: Vec<&String> = found
        .iter()
        .filter(|finding| finding.starts_with("chain broke"))
        .collect();
    let recomputed = name_of(b"what it is");
    let mut expected = [
        format!("chain broke at {absent}: missing object {absent}"),
        format!(
            "chain broke at {tampered}: object {tampered} hashes to {recomputed} — tampered or \
             corrupt: `prodrome quarantine {tampered}` (`EventStore::quarantine`) sets it aside \
             so the store answers again"
        ),
    ];
    expected.sort();
    assert_eq!(graph, expected.iter().collect::<Vec<_>>());
}

#[test]
fn the_dating_finding() {
    let root = scratch("dating");
    let genesis = write(&root, &created(None, "bassel", 5));
    let behind = write(&root, &created(Some(&genesis), "triage", 3));
    assert_eq!(
        findings(&root),
        vec![format!(
            "untrusted event {behind} is dated 2026-09-03T12:00:00, behind its predecessor \
             (2026-09-05T12:00:00)"
        )]
    );
}

#[test]
fn the_unreadable_directory_findings() {
    let root = scratch("io");
    fs::remove_dir_all(root.join("objects")).expect("rmdir");
    fs::write(root.join("objects"), "not a directory").expect("writes");
    assert_eq!(
        findings(&root),
        vec![format!(
            "{}: Not a directory (os error 20)",
            root.join("objects").display()
        )]
    );

    let root = scratch("quarantine-io");
    fs::write(root.join("quarantine"), "not a directory").expect("writes");
    assert_eq!(
        findings(&root),
        vec![format!(
            "{}: Not a directory (os error 20)",
            root.join("quarantine").display()
        )]
    );
}

fn change(genesis: &str, deps: &[&str], event: TodoEvent<Todo>) -> Envelope<Todo> {
    let hash = |name: &&str| Hash::new((*name).to_owned()).expect("a name");
    Envelope::Change(
        mk_change(hash(&genesis), deps.iter().map(hash).collect(), event).expect("a change"),
    )
}

#[test]
fn every_change_identity_finding() {
    let root = scratch("draft-a");
    let genesis = |label| Envelope::Genesis(mk_genesis(label, &"0".repeat(32)).expect("a genesis"));
    let mine = write(&root, &genesis("mine"));
    let theirs = write(&root, &genesis("theirs"));
    let done = write(
        &root,
        &change(
            &mine,
            &[],
            mk_completed("alpha", at(1), "bassel", "").expect("valid"),
        ),
    );
    let dropped = write(
        &root,
        &change(
            &mine,
            &[],
            mk_cancelled("alpha", at(2), "bassel", "").expect("valid"),
        ),
    );
    let reopened = write(
        &root,
        &change(
            &mine,
            &[&done],
            mk_reopened("alpha", at(3), "bassel", "").expect("valid"),
        ),
    );
    let absent = name_of(b"never written");

    let redundant = write(
        &root,
        &change(
            &mine,
            &[&done, &reopened],
            mk_completed("alpha", at(4), "bassel", "").expect("valid"),
        ),
    );
    let other_todo = write(
        &root,
        &change(
            &mine,
            &[&done],
            mk_completed("beta", at(4), "bassel", "").expect("valid"),
        ),
    );
    let crossing = write(
        &root,
        &change(
            &theirs,
            &[&dropped],
            mk_reopened("alpha", at(4), "bassel", "").expect("valid"),
        ),
    );
    let stranger = write(
        &root,
        &change(
            &done,
            &[],
            mk_created("gamma", at(4), "bassel", "", "").expect("valid"),
        ),
    );
    let snapshot = mk_snapshot(
        Hash::new(mine.clone()).expect("a name"),
        vec![Hash::new(absent.clone()).expect("a name")],
        None,
    )
    .expect("a snapshot");
    let incomplete = write(&root, &Envelope::Snapshot(snapshot));

    let mut by_object = vec![
        (
            redundant.clone(),
            vec![format!(
                "change {redundant} depends on {done}, which its dep {reopened} already rests \
                 on (SPEC §3)"
            )],
        ),
        (
            other_todo.clone(),
            vec![format!(
                "change {other_todo} depends on {done}, which writes no register its event \
                 writes (SPEC §3)"
            )],
        ),
        (
            crossing.clone(),
            vec![format!(
                "object {crossing} rests on {dropped}, of another genesis (SPEC §3)"
            )],
        ),
        (
            stranger.clone(),
            vec![format!(
                "object {stranger} names genesis {done}, which the store does not hold as a \
                 genesis (SPEC §3)"
            )],
        ),
        (
            incomplete.clone(),
            vec![format!(
                "snapshot {incomplete} attests {absent}, which the store does not hold \
                 (SPEC §3)"
            )],
        ),
    ];
    by_object.sort();
    let mut expected = vec![format!("chain broke at {absent}: missing object {absent}")];
    expected.extend(by_object.into_iter().flat_map(|(_, found)| found));
    assert_eq!(findings(&root), expected);
}
