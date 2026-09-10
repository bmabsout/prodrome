//! What the verbs print.
//!
//! Every function here is total and returns a `String`: the binary is the only
//! thing that writes to a stream, so a test drives a verb and reads its answer.

use prodrome::event::TodoId;
use prodrome::fpl::print_term;
use prodrome::registers::Kind;
use prodrome::view::{Confidence, Entry, Provisional};

use crate::price::iso;
use crate::Reading;

/// A price, `None` where §6.4 says a todo has none: a todo with no spec and
/// no checklist has no function, and absence is not a zero.
fn priced(entry: &Entry) -> Option<String> {
    entry.value().map(|value| format!("{value:.3}"))
}

/// Why a row is not the confirmed reading, whole — empty when it is.
fn provisional(entry: &Entry) -> String {
    match entry.confidence {
        Confidence::Confirmed => String::new(),
        Confidence::Provisional(reason) => {
            let claimed = format!("claimed {}", entry.claimed());
            let content = "content unconfirmed".to_owned();
            let text = match reason {
                Provisional::Claimed => claimed,
                Provisional::Content => content,
                Provisional::ClaimedAndContent => format!("{claimed}, {content}"),
            };
            format!("  [{text}]")
        }
    }
}

/// The roster a reading was taken under, as the summary line says it.
fn under(untrusted: &[String]) -> String {
    if untrusted.is_empty() {
        "standing behind every writer".to_owned()
    } else {
        format!("untrusted: {}", untrusted.join(", "))
    }
}

/// The open todos at the reading's instant, most urgent first.
///
/// OPEN AND ALREADY CREATED. `view::entries` answers for every todo the DAG
/// has ever mentioned, the future's included, because that is what a §6.7 row
/// is; a list of what to do next is the rows whose todo existed at `t` and
/// whose outcome at `t` is nothing.
pub fn list(reading: &Reading) -> String {
    let mut rows: Vec<&Entry> = reading
        .entries
        .iter()
        .filter(|entry| entry.outcome.is_none() && reading.existed(&entry.todo))
        .collect();
    rows.sort_by(|a, b| match (a.value(), b.value()) {
        (Some(x), Some(y)) => x.total_cmp(&y).then_with(|| a.todo.cmp(&b.todo)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.todo.cmp(&b.todo),
    });

    let mut out = vec![format!(
        "{} open at {} — {}",
        rows.len(),
        iso(reading.at),
        under(&reading.untrusted)
    )];
    let width = rows
        .iter()
        .map(|entry| entry.todo.as_str().len())
        .max()
        .unwrap_or(0);
    for entry in rows {
        out.push(format!(
            "{}  {:width$}  {}{}",
            priced(entry).unwrap_or_else(|| "  —  ".to_owned()),
            entry.todo.as_str(),
            reading.body(&entry.todo),
            provisional(entry)
        ));
    }
    out.join("\n")
}

fn field(out: &mut Vec<String>, name: &str, value: impl AsRef<str>) {
    let value = value.as_ref();
    if !value.is_empty() {
        out.push(format!("{name:<11}{value}"));
    }
}

/// One todo in full, `None` when the DAG has never mentioned it.
pub fn show(reading: &Reading, todo: &TodoId) -> Option<String> {
    let entry = reading.entries.iter().find(|entry| &entry.todo == todo)?;
    let mut out = Vec::new();
    field(&mut out, "todo", entry.todo.as_str());
    field(&mut out, "read at", iso(reading.at));
    field(&mut out, "state", entry.state());
    field(&mut out, "since", entry.at());
    field(&mut out, "claimed", entry.claimed());
    field(
        &mut out,
        "confidence",
        match entry.confidence {
            Confidence::Confirmed => "confirmed".to_owned(),
            Confidence::Provisional(_) => format!("provisional{}", provisional(entry).trim_start()),
        },
    );
    field(&mut out, "created", reading.created_at(todo));
    field(&mut out, "price", priced(entry).unwrap_or_default());
    field(
        &mut out,
        "spec",
        entry.spec().map(print_term).unwrap_or_default(),
    );
    field(&mut out, "body", reading.body(todo));
    field(&mut out, "detail", reading.detail(todo));
    field(
        &mut out,
        "content",
        entry
            .content
            .as_ref()
            .map(|hash| hash.as_str())
            .unwrap_or(""),
    );
    for (kind, names) in &entry.conflicts {
        field(
            &mut out,
            &format!("conflict:{}", Kind::as_str(*kind)),
            names
                .iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    field(
        &mut out,
        "stream",
        entry
            .stream
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join("\n           "),
    );
    Some(out.join("\n"))
}
