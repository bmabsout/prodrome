//! `prodrome-github`, driven with the fixture payloads in
//! `tests/fixtures/github/` and with generated deliveries for its two laws.
//!
//! The fixtures are trimmed copies of what GitHub sends: every field the
//! mirror reads, in the shape the webhook documentation gives it, and a few it
//! does not read, so a test also shows what is ignored.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use clap::Parser;
use proptest::prelude::*;

use prodrome::event::Actor;
use prodrome::policy::Untrusted;
use prodrome_cli::command::Cli;
use prodrome_cli::{read, run, Outcome, Store};
use prodrome_github::{
    apply_delivery, instant_of, Action, Comment, Delivery, Issue, Standing, BOT, MIRROR,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn a_store() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "prodrome-github-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("objects")).expect("a store");
    root
}

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/github")
        .join(name)
        .display()
        .to_string()
}

/// `prodrome …`, or `prodrome-github …` when the first word is `github`.
fn prodrome(root: &Path, args: &[&str]) -> Result<Outcome, prodrome_cli::Error> {
    let store = ["--store".to_owned(), root.display().to_string()];
    let words = |program: &str, rest: &[&str]| -> Vec<String> {
        std::iter::once(program.to_owned())
            .chain(rest.iter().map(|arg| (*arg).to_owned()))
            .chain(store.iter().cloned())
            .collect()
    };
    match args.split_first() {
        Some((&"github", rest)) => prodrome_github::run(
            &prodrome_github::Cli::try_parse_from(words("prodrome-github", rest))
                .expect("the arguments parse"),
        ),
        _ => run(&Cli::try_parse_from(words("prodrome", args)).expect("the arguments parse")),
    }
}

fn said(root: &Path, args: &[&str]) -> String {
    prodrome(root, args).expect("the verb answers").text
}

/// Apply one fixture, and say what the command said.
fn github(root: &Path, name: &str) -> String {
    said(root, &["github", &fixture(name)])
}

fn objects(root: &Path) -> BTreeSet<String> {
    std::fs::read_dir(root.join("objects"))
        .expect("objects/")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn row<'a>(list: &'a str, todo: &str) -> Option<&'a str> {
    list.lines()
        .find(|line| line.split_whitespace().nth(1) == Some(todo))
}

const LATER: &str = "2026-09-30T00:00:00";

// --- the fixtures -------------------------------------------------------------

#[test]
fn an_opened_issue_is_an_item_priced_neutral() {
    let root = a_store();
    let written = github(&root, "issues-opened.json");
    assert_eq!(
        written.lines().count(),
        2,
        "a Created and a record: {written}"
    );
    let list = said(&root, &["list", "--at", LATER]);
    let row = row(&list, "gh-12").expect("gh-12 is listed");
    assert!(row.starts_with("0.800"), "{row}");
    assert!(row.contains("verify is slow on large stores"), "{row}");

    let objects: Vec<String> = objects(&root)
        .iter()
        .map(|name| std::fs::read_to_string(root.join("objects").join(name)).expect("an object"))
        .collect();
    let record = objects
        .iter()
        .find(|text| text.contains("Authored("))
        .expect("a record");
    assert!(record.contains(&format!("actor='{MIRROR}'")), "{record}");
    assert!(record.contains("category='issue'"), "{record}");
    assert!(
        record.contains("https://github.com/bmabsout/prodrome/issues/12"),
        "the source links back: {record}"
    );
    assert!(record.contains("spec=Flat(value=0.8)"), "{record}");
}

#[test]
fn a_replayed_delivery_appends_nothing() {
    let root = a_store();
    github(&root, "issues-opened.json");
    let before = objects(&root);
    assert_eq!(
        github(&root, "issues-opened.json"),
        "nothing to append: already applied"
    );
    assert_eq!(objects(&root), before);
}

#[test]
fn a_retitle_is_a_content_revision_and_a_body_edit_is_nothing() {
    let root = a_store();
    github(&root, "issues-opened.json");
    assert_eq!(github(&root, "issues-edited-title.json").lines().count(), 1);
    let shown = said(&root, &["show", "gh-12", "--at", LATER]);
    assert!(
        shown.contains("verify is quadratic on large stores"),
        "{shown}"
    );
    assert!(
        shown.contains("0.800"),
        "a retitle keeps the price: {shown}"
    );
    assert!(github(&root, "issues-edited-body.json").starts_with("nothing to append"));
}

