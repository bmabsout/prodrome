//! §3 — the DAG as a value: named objects, their tips, closure, order and
//! findings, with no files. [`crate::store`] is the files around one.

mod finding;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

pub use finding::{Finding, Unread};

use crate::change::Change;
use crate::event::{parents_of, parse_envelope, Envelope, Hash};
use crate::literal::{Datetime, ProdromeError};
use crate::policy::Policy;
use crate::registers::{Genesis, Node};
use crate::schema::Schema;
use crate::sign::{Proof, Registrar};

/// Named objects of the schema `E`, and the named prints that are not objects.
#[derive(Debug, Clone, PartialEq)]
pub struct Dag<E> {
    objects: BTreeMap<Hash, Envelope<E>>,
    unread: BTreeMap<Hash, Unread>,
}

impl<E> FromIterator<(Hash, Envelope<E>)> for Dag<E> {
    fn from_iter<I: IntoIterator<Item = (Hash, Envelope<E>)>>(objects: I) -> Self {
        Dag {
            objects: objects.into_iter().collect(),
            unread: BTreeMap::new(),
        }
    }
}

/// A stored print, rehashed against its name before it is parsed at the
/// schema `schema`.
pub fn decode<E: Schema>(
    schema: &E::Vocabulary,
    name: &Hash,
    bytes: &[u8],
) -> Result<Envelope<E>, Unread> {
    let computed = Hash::of_bytes(bytes);
    if computed != *name {
        return Err(Unread::Tampered(computed));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Unread::NotText)?;
    parse_envelope(schema, text).map_err(Unread::Unparsed)
}

impl<E> Dag<E> {
    pub fn objects(&self) -> &BTreeMap<Hash, Envelope<E>> {
        &self.objects
    }

    pub fn unread(&self) -> &BTreeMap<Hash, Unread> {
        &self.unread
    }

    pub fn get(&self, name: &Hash) -> Option<&Envelope<E>> {
        self.objects.get(name)
    }

    pub(crate) fn into_objects(self) -> BTreeMap<Hash, Envelope<E>> {
        self.objects
    }

    pub(crate) fn remove(&mut self, name: &Hash) -> Option<Envelope<E>> {
        self.objects.remove(name)
    }

    /// THE HISTORY THESE OBJECTS HOLD: the largest down-set among them, every
    /// object whose ancestors are all here. Of a store closed under parents
    /// it is the store; of one missing an object (set aside, never arrived)
    /// it is the store without that object and everything resting on it,
    /// which is the history a replica holding exactly those objects has. An
    /// interior operator on sets of objects: never more than the set,
    /// idempotent and monotone, so a function of the objects alone (law 40).
    /// A cycle, which only a hash collision makes, is in no down-set.
    #[must_use]
    pub fn interior(&self) -> Dag<E>
    where
        E: Clone,
    {
        let mut interior = Dag {
            objects: BTreeMap::new(),
            unread: self.unread.clone(),
        };
        interior.grow(self.objects.clone());
        interior
    }

    /// Extend this down-set by every object of `waiting` that rests only on
    /// it and on each other, and answer the names taken and the objects that
    /// stay waiting, each resting on something neither holds. The one step
    /// [`Dag::interior`] and a store's memory share: what arrives either
    /// joins the history or waits for what it rests on, and whatever
    /// arrives later that completes it brings it in.
    pub(crate) fn grow(
        &mut self,
        mut waiting: BTreeMap<Hash, Envelope<E>>,
    ) -> (BTreeSet<Hash>, BTreeMap<Hash, Envelope<E>>) {
        waiting.retain(|name, _| !self.objects.contains_key(name));
        let mut children: BTreeMap<Hash, Vec<Hash>> = BTreeMap::new();
        let mut short: BTreeMap<Hash, usize> = BTreeMap::new();
        let mut ready: Vec<Hash> = Vec::new();
        for (name, object) in &waiting {
            let mut count = 0;
            for parent in parents_of(object) {
                if waiting.contains_key(&parent) {
                    children.entry(parent).or_default().push(name.clone());
                    count += 1;
                } else if !self.objects.contains_key(&parent) {
                    // Missing: this object waits until it arrives.
                    count += 1;
                }
            }
            if count == 0 {
                ready.push(name.clone());
            } else {
                short.insert(name.clone(), count);
            }
        }
        let mut taken = BTreeSet::new();
        while let Some(name) = ready.pop() {
            let object = waiting.remove(&name).expect("a ready object waits");
            self.objects.insert(name.clone(), object);
            for child in children.remove(&name).into_iter().flatten() {
                let count = short.get_mut(&child).expect("a child waits");
                *count -= 1;
                if *count == 0 {
                    short.remove(&child);
                    ready.push(child);
                }
            }
            taken.insert(name);
        }
        (taken, waiting)
    }

