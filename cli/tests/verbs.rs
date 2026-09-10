//! The verbs, driven end to end against a store on disk.
//!
//! Through `Cli::try_parse_from` and never through a spawned process: the
//! command line's grammar is part of what is being tested, and the answer is a
//! value the library returned rather than bytes scraped off a pipe.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{mk_created, Hash};
use prodrome::literal::Datetime;
use prodrome::policy::Untrusted;
use prodrome_cli::command::Cli;
use prodrome_cli::{run, Outcome, Store};

use clap::Parser;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn a_store() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "prodrome-cli-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// One invocation. `--store` is always named, because a test that fell back to
/// `./roadmap` would be reading the repository's own store.
fn prodrome(root: &Path, args: &[&str]) -> Result<Outcome, prodrome_cli::Error> {
    let mut argv: Vec<String> = vec!["prodrome".to_owned()];
    argv.extend(args.iter().map(|arg| (*arg).to_owned()));
    argv.push("--store".to_owned());
    argv.push(root.display().to_string());
    run(&Cli::try_parse_from(argv).expect("the arguments parse"))
}

fn said(root: &Path, args: &[&str]) -> String {
    prodrome(root, args).expect("the verb answers").text
}

/// A seeded store: two todos, one of them completed, all of it backdated so
/// the time-machine assertions below have a past to ask about.
fn seeded() -> PathBuf {
    let root = a_store();
    said(&root, &["init", &root.display().to_string()]);
    said(
        &root,
        &[
            "add",
            "publish",
            "--body",
            "publish the crate",
            "--priority",
            "70",
            "--actor",
            "bassel",
            "--at",
            "2026-09-08T09:00:00",
        ],
    );
    said(
        &root,
        &[
            "add",
            "ship-the-cli",
            "--body",
            "a command line over a store",
            "--detail",
            "the verbs, and a roadmap seeded with them",
            "--priority",
            "35",
            "--actor",
            "bassel",
            "--at",
            "2026-09-09T09:00:00",
        ],
    );
    said(
        &root,
        &[
            "done",
            "ship-the-cli",
            "--actor",
            "bassel",
            "--at",
            "2026-09-10T09:00:00",
        ],
    );
    root
}

#[test]
fn init_makes_a_store_once() {
    let root = a_store();
    let dir = root.display().to_string();
    assert!(said(&root, &["init", &dir]).contains("initialised"));
    assert!(root.join("objects").is_dir());
    assert!(
        prodrome(&root, &["init", &dir]).is_err(),
        "twice is a refusal"
    );
}

#[test]
fn list_is_ascending_in_fulfillment() {
    let root = seeded();
    let text = said(&root, &["list", "--at", "2026-09-09T12:00:00"]);
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("2 open at 2026-09-09T12:00:00"),
        "{text}"
    );
    assert!(lines[1].starts_with("0.350  ship-the-cli"), "{text}");
    assert!(lines[2].starts_with("0.700  publish"), "{text}");
}

#[test]
fn a_todo_is_absent_before_it_was_created() {
    let root = seeded();
    let text = said(&root, &["list", "--at", "2026-09-08T12:00:00"]);
    assert!(text.starts_with("1 open"), "{text}");
    assert!(text.contains("publish"), "{text}");
    assert!(!text.contains("ship-the-cli"), "{text}");
}

#[test]
fn a_completed_todo_leaves_the_list() {
    let root = seeded();
    let text = said(&root, &["list", "--at", "2026-09-10T12:00:00"]);
    assert!(text.starts_with("1 open"), "{text}");
    assert!(!text.contains("ship-the-cli"), "{text}");
}

#[test]
fn reopen_brings_it_back() {
    let root = seeded();
    said(
        &root,
        &[
            "reopen",
            "ship-the-cli",
            "--actor",
            "bassel",
            "--at",
            "2026-09-10T10:00:00",
        ],
    );
    let text = said(&root, &["list", "--at", "2026-09-10T12:00:00"]);
    assert!(text.starts_with("2 open"), "{text}");
}

#[test]
fn cancel_closes_a_todo_too() {
    let root = seeded();
    said(
        &root,
        &[
            "cancel",
            "publish",
            "--actor",
            "bassel",
            "--at",
            "2026-09-10T10:00:00",
        ],
    );
    let text = said(&root, &["show", "publish", "--at", "2026-09-10T12:00:00"]);
    assert!(text.contains("state      cancelled"), "{text}");
}