#[test]
fn close_completes_and_reopen_reopens() {
    let root = a_store();
    github(&root, "issues-opened.json");
    github(&root, "issues-closed.json");
    let closed = said(&root, &["list", "--at", "2026-09-22T12:00:00"]);
    assert!(row(&closed, "gh-12").is_none(), "{closed}");
    github(&root, "issues-reopened.json");
    let open = said(&root, &["list", "--at", LATER]);
    assert!(row(&open, "gh-12").is_some(), "{open}");
}

#[test]
fn an_uninteresting_action_appends_nothing_to_a_known_item() {
    let root = a_store();
    github(&root, "issues-opened.json");
    assert!(github(&root, "issues-labeled.json").starts_with("nothing to append: labeled"));
}

#[test]
fn a_maintainer_prices_flat_with_a_deadline_or_by_reference() {
    let root = a_store();
    github(&root, "issues-opened.json");
    github(&root, "issues-opened-7.json");

    github(&root, "comment-price-flat.json");
    let list = said(&root, &["list", "--at", "2026-09-21T09:30:00"]);
    assert!(
        row(&list, "gh-12").expect("listed").starts_with("0.400"),
        "{list}"
    );
    let shown = said(&root, &["show", "gh-12", "--at", LATER]);
    assert!(shown.contains("Flat(value=0.4)"), "{shown}");

    github(&root, "comment-price-deadline.json");
    let shown = said(&root, &["show", "gh-12", "--at", LATER]);
    assert!(shown.contains("Decay(start=0.3"), "{shown}");
    assert!(
        shown.contains("datetime(2026, 10, 15, 17, 0, 0)"),
        "{shown}"
    );

    github(&root, "comment-price-ref.json");
    let list = said(&root, &["list", "--at", LATER]);
    let twelve = row(&list, "gh-12")
        .expect("gh-12")
        .split_whitespace()
        .next();
    let seven = row(&list, "gh-7").expect("gh-7").split_whitespace().next();
    assert_eq!(twelve, seven, "a reference is exactly as urgent: {list}");
}

#[test]
fn the_commenter_is_the_actor_of_a_price() {
    let root = a_store();
    github(&root, "issues-opened.json");
    github(&root, "comment-price-flat.json");
    let texts: Vec<String> = objects(&root)
        .iter()
        .map(|name| std::fs::read_to_string(root.join("objects").join(name)).expect("an object"))
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text.contains("SpecRevised(") && text.contains("actor='bmabsout'")),
        "{texts:?}"
    );
}

#[test]
fn an_outsiders_price_is_ignored_before_it_is_parsed() {
    let root = a_store();
    github(&root, "issues-opened.json");
    let before = objects(&root);
    assert!(
        github(&root, "comment-price-outsider.json").contains("not a maintainer"),
        "a malformed /price from a stranger is not answered"
    );
    assert!(github(&root, "comment-price-contributor.json").contains("not a maintainer"));
    assert_eq!(objects(&root), before);
}

#[test]
fn a_maintainers_malformed_price_is_a_refusal() {
    let root = a_store();
    github(&root, "issues-opened.json");
    let refusal = prodrome(&root, &["github", &fixture("comment-price-malformed.json")])
        .expect_err("refused");
    assert!(refusal.to_string().contains("/price"), "{refusal}");
}

#[test]
fn a_ref_to_an_unknown_item_is_a_refusal() {
    let root = a_store();
    github(&root, "issues-opened.json");
    assert!(prodrome(&root, &["github", &fixture("comment-price-ref.json")]).is_err());
}

#[test]
fn comments_that_are_not_prices_and_pull_requests_append_nothing() {
    let root = a_store();
    github(&root, "issues-opened.json");
    let before = objects(&root);
    assert!(github(&root, "comment-plain.json").contains("not a /price"));
    assert!(github(&root, "comment-on-pull-request.json").contains("pull request"));
    assert_eq!(objects(&root), before);
}

#[test]
fn an_issue_from_before_the_mirror_is_created_by_its_first_delivery() {
    let root = a_store();
    assert_eq!(
        github(&root, "comment-unmirrored-issue.json")
            .lines()
            .count(),
        3
    );
    let list = said(&root, &["list", "--at", LATER]);
    let row = row(&list, "gh-3").expect("gh-3 is listed");
    assert!(row.starts_with("0.600"), "{row}");
    assert!(row.contains("document the literal grammar"), "{row}");
}