    /// The DAG, or the refusal of its first print that is not an object.
    pub fn whole(self) -> Result<Dag<E>, ProdromeError> {
        match self.unread.iter().next() {
            Some((name, why)) => Err(why.refusal(name)),
            None => Ok(self),
        }
    }

    /// Every object no object names as a parent.
    pub fn tips(&self) -> BTreeSet<Hash> {
        let graph: Vec<(&Hash, Vec<Hash>)> = self
            .objects
            .iter()
            .map(|(name, object)| (name, parents_of(object)))
            .collect();
        tips_among(
            graph
                .iter()
                .map(|(name, parents)| (*name, parents.as_slice())),
        )
    }

    /// `seeds` and everything they rest on; a name this DAG lacks is in it,
    /// and not walked past.
    pub fn closure(&self, seeds: impl IntoIterator<Item = Hash>) -> BTreeSet<Hash> {
        let mut found = BTreeSet::new();
        let mut pending: Vec<Hash> = seeds.into_iter().collect();
        while let Some(name) = pending.pop() {
            if let Some(object) = self.objects.get(&name).filter(|_| !found.contains(&name)) {
                pending.extend(parents_of(object));
            }
            found.insert(name);
        }
        found
    }

    /// Kahn's algorithm with a min-heap on the name: parents first, and
    /// incomparable objects in name order. A missing parent or a cycle is a
    /// refusal.
    pub fn linearise(&self) -> Result<Vec<Hash>, ProdromeError> {
        for (name, object) in &self.objects {
            if let Some(parent) = parents_of(object)
                .into_iter()
                .find(|parent| !self.objects.contains_key(parent))
            {
                return Err(ProdromeError::Store(format!(
                    "missing object {}, named as a parent by {}",
                    parent.as_str(),
                    name.as_str()
                )));
            }
        }
        self.order_among(&self.objects.keys().collect())
    }

    /// `among`, parents first, by Kahn's walk over them alone: a parent
    /// outside them is passed over. A cycle is a refusal.
    pub(crate) fn order_among(&self, among: &BTreeSet<&Hash>) -> Result<Vec<Hash>, ProdromeError> {
        let mut children: BTreeMap<&Hash, Vec<&Hash>> = BTreeMap::new();
        let mut waiting: BTreeMap<&Hash, usize> = BTreeMap::new();
        for (name, object) in among
            .iter()
            .filter_map(|name| self.objects.get_key_value(*name))
        {
            waiting.insert(name, 0);
            for parent in &parents_of(object) {
                let Some(known) = among.get(parent) else {
                    continue;
                };
                children.entry(known).or_default().push(name);
                *waiting.get_mut(name).expect("counted above") += 1;
            }
        }
        let mut ready: BinaryHeap<Reverse<&Hash>> = waiting
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(name, _)| Reverse(*name))
            .collect();
        let mut order: Vec<Hash> = Vec::with_capacity(waiting.len());
        while let Some(Reverse(name)) = ready.pop() {
            order.push(name.clone());
            for child in children.get(name).into_iter().flatten() {
                let count = waiting.get_mut(*child).expect("every object is counted");
                *count -= 1;
                if *count == 0 {
                    ready.push(Reverse(child));
                }
            }
        }
        if order.len() != waiting.len() {
            let placed: BTreeSet<&Hash> = order.iter().collect();
            let stuck = waiting
                .keys()
                .find(|name| !placed.contains(*name))
                .expect("a short order left something out");
            return Err(ProdromeError::Store(format!(
                "cycle in the object graph at {}",
                stuck.as_str()
            )));
        }
        Ok(order)
    }

    /// Is `name` a legacy root, a `Sealed` with no `prev`?
    pub(crate) fn is_root(&self, name: &Hash) -> bool {
        matches!(
            self.objects.get(name),
            Some(Envelope::Sealed { prev: None, .. })
        )
    }

    /// Every prodrome's genesis: each `Genesis`, and the least legacy root,
    /// whose prodrome every legacy object is in (§3).
    pub fn geneses(&self) -> BTreeSet<Hash> {
        let genesis = self
            .objects
            .iter()
            .filter(|(_, object)| matches!(object, Envelope::Genesis(_)))
            .map(|(name, _)| name.clone());
        let root = self.objects.keys().find(|name| self.is_root(name)).cloned();
        genesis.chain(root).collect()
    }

    /// The registers' key for the prodrome `genesis` starts: `None` for a
    /// legacy root.
    pub fn key(&self, genesis: &Hash) -> Genesis {
        (!self.is_root(genesis)).then(|| genesis.clone())
    }

    /// The prodrome `object` is in: a signature's is what it signs'.
    pub(crate) fn prodrome(&self, name: &Hash, object: &Envelope<E>) -> Genesis {
        match object {
            Envelope::Sealed { .. } | Envelope::Woven { .. } => None,
            Envelope::Genesis(_) => self.key(name),
            Envelope::Signed(signed) => self
                .objects
                .get_key_value(&signed.object)
                .and_then(|(name, object)| self.prodrome(name, object)),
            other => other.genesis().and_then(|genesis| self.key(genesis)),
        }
    }

    /// The genesis of the prodrome the object `name` is in (§3): a
    /// `Genesis`'s own name, the one a change or a snapshot names, and for
    /// the legacy prodrome its least root. `None` where the DAG lacks it.
    #[must_use]
    pub fn genesis_of(&self, name: &Hash) -> Option<Hash> {
        let object = self.objects.get(name)?;
        match self.prodrome(name, object) {
            Some(genesis) => Some(genesis),
            None => self.objects.keys().find(|name| self.is_root(name)).cloned(),
        }
    }

    /// The tips of one prodrome's objects.
    pub fn tips_in(&self, prodrome: &Genesis) -> BTreeSet<Hash> {
        self.tips()
            .into_iter()
            .filter(|tip| self.prodrome(tip, &self.objects[tip]) == *prodrome)
            .collect()
    }
}