#[test]
fn revise_reprices_from_its_instant_on() {
    let root = seeded();
    said(
        &root,
        &[
            "revise",
            "publish",
            "--priority",
            "20",
            "--actor",
            "bassel",
            "--at",
            "2026-09-10T09:00:00",
        ],
    );
    let before = said(&root, &["list", "--at", "2026-09-10T08:00:00"]);
    let after = said(&root, &["list", "--at", "2026-09-10T12:00:00"]);
    assert!(before.contains("0.700  publish"), "{before}");
    assert!(after.contains("0.200  publish"), "{after}");
}

#[test]
fn revise_without_a_price_is_a_refusal() {
    let root = seeded();
    assert!(prodrome(&root, &["revise", "publish", "--actor", "bassel"]).is_err());
}

#[test]
fn a_verb_refuses_an_id_the_store_does_not_hold() {
    let root = seeded();
    assert!(prodrome(&root, &["done", "nope", "--actor", "bassel"]).is_err());
    assert!(prodrome(&root, &["show", "nope"]).is_err());
    assert!(prodrome(
        &root,
        &["add", "publish", "--body", "again", "--actor", "bassel"]
    )
    .is_err());
}

#[test]
fn show_carries_the_record_and_the_stream() {
    let root = seeded();
    let text = said(
        &root,
        &["show", "ship-the-cli", "--at", "2026-09-10T12:00:00"],
    );
    assert!(text.contains("state      completed"), "{text}");
    assert!(text.contains("created    2026-09-09T09:00:00"), "{text}");
    assert!(text.contains("detail     the verbs"), "{text}");
    assert!(text.contains("confidence confirmed"), "{text}");
    // Created, SpecRevised, the record, and the completion.
    assert_eq!(text.matches("stream").count(), 1, "{text}");
}

#[test]
fn an_untrusted_writer_claims_and_the_row_says_so() {
    let root = seeded();
    let text = said(
        &root,
        &[
            "list",
            "--at",
            "2026-09-10T12:00:00",
            "--untrusted",
            "bassel",
        ],
    );
    assert!(text.contains("untrusted: bassel"), "{text}");
    // The completion only CLAIMS, so the confirmed reading still has it open,
    // and the row carries the claim rather than hiding it.
    assert!(text.contains("ship-the-cli"), "{text}");
    assert!(
        text.contains("[claimed completed, content unconfirmed]"),
        "{text}"
    );
}

#[test]
fn verify_passes_a_store_the_verbs_wrote() {
    let root = seeded();
    let outcome = prodrome(&root, &["verify"]).expect("verify answers");
    assert!(outcome.ok, "{}", outcome.text);
    assert!(outcome.text.starts_with("ok: "), "{}", outcome.text);
}

#[test]
fn verify_fails_a_tampered_object() {
    let root = seeded();
    let objects = std::fs::read_dir(root.join("objects")).expect("the store has objects");
    let victim = objects
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .next()
        .expect("at least one object");
    std::fs::write(&victim, "Sealed(prev=None, event=None)").expect("tampers");
    let outcome = prodrome(&root, &["verify"]).expect("verify answers with findings");
    assert!(!outcome.ok, "{}", outcome.text);
    assert!(outcome.text.contains("does not hash"), "{}", outcome.text);
}

#[test]
fn weave_settles_two_heads_and_is_a_no_op_on_one() {
    let root = seeded();
    assert!(said(&root, &["weave"]).contains("nothing to weave"));

    // A second branch: an append onto something that is not the tip, which is
    // what two replicas that both wrote look like once they have exchanged
    // objects.
    let store = Store::new(&root, Untrusted::none());
    let dag = store.read_dag_named().expect("the store reads");
    let elder: Hash = dag[0].0.clone();
    let at = Datetime::new(2026, 9, 10, 11, 0, 0, 0).expect("an instant");
    store
        .append(
            mk_created("a-second-branch", at, "bassel", "written elsewhere", "").expect("an event"),
            Some(&[elder]),
        )
        .expect("appends onto an elder object");
    assert_eq!(store.tips().len(), 2, "the store has forked");

    let merge = said(&root, &["weave"]);
    assert_eq!(merge.len(), 64, "a weave answers with the object it wrote");
    assert_eq!(store.tips().len(), 1, "and the store is one history again");
    assert!(store.verify().is_empty(), "{:?}", store.verify());

    let text = said(&root, &["list", "--at", "2026-09-10T12:00:00"]);
    assert!(text.contains("a-second-branch"), "{text}");
}