#[test]
fn a_proposal_is_a_claim_until_a_maintainer_accepts_it() {
    let root = a_store();
    github(&root, "issues-opened.json");
    let proposal = fixture("proposal.md");
    let opened = fixture("issues-opened.json");
    assert_eq!(
        said(&root, &["github", &opened, "--proposal", &proposal])
            .lines()
            .count(),
        1
    );
    assert_eq!(
        said(&root, &["github", &opened, "--proposal", &proposal]),
        "nothing to append: already applied"
    );

    // Under a reader who names the bot, the proposal binds nothing ...
    let guarded = said(&root, &["list", "--at", LATER, "--untrusted", BOT]);
    assert!(
        row(&guarded, "gh-12").expect("listed").starts_with("0.800"),
        "{guarded}"
    );
    // ... and under one who names nobody it is what the item reads.
    let credulous = said(&root, &["list", "--at", LATER]);
    assert!(
        row(&credulous, "gh-12")
            .expect("listed")
            .starts_with("0.500"),
        "{credulous}"
    );

    github(&root, "comment-price-accept.json");
    let accepted = said(&root, &["list", "--at", LATER, "--untrusted", BOT]);
    assert!(
        row(&accepted, "gh-12")
            .expect("listed")
            .starts_with("0.500"),
        "{accepted}"
    );
    let shown = said(&root, &["show", "gh-12", "--at", LATER, "--untrusted", BOT]);
    assert!(shown.contains("confirmed"), "{shown}");
}

#[test]
fn a_proposal_is_never_dated_behind_what_it_is_written_on() {
    let root = a_store();
    github(&root, "issues-opened.json");
    // A later delivery lands while the model is thinking ...
    github(&root, "comment-unmirrored-issue.json");
    // ... and the claim about the earlier issue is recorded after it.
    said(
        &root,
        &[
            "github",
            &fixture("issues-opened.json"),
            "--proposal",
            &fixture("proposal.md"),
        ],
    );
    let verdict = prodrome(&root, &["verify", "--untrusted", BOT]).expect("verify answers");
    assert!(verdict.ok, "{}", verdict.text);
    assert_eq!(
        said(
            &root,
            &[
                "github",
                &fixture("issues-opened.json"),
                "--proposal",
                &fixture("proposal.md"),
            ],
        ),
        "nothing to append: already applied"
    );
}

#[test]
fn accepting_with_no_proposal_is_a_refusal() {
    let root = a_store();
    github(&root, "issues-opened.json");
    assert!(prodrome(&root, &["github", &fixture("comment-price-accept.json")]).is_err());
}

#[test]
fn a_proposal_cannot_accept_itself() {
    let root = a_store();
    github(&root, "issues-opened.json");
    let refusal = prodrome(
        &root,
        &[
            "github",
            &fixture("issues-opened.json"),
            "--proposal",
            &fixture("proposal-accept.md"),
        ],
    )
    .expect_err("refused");
    assert!(refusal.to_string().contains("maintainer"), "{refusal}");
}

// --- the laws -----------------------------------------------------------------

fn at(minutes: u32) -> prodrome::literal::Datetime {
    let (day, rest) = (20 + minutes / (24 * 60), minutes % (24 * 60));
    instant_of(&format!(
        "2026-09-{day:02}T{:02}:{:02}:00Z",
        rest / 60,
        rest % 60
    ))
    .expect("an instant")
}

fn issue(number: u64, title: &str, updated: u32) -> Issue {
    Issue {
        number,
        title: title.to_owned(),
        url: format!("https://github.com/bmabsout/prodrome/issues/{number}"),
        author: "octocat".to_owned(),
        created_at: at(0),
        updated_at: at(updated),
        closed_at: None,
    }
}