impl<E: Schema> Dag<E> {
    /// Each print decoded under its name at the schema `schema`, a print
    /// that is not an object kept with why.
    pub fn from_prints(
        schema: &E::Vocabulary,
        prints: impl IntoIterator<Item = (Hash, Result<Vec<u8>, ProdromeError>)>,
    ) -> Dag<E> {
        let mut dag = Dag::from_iter([]);
        for (name, bytes) in prints {
            match bytes
                .map_err(Unread::Io)
                .and_then(|bytes| decode(schema, &name, &bytes))
            {
                Ok(object) => {
                    dag.objects.insert(name, object);
                }
                Err(why) => {
                    dag.unread.insert(name, why);
                }
            }
        }
        dag
    }

    /// The objects as the registers read them, in the linearisation's order.
    /// A DAG missing a parent refuses: its [`Dag::interior`] is the history
    /// it holds.
    pub fn nodes(&self) -> Result<Vec<Node<E>>, ProdromeError> {
        Ok(self
            .linearise()?
            .iter()
            .filter_map(|name| self.node(name))
            .collect())
    }

    /// One object as the registers read it, in this DAG's prodromes.
    pub(crate) fn node(&self, name: &Hash) -> Option<Node<E>> {
        let (name, object) = self.objects.get_key_value(name)?;
        Some(Node::placed(
            name.clone(),
            object,
            self.prodrome(name, object),
        ))
    }

    /// §3's findings: every print that is not an object, by name, then every
    /// parent no object is, or else a cycle, or else the dating rule; then
    /// §3's genesis, deps and snapshot rules, object by object.
    pub fn verify(&self, policy: &impl Policy<E>) -> Vec<Finding> {
        let mut findings: Vec<Finding> = self
            .unread
            .iter()
            .map(|(name, why)| Finding::Unread {
                name: name.clone(),
                why: why.clone(),
            })
            .collect();
        let missing: BTreeSet<Hash> = self
            .objects
            .values()
            .flat_map(parents_of)
            .filter(|parent| !self.objects.contains_key(parent))
            .collect();
        if missing.is_empty() {
            match self.linearise() {
                Ok(order) => findings.extend(self.dated(&order, policy)),
                Err(error) => findings.push(Finding::Cycle(error)),
            }
        }
        findings.extend(missing.into_iter().map(|at| Finding::Broken {
            why: self.unread.get(&at).cloned(),
            at,
        }));
        findings.extend(self.signature_findings());
        for (name, object) in &self.objects {
            findings.extend(self.genesis_findings(name, object));
            match object {
                Envelope::Change(change) => findings.extend(self.deps_findings(name, change)),
                Envelope::Snapshot(_) => findings.extend(
                    self.closure(parents_of(object))
                        .into_iter()
                        .filter(|held| !self.objects.contains_key(held))
                        .map(|missing| Finding::Incomplete {
                            snapshot: name.clone(),
                            missing,
                        }),
                ),
                _ => {}
            }
        }
        findings
    }

    /// §3 and §5: a signature that does not verify, and an object whose
    /// actor has a key and which no key of its actor's signed, keys read as
    /// they are written ([`Registrar::Anyone`]).
    fn signature_findings(&self) -> Vec<Finding> {
        let proof = Proof::of(self, &Registrar::Anyone);
        let mut findings = Vec::new();
        for (name, object) in &self.objects {
            if matches!(object, Envelope::Signed(signed) if !signed.verifies()) {
                findings.push(Finding::Forged {
                    signed: name.clone(),
                });
            }
            if let Some(event) = object.event() {
                if proof.keyed(&self.prodrome(name, object), event.actor()) && !proof.proves(name) {
                    findings.push(Finding::Unsigned {
                        object: name.clone(),
                        actor: event.actor().clone(),
                    });
                }
            }
        }
        findings
    }

