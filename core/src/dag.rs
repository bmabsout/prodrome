//! §3 — the DAG as a value: named objects, their tips, closure, order and
//! findings, with no files. [`crate::store`] is the files around one.

mod finding;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

pub use finding::{Finding, Unread};

use crate::change::Change;
use crate::event::{parents_of, parse_envelope, Envelope, Hash, TodoEvent, TodoId};
use crate::fold::{Kind, Write};
use crate::literal::{Datetime, ProdromeError};
use crate::payload::Payload;
use crate::policy::Policy;
use crate::registers::{Genesis, Node};

/// Named objects, and the named prints that are not objects.
#[derive(Debug, Clone, PartialEq)]
pub struct Dag<P> {
    objects: BTreeMap<Hash, Envelope<P>>,
    unread: BTreeMap<Hash, Unread>,
}

impl<P> FromIterator<(Hash, Envelope<P>)> for Dag<P> {
    fn from_iter<I: IntoIterator<Item = (Hash, Envelope<P>)>>(objects: I) -> Self {
        Dag {
            objects: objects.into_iter().collect(),
            unread: BTreeMap::new(),
        }
    }
}

/// A stored print, rehashed against its name before it is parsed.
pub fn decode<P: Payload>(name: &Hash, bytes: &[u8]) -> Result<Envelope<P>, Unread> {
    let computed = Hash::of_bytes(bytes);
    if computed != *name {
        return Err(Unread::Tampered(computed));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Unread::NotText)?;
    parse_envelope(text).map_err(Unread::Unparsed)
}

impl<P> Dag<P> {
    pub fn objects(&self) -> &BTreeMap<Hash, Envelope<P>> {
        &self.objects
    }

    pub fn unread(&self) -> &BTreeMap<Hash, Unread> {
        &self.unread
    }

    pub fn get(&self, name: &Hash) -> Option<&Envelope<P>> {
        self.objects.get(name)
    }

    /// The DAG, or the refusal of its first print that is not an object.
    pub fn whole(self) -> Result<Dag<P>, ProdromeError> {
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
        self.order(false)
    }