/// One generated delivery: an action on one of three issues, at a minute of
/// a week.
fn a_delivery() -> impl Strategy<Value = (Delivery, Option<String>)> {
    let titles = prop::sample::select(vec!["one", "two", "three"]);
    let comments = prop::sample::select(vec![
        "/price 40",
        "/price 30 --deadline 2026-10-15",
        "/price ref gh-1",
        "/price accept",
        "/price nonsense",
        "thanks!",
    ]);
    (
        1u64..4,
        titles,
        1u32..(7 * 24 * 60),
        0usize..8,
        comments,
        any::<bool>(),
    )
        .prop_map(|(number, title, minute, action, comment, maintainer)| {
            let mut issue = issue(number, title, minute);
            let mut proposal = None;
            let action = match action {
                0 => Action::Opened,
                1 => Action::Edited {
                    title_changed: true,
                },
                2 => Action::Edited {
                    title_changed: false,
                },
                3 => {
                    issue.closed_at = Some(at(minute));
                    Action::Closed
                }
                4 => Action::Reopened,
                5 => {
                    proposal = Some("/price 55\nRationale: generated".to_owned());
                    Action::Edited {
                        title_changed: false,
                    }
                }
                6 => Action::Other("labeled".to_owned()),
                _ => Action::Commented(Comment {
                    login: "Maintainer".to_owned(),
                    standing: if maintainer {
                        Standing::Maintainer
                    } else {
                        Standing::Outsider
                    },
                    body: comment.to_owned(),
                    at: at(minute),
                }),
            };
            (
                Delivery {
                    issue,
                    action,
                    pull_request: false,
                },
                proposal,
            )
        })
}

/// Apply every delivery in order, and keep the ones the store accepted: a
/// refusal writes nothing and is not a delivery that was applied.
fn replay(
    store: &Store,
    deliveries: &[(Delivery, Option<String>)],
) -> Vec<(Delivery, Option<String>)> {
    deliveries
        .iter()
        .filter(|(delivery, proposal)| apply_delivery(store, delivery, proposal.as_deref()).is_ok())
        .cloned()
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Replaying what was applied changes nothing: applying the deliveries a
    /// store accepted once more appends nothing, and a fresh store given them
    /// twice holds exactly the objects of one given them once.
    ///
    /// Over the ACCEPTED deliveries because two refusals read the store — a
    /// `/price ref` to an item not yet mirrored, a `/price accept` with no
    /// proposal yet — and are applied if they are delivered again later, when
    /// what they name exists.
    #[test]
    fn replaying_twice_is_replaying_once(
        deliveries in prop::collection::vec(a_delivery(), 0..24),
    ) {
        let first = a_store();
        let store = Store::new(&first, Untrusted::none());
        let applied = replay(&store, &deliveries);
        let once = objects(&first);
        replay(&store, &applied);
        prop_assert_eq!(&objects(&first), &once);
        prop_assert!(store.verify().is_empty(), "{:?}", store.verify());
        // The claims keep §3's clock rule for a reader who names the bot.
        let guarded = Store::new(&first, Untrusted::of(vec![Actor::new(BOT).expect("an actor")]));
        prop_assert!(guarded.verify().is_empty(), "{:?}", guarded.verify());

        let fresh = a_store();
        let again = Store::new(&fresh, Untrusted::none());
        replay(&again, &applied);
        replay(&again, &applied);
        prop_assert_eq!(&objects(&fresh), &once);
    }

    /// Closing an issue and reopening it folds to open, from the reopening
    /// on, whatever history came before; closing alone folds to closed. The
    /// close is dated after that history, as a fresh delivery is.
    #[test]
    fn close_then_reopen_folds_to_open(
        before in prop::collection::vec(a_delivery(), 0..12),
        closed in (7 * 24 * 60)..(8 * 24 * 60u32),
        gap in 0u32..(24 * 60),
    ) {
        let root = a_store();
        let store = Store::new(&root, Untrusted::none());
        replay(&store, &before);

        let mut closing = issue(1, "one", closed);
        closing.closed_at = Some(at(closed));
        let close = Delivery { issue: closing, action: Action::Closed, pull_request: false };
        let reopen = Delivery {
            issue: issue(1, "one", closed + gap),
            action: Action::Reopened,
            pull_request: false,
        };
        let open = |store: &Store, minute| {
            let reading = read(store, at(minute)).expect("a reading");
            reading
                .entries
                .iter()
                .find(|entry| entry.todo.as_str() == "gh-1")
                .map(|entry| entry.outcome.is_none())
        };

        apply_delivery(&store, &close, None).expect("close applies");
        prop_assert_eq!(open(&store, closed + gap), Some(false));
        apply_delivery(&store, &reopen, None).expect("reopen applies");
        prop_assert_eq!(open(&store, closed + gap), Some(true));
    }
}