    /// §3: an object names a genesis the store holds, and rests on
    /// nothing of another. The edge to a stranger is that finding alone.
    fn genesis_findings(&self, name: &Hash, object: &Envelope<E>) -> Vec<Finding> {
        let stranger = object.genesis().filter(|genesis| {
            !self.is_root(genesis)
                && !matches!(self.objects.get(*genesis), Some(Envelope::Genesis(_)))
        });
        let prodrome = self.prodrome(name, object);
        let crossings = parents_of(object).into_iter().filter(|parent| {
            Some(parent) != stranger
                && self
                    .objects
                    .get(parent)
                    .is_some_and(|held| self.prodrome(parent, held) != prodrome)
        });
        stranger
            .map(|genesis| Finding::Stranger {
                object: name.clone(),
                genesis: genesis.clone(),
            })
            .into_iter()
            .chain(crossings.map(|parent| Finding::Crossing {
                object: name.clone(),
                parent,
            }))
            .collect()
    }

    /// §3: each dep is a write to the change's entity, and no dep rests on
    /// another.
    fn deps_findings(&self, name: &Hash, change: &Change<E>) -> Vec<Finding> {
        let beneath: Vec<(&Hash, BTreeSet<Hash>)> = change
            .deps
            .iter()
            .map(|dep| {
                let parents = self.objects.get(dep).map(parents_of).unwrap_or_default();
                (dep, self.closure(parents))
            })
            .collect();
        let mut findings = Vec::new();
        for (dep, _) in &beneath {
            let held = self.objects.get(dep);
            if held.is_some_and(|held| held.event().map(Schema::key) != Some(change.event.key())) {
                findings.push(Finding::Foreign {
                    change: name.clone(),
                    dep: (*dep).clone(),
                });
            }
            for (other, below) in &beneath {
                if below.contains(*dep) {
                    findings.push(Finding::Redundant {
                        change: name.clone(),
                        dep: (*dep).clone(),
                        beneath: (*other).clone(),
                    });
                }
            }
        }
        findings
    }

    /// An event the policy does not confirm, dated before the latest stamp
    /// anywhere beneath it: a forced clock that ran backwards.
    fn dated(&self, order: &[Hash], policy: &impl Policy<E>) -> Vec<Finding> {
        let mut high: BTreeMap<&Hash, Datetime> = BTreeMap::new();
        let mut findings = Vec::new();
        for name in order {
            let (name, object) = self
                .objects
                .get_key_value(name)
                .expect("the order names objects");
            let behind = parents_of(object)
                .iter()
                .filter_map(|parent| high.get(parent).copied())
                .max();
            let event = object.event();
            if let (Some(event), Some(behind)) = (event, behind) {
                if !policy.confirms(name, event) && event.at() < behind {
                    findings.push(Finding::Dated {
                        name: name.clone(),
                        at: event.at(),
                        behind,
                    });
                }
            }
            if let Some(stamp) = behind.into_iter().chain(event.map(Schema::at)).max() {
                high.insert(name, stamp);
            }
        }
        findings
    }
}

/// The tips of a graph given as each name with the parents it names.
pub(crate) fn tips_among<'a>(
    graph: impl IntoIterator<Item = (&'a Hash, &'a [Hash])>,
) -> BTreeSet<Hash> {
    let mut names: Vec<&Hash> = Vec::new();
    let mut named: BTreeSet<&Hash> = BTreeSet::new();
    for (name, parents) in graph {
        names.push(name);
        named.extend(parents);
    }
    names
        .into_iter()
        .filter(|name| !named.contains(name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_created, mk_sealed, seal_hash, TodoEvent};
    use crate::reference::Todo;

    #[test]
    fn linearise_refuses_a_missing_parent() {
        let at = Datetime::new(2026, 9, 1, 12, 0, 0, 0).expect("a real instant");
        let event = mk_created("alpha", at, "bassel", "", "").expect("valid");
        let absent = Hash::new("a".repeat(64)).expect("hex");
        let orphaned: Envelope<TodoEvent<Todo>> = mk_sealed(Some(absent.clone()), event);
        let name = seal_hash(&orphaned);
        let dag: Dag<TodoEvent<Todo>> = [(name.clone(), orphaned)].into_iter().collect();
        assert_eq!(
            dag.linearise().unwrap_err().to_string(),
            format!(
                "missing object {}, named as a parent by {}",
                absent.as_str(),
                name.as_str()
            )
        );
    }
}
