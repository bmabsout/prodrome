use std::fmt;

use crate::event::Hash;
use crate::literal::{Datetime, ProdromeError};

/// One thing `verify` says about a store, in the words §3 and Draft A use.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// The listing of `objects/` or `quarantine/` failed.
    Io(ProdromeError),
    Unread {
        name: Hash,
        why: Unread,
    },
    /// An entry of `objects/` not named like an object.
    Garbage(String),
    /// `HEAD` or `refs/`, from before the tips were derived.
    Leftover(&'static str),
    /// A file in `quarantine/`.
    Quarantined(String),
    /// A parent no readable object is: absent, or unread for `why`.
    Broken {
        at: Hash,
        why: Option<Unread>,
    },
    Cycle(ProdromeError),
    Dated {
        name: Hash,
        at: Datetime,
        behind: Datetime,
    },
}

/// Why a named print is not an object.
#[derive(Debug, Clone, PartialEq)]
pub enum Unread {
    Io(ProdromeError),
    /// The bytes hash to this instead.
    Tampered(Hash),
    NotText,
    Unparsed(ProdromeError),
}

impl Unread {
    /// What a read of the object refuses with; a tampered file's names the way out.
    pub fn refusal(&self, name: &Hash) -> ProdromeError {
        let name = name.as_str();
        match self {
            Unread::Io(error) | Unread::Unparsed(error) => error.clone(),
            Unread::Tampered(computed) => ProdromeError::Store(format!(
                "object {name} hashes to {} — tampered or corrupt: `prodrome quarantine {name}` \
                 (`EventStore::quarantine`) sets it aside so the store answers again",
                computed.as_str()
            )),
            Unread::NotText => ProdromeError::Store(format!("object {name} is not UTF-8")),
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finding::Io(error) | Finding::Cycle(error) => write!(f, "{error}"),
            Finding::Unread { name, why } => {
                let name = name.as_str();
                match why {
                    Unread::Io(_) => write!(f, "object {name} could not be read"),
                    Unread::Tampered(_) => write!(
                        f,
                        "object {name} does not hash to its filename (tampered/corrupt)"
                    ),
                    Unread::NotText => write!(f, "object {name} failed to parse: not UTF-8"),
                    Unread::Unparsed(error) => write!(f, "object {name} failed to parse: {error}"),
                }
            }
            Finding::Garbage(file) => write!(
                f,
                "objects/{file} is not an object: a temp an interrupted write left, or a stray \
                 (SPEC §3); delete it"
            ),
            Finding::Leftover(file) => write!(
                f,
                "{file} is left from before tips were derived (SPEC §3): nothing reads it, delete it"
            ),
            Finding::Quarantined(file) => write!(
                f,
                "quarantine/{file} failed its hash and was set aside (SPEC §3): restore the \
                 object from a replica, then delete this file"
            ),
            Finding::Broken { at, why } => {
                let refusal = match why {
                    Some(why) => why.refusal(at),
                    None => ProdromeError::Store(format!("missing object {}", at.as_str())),
                };
                write!(f, "chain broke at {}: {refusal}", at.as_str())
            }
            Finding::Dated { name, at, behind } => write!(
                f,
                "untrusted event {} is dated {}, behind its predecessor ({})",
                name.as_str(),
                at.isoformat(),
                behind.isoformat()
            ),
        }
    }
}
