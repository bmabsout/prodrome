//! A coordinated decision (design §6, §6.1): a non-monotone question asked
//! of the replica of record under its lock, and the record of the answer
//! written through, before anything is performed.

use std::fs::File;
use std::sync::{Arc, MutexGuard};

use super::memory::Memory;
use super::EventStore;
use crate::dag::Dag;
use crate::event::Hash;
use crate::literal::ProdromeError;
use crate::policy::Policy;
use crate::registers::Folded;
use crate::schema::Schema;

/// THE STORE OF RECORD, LOCKED: [`EventStore::decide`]'s answer, and the
/// only value that can answer a question no monotone reading can ("is this
/// proposal still standing, so I may send?"), because while it lives no
/// writer of the store, in this process or another, finishes a write.
///
/// What it reads is everything any writer finished; what it appends is on
/// disk, synced, when the append returns (WRITTEN THROUGH). So a host
/// decides, appends the record of its decision, and only then performs the
/// effect: a crash at any point leaves either no record and no effect, or a
/// record a later decision reads. Dropping it lets the lock go.
///
/// Only [`EventStore`] makes one. A store in memory and an overlay hold no
/// lock any other writer takes and do not write through, so a decision
/// cannot be written against either: the CALM line (§6), as a type. Monotone
/// writes need none of this, and every replica appends them.
///
/// ```
/// # use prodrome::event::{mk_completed, TodoEvent, TodoId};
/// # use prodrome::literal::Datetime;
/// # use prodrome::policy::Untrusted;
/// # use prodrome::reference::Todo;
/// # use prodrome::store::EventStore;
/// # fn main() -> Result<(), prodrome::literal::ProdromeError> {
/// # let dir = std::env::temp_dir().join("prodrome-decision-doc");
/// # let _ = std::fs::remove_dir_all(&dir);
/// let store = EventStore::<TodoEvent<Todo>>::new(&dir, Untrusted::none());
/// store.init("decisions")?;
/// let at = Datetime::new(2026, 10, 1, 9, 0, 0, 0)?;
/// let todo = TodoId::new("send-it")?;
/// let mut decision = store.decide()?;
/// let unsent = !decision
///     .folded()?
///     .entities()
///     .any(|(key, _)| *key == todo);
/// if unsent {
///     decision.append(mk_completed("send-it", at, "host", "sent")?)?;
/// }
/// drop(decision);
/// // Only now is the effect performed: its record is on disk.
/// # let _ = std::fs::remove_dir_all(&dir);
/// # Ok(())
/// # }
/// ```
///
/// ```compile_fail
/// use prodrome::event::TodoEvent;
/// use prodrome::reference::Todo;
/// use prodrome::store::{EventStore, Overlay};
///
/// type Store = EventStore<TodoEvent<Todo>>;
/// fn send(overlay: &Overlay<TodoEvent<Todo>, Store>) {
///     // An overlay writes to memory: it has no lock to decide under.
///     let decision = overlay.decide();
/// }
/// ```
///
/// ```compile_fail
/// use prodrome::event::TodoEvent;
/// use prodrome::reference::Todo;
/// use prodrome::store::MemoryStore;
///
/// fn send(replica: &MemoryStore<TodoEvent<Todo>>) {
///     // A store in memory has no lock any other writer takes.
///     let decision = replica.decide();
/// }
/// ```
///
/// While it lives, read through it and not through the store: the store's
/// own reads wait for it, on this thread too.
pub struct Decision<'s, E: Schema, Pol> {
    pub(super) store: &'s EventStore<E, Pol>,
    pub(super) memory: MutexGuard<'s, Memory<E>>,
    pub(super) _locked: Option<File>,
}

impl<E: Schema, Pol: Policy<E>> Decision<'_, E, Pol> {
    /// Every object the store holds, as of the lock.
    #[must_use]
    pub fn dag(&self) -> &Arc<Dag<E>> {
        self.memory.dag()
    }

    /// The fold of every object the store holds ([`EventStore::folded`]).
    ///
    /// # Errors
    ///
    /// A cycle among what is held, which only a hash collision could make.
    pub fn folded(&mut self) -> Result<&Arc<Folded<E>>, ProdromeError> {
        self.memory.folded()
    }

    /// [`EventStore::append`] under this lock, written through: the object
    /// is on disk when this returns.
    ///
    /// # Errors
    ///
    /// [`EventStore::append`]'s.
    pub fn append(&mut self, event: E) -> Result<Hash, ProdromeError> {
        let (name, sealed) = self.memory.change(self.store.genesis.as_ref(), event)?;
        match sealed {
            Some(sealed) => self.store.write(&mut self.memory, sealed),
            None => Ok(name),
        }
    }
}