    /// [`Dag::linearise`], passing over a parent this DAG lacks when `gaps`.
    fn order(&self, gaps: bool) -> Result<Vec<Hash>, ProdromeError> {
        let mut children: BTreeMap<&Hash, Vec<&Hash>> = BTreeMap::new();
        let mut waiting: BTreeMap<&Hash, usize> = BTreeMap::new();
        for (name, object) in &self.objects {
            waiting.insert(name, 0);
            for parent in &parents_of(object) {
                let Some((known, _)) = self.objects.get_key_value(parent) else {
                    if gaps {
                        continue;
                    }
                    return Err(ProdromeError::Store(format!(
                        "missing object {}, named as a parent by {}",
                        parent.as_str(),
                        name.as_str()
                    )));
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
        let mut order: Vec<Hash> = Vec::with_capacity(self.objects.len());
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
        if order.len() != self.objects.len() {
            let placed: BTreeSet<&Hash> = order.iter().collect();
            let stuck = self
                .objects
                .keys()
                .find(|name| !placed.contains(name))
                .expect("a short order left something out");
            return Err(ProdromeError::Store(format!(
                "cycle in the object graph at {}",
                stuck.as_str()
            )));
        }
        Ok(order)
    }

    fn is_root(&self, name: &Hash) -> bool {
        matches!(
            self.objects.get(name),
            Some(Envelope::Sealed { prev: None, .. })
        )
    }

    /// Every prodrome's genesis: each `Genesis`, and the least legacy root,
    /// whose prodrome every legacy object is in (Draft A, A2).
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

    fn prodrome(&self, name: &Hash, object: &Envelope<P>) -> Genesis {
        match object {
            Envelope::Sealed { .. } | Envelope::Woven { .. } => None,
            Envelope::Genesis(_) => self.key(name),
            other => other.genesis().and_then(|genesis| self.key(genesis)),
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

impl<P: Payload> Dag<P> {
    /// Each print decoded under its name, a print that is not an object kept
    /// with why.
    pub fn from_prints(
        prints: impl IntoIterator<Item = (Hash, Result<Vec<u8>, ProdromeError>)>,
    ) -> Dag<P> {
        let mut dag = Dag::from_iter([]);
        for (name, bytes) in prints {
            match bytes
                .map_err(Unread::Io)
                .and_then(|bytes| decode(&name, &bytes))
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
    pub fn nodes(&self) -> Result<Vec<Node<P>>, ProdromeError> {
        self.nodes_in(self.linearise()?)
    }

    /// [`Dag::nodes`] over what is held when a parent is not: a writer's
    /// read of a store with an object set aside.
    pub fn nodes_across_gaps(&self) -> Result<Vec<Node<P>>, ProdromeError> {
        self.nodes_in(self.order(true)?)
    }

    fn nodes_in(&self, order: Vec<Hash>) -> Result<Vec<Node<P>>, ProdromeError> {
        Ok(order
            .into_iter()
            .map(|name| {
                let object = &self.objects[&name];
                let genesis = self.prodrome(&name, object);
                Node {
                    genesis,
                    ..Node::of(name, object)
                }
            })
            .collect())
    }

    /// §3's findings: every print that is not an object, by name, then every
    /// parent no object is, or else a cycle, or else the dating rule; then
    /// Draft A's, object by object.
    pub fn verify(&self, policy: &impl Policy<P>) -> Vec<Finding> {
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

    /// Draft A: an object names a genesis the store holds, and rests on
    /// nothing of another.
    fn genesis_findings(&self, name: &Hash, object: &Envelope<P>) -> Vec<Finding> {
        let stranger = object.genesis().filter(|genesis| {
            !self.is_root(genesis)
                && !matches!(self.objects.get(*genesis), Some(Envelope::Genesis(_)))
        });
        let prodrome = self.prodrome(name, object);
        let crossings = parents_of(object).into_iter().filter(|parent| {
            self.objects
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

    /// Draft A: each dep writes a register the change's event writes, and no
    /// dep rests on another.
    fn deps_findings(&self, name: &Hash, change: &Change<P>) -> Vec<Finding> {
        let written = registers(Some(&change.event));
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
            if held.is_some_and(|held| registers(held.event()).is_disjoint(&written)) {
                findings.push(Finding::Unwritten {
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
    fn dated(&self, order: &[Hash], policy: &impl Policy<P>) -> Vec<Finding> {
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
                if !policy.confirms(event) && event.at() < behind {
                    findings.push(Finding::Dated {
                        name: name.clone(),
                        at: event.at(),
                        behind,
                    });
                }
            }
            if let Some(stamp) = behind.into_iter().chain(event.map(TodoEvent::at)).max() {
                high.insert(name, stamp);
            }
        }
        findings
    }
}

/// The registers an event writes.
fn registers<P: Payload>(event: Option<&TodoEvent<P>>) -> BTreeSet<(&TodoId, Kind)> {
    event
        .into_iter()
        .flat_map(|event| Write::of(event).filter_map(|write| Some((event.todo(), write.kind()?))))
        .collect()
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
    use crate::event::{mk_created, mk_sealed, seal_hash};
    use crate::reference::Todo;

    #[test]
    fn linearise_refuses_a_missing_parent() {
        let at = Datetime::new(2026, 9, 1, 12, 0, 0, 0).expect("a real instant");
        let event = mk_created("alpha", at, "bassel", "", "").expect("valid");
        let absent = Hash::new("a".repeat(64)).expect("hex");
        let orphaned: Envelope<Todo> = mk_sealed(Some(absent.clone()), event);
        let name = seal_hash(&orphaned);
        let dag: Dag<Todo> = [(name.clone(), orphaned)].into_iter().collect();
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
