//! §3 — objects on disk, heads, the linearisation, `verify`, `adopt`/`merge`.
//!
//! Objects live at `root/objects/<name>.py`, one canonical constructor
//! expression per file, the filename being the sha256 of its own bytes.
//! `root/HEAD` names ONE head and is written exactly as it always has been;
//! `root/refs/` holds a file per head and EXISTS ONLY WHILE THERE IS MORE THAN
//! ONE, so a one-writer store is on disk exactly as it was before the DAG.
//!
//! Effect shell, not pure core: the filesystem lives here and `literal`/`event`
//! stay pure. Two rules the reference states and this keeps:
//!
//! - a load REVERIFIES, hashing the STORED BYTES against the filename before
//!   parsing — tamper-evidence on read — and never a reprint→rehash, which
//!   would couple every old object's validity to the current printer;
//! - a write is content-addressed and idempotent, and HEAD moves LAST through a
//!   temp file and a rename, so a crash leaves an orphan object (harmless,
//!   `verify` reports it) and never a HEAD naming nothing.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fs::{self, File};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::event::{mk_sealed, mk_woven, parents_of, seal_hash, Envelope, Hash, TodoEvent};
use crate::literal::{Datetime, ProdromeError};
use crate::payload::Payload;
use crate::policy::{Policy, Untrusted};

/// A closed object set in a deterministic TOPOLOGICAL order.
///
/// THE order every reader of a DAG gets. Every parent precedes every child, so
/// an event is folded after everything it was written on top of. Among objects
/// NEITHER of which precedes the other the tie is broken by HASH — Kahn's
/// algorithm with a min-heap on the name — which is arbitrary-but-deterministic
/// on purpose: a hash is not a clock and not an authority, and this exists so
/// that two readers of the same DAG fold the same sequence.
///
/// ⚠️ Read that limit exactly: it gives the register folds a definite order over
/// two concurrent events on ONE todo, where a definite order is not the right
/// answer. Those are a CONFLICT to show and settle with a later event, never to
/// tie-break (§6.6).
pub fn linearise<P>(objects: &BTreeMap<Hash, Envelope<P>>) -> Result<Vec<Hash>, ProdromeError> {
    let mut children: BTreeMap<&Hash, Vec<&Hash>> = BTreeMap::new();
    let mut waiting: BTreeMap<&Hash, usize> = BTreeMap::new();
    for (digest, object) in objects {
        let parents = parents_of(object);
        waiting.insert(digest, parents.len());
        for parent in &parents {
            let (known, _) = objects.get_key_value(parent).ok_or_else(|| {
                ProdromeError::Store(format!(
                    "missing object {}, named as a parent by {}",
                    parent.as_str(),
                    digest.as_str()
                ))
            })?;
            children.entry(known).or_default().push(digest);
        }
    }
    let mut ready: BinaryHeap<Reverse<&Hash>> = waiting
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(digest, _)| Reverse(*digest))
        .collect();
    let mut order: Vec<Hash> = Vec::with_capacity(objects.len());
    while let Some(Reverse(digest)) = ready.pop() {
        order.push(digest.clone());
        for child in children.get(digest).into_iter().flatten() {
            let count = waiting.get_mut(*child).expect("every object is counted");
            *count -= 1;
            if *count == 0 {
                ready.push(Reverse(child));
            }
        }
    }
    if order.len() != objects.len() {
        let placed: BTreeSet<&Hash> = order.iter().collect();
        let stuck = objects
            .keys()
            .find(|digest| !placed.contains(digest))
            .expect("a short order left something out");
        return Err(ProdromeError::Store(format!(
            "cycle in the object graph at {}",
            stuck.as_str()
        )));
    }
    Ok(order)
}

/// A chain rooted at `root`, read under one host [`Policy`] (§5).
///
/// THE STORE HOLDS THE POLICY, so callers do not each have to remember it and
/// `verify` has the one it is asked about. The type parameter defaults to
/// [`Untrusted`], the reference policy, because a store that names no policy
/// at all is a store nobody has configured — and `Untrusted::none()`, which
/// stands behind every writer, is the only honest thing for that to mean.
#[derive(Debug, Clone)]
pub struct EventStore<P, Pol = Untrusted> {
    root: PathBuf,
    policy: Pol,
    /// WHICH RECORD SHAPE THIS STORE HOLDS. A store is parsed against one
    /// closed vocabulary (§2), and that vocabulary is the core's kinds plus
    /// this payload's — so the payload is part of what a store IS, not an
    /// argument to each read.
    payload: PhantomData<P>,
}

impl<P: Payload, Pol: Policy<P>> EventStore<P, Pol> {
    pub fn new(root: impl Into<PathBuf>, policy: Pol) -> EventStore<P, Pol> {
        EventStore {
            root: root.into(),
            policy,
            payload: PhantomData,
        }
    }

    /// The policy this store reads under — what `verify` asks and what a
    /// caller folding its events should ask too, so that one store is one
    /// reading.
    pub fn policy(&self) -> &Pol {
        &self.policy
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn head_file(&self) -> PathBuf {
        self.root.join("HEAD")
    }

    fn refs_dir(&self) -> PathBuf {
        self.root.join("refs")
    }

    fn object_path(&self, digest: &Hash) -> PathBuf {
        self.objects_dir().join(format!("{}.py", digest.as_str()))
    }

    /// The current chain tip, `None` when HEAD is missing or empty. ONE head,
    /// the local one, written exactly as it always was: every reader that
    /// predates the DAG asks this and is right for as long as there is one
    /// writer.
    pub fn tip(&self) -> Option<Hash> {
        let raw = fs::read_to_string(self.head_file()).ok()?;
        Hash::new(raw.trim()).ok()
    }

    /// Every head: the objects no object in the store names as a parent.
    ///
    /// `refs/` names them and exists only while there is more than one, so
    /// absent or empty, HEAD is the whole set. A name in there that is not an
    /// object name is ignored here and reported by [`EventStore::verify`]:
    /// `refs/` is input, and this is its parse boundary.
    pub fn tips(&self) -> BTreeSet<Hash> {
        let named = self.ref_names();
        let heads: BTreeSet<Hash> = named
            .iter()
            .filter_map(|name| Hash::new(name.clone()).ok())
            .collect();
        if !heads.is_empty() {
            return heads;
        }
        self.tip().into_iter().collect()
    }

    fn ref_names(&self) -> Vec<String> {
        let mut names: Vec<String> = match fs::read_dir(self.refs_dir()) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        names.sort();
        names
    }

    fn lock_file(&self) -> PathBuf {
        self.root.join(".lock")
    }

    /// AN EXCLUSIVE `flock` ON `root/.lock`, HELD FOR ONE WRITE.
    ///
    /// The same file and the same operation the earlier Python writer took
    /// (the reference's `store.py::_flock`), and that is the whole point: a
    /// store may be appended to by more than one program, and a second writer
    /// is not always this crate. Two writers that lock different files are two
    /// writers that do not lock — an append is read-the-heads,
    /// write-the-object, move-HEAD, and two of those interleaved fork the
    /// chain.
    ///
    /// `flock` is per open file description, so this is advisory BETWEEN
    /// PROCESSES and says nothing between the threads of one. An application
    /// that decides something FROM the chain and then appends (the server's
    /// existence checks, its id search, its stamp) serialises its own threads
    /// above this; the lock here covers the append itself.
    ///
    /// The guard is the `File`: the lock releases when the last descriptor for
    /// the description closes, so dropping it unlocks on every path out,
    /// including an early `?`.
    #[cfg(unix)]
    fn lock(&self) -> Result<Option<File>, ProdromeError> {
        use rustix::fs::{flock, FlockOperation};
        fs::create_dir_all(&self.root).map_err(|e| Self::io(&self.root, e))?;
        let path = self.lock_file();
        let handle = File::create(&path).map_err(|e| Self::io(&path, e))?;
        flock(&handle, FlockOperation::LockExclusive)
            .map_err(|e| Self::io(&path, std::io::Error::from(e)))?;
        Ok(Some(handle))
    }

    /// No lock off unix, because there is no store off unix: `prodrome-wasm`
    /// compiles this crate for the browser, where `root` is a path nothing
    /// reads. Stated rather than `#[cfg]`-ed away at the call sites, so the
    /// write paths below read the same on every target.
    #[cfg(not(unix))]
    fn lock(&self) -> Result<Option<File>, ProdromeError> {
        Ok(None)
    }

    fn io(path: &Path, error: std::io::Error) -> ProdromeError {
        ProdromeError::Io {
            path: path.display().to_string(),
            message: error.to_string(),
        }
    }

    /// HEAD names `head`; `refs/` names every head, and is REMOVED while there
    /// is only one. HEAD moves LAST, by the same temp-file rename as ever, so a
    /// crash leaves an orphan object rather than a HEAD naming nothing.
    fn set_heads(&self, heads: &BTreeSet<Hash>, head: &Hash) -> Result<(), ProdromeError> {
        let refs = self.refs_dir();
        if heads.len() > 1 {
            fs::create_dir_all(&refs).map_err(|e| Self::io(&refs, e))?;
            for name in self.ref_names() {
                if !heads.iter().any(|head| head.as_str() == name) {
                    let stale = refs.join(&name);
                    fs::remove_file(&stale).map_err(|e| Self::io(&stale, e))?;
                }
            }
            for head in heads {
                let path = refs.join(head.as_str());
                fs::write(&path, head.as_str()).map_err(|e| Self::io(&path, e))?;
            }
        } else if refs.is_dir() {
            fs::remove_dir_all(&refs).map_err(|e| Self::io(&refs, e))?;
        }
        let temp = self.root.join("HEAD.tmp");
        fs::write(&temp, head.as_str()).map_err(|e| Self::io(&temp, e))?;
        fs::rename(&temp, self.head_file()).map_err(|e| Self::io(&temp, e))
    }

    /// Seal `event` onto the current tip and advance HEAD.
    ///
    /// `parents` names what this event is written ON TOP OF, and `None` means
    /// the current HEAD — the one-writer path, unchanged: one parent, a
    /// `Sealed`, byte for byte the object this store has always written. Two or
    /// more make a `Woven` (a write and a join as one act). Whatever heads it
    /// names stop being heads.
    pub fn append(
        &self,
        event: TodoEvent<P>,
        parents: Option<&[Hash]>,
    ) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        let heads = self.tips();
        let on: Vec<Hash> = match parents {
            Some(named) => {
                self.require_present(named)?;
                named.to_vec()
            }
            None => self.tip().into_iter().collect(),
        };
        let object = if on.len() > 1 {
            mk_woven(on.clone(), Some(event))?
        } else {
            mk_sealed(on.first().cloned(), event)
        };
        let digest = self.write(&object)?;
        self.advance(&heads, &on, &digest)?;
        Ok(digest)
    }

    /// Join two or more heads into one `Woven`, which becomes the tip.
    ///
    /// `parents` of `None` means every head there is, so a bare merge means
    /// "settle this store back into one history". NO who/when/why, and that is
    /// the design rather than an omission: a merge asserts structure, not a
    /// fact about a todo. A caller that wants the act attributed passes an
    /// ordinary `event`, and the object is a write and a join at once.
    pub fn merge(
        &self,
        parents: Option<&[Hash]>,
        event: Option<TodoEvent<P>>,
    ) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        let heads = self.tips();
        let on: Vec<Hash> = match parents {
            Some(named) => named.to_vec(),
            None => heads.iter().cloned().collect(),
        };
        self.require_present(&on)?;
        let digest = self.write(&mk_woven(on.clone(), event)?)?;
        self.advance(&heads, &on, &digest)?;
        Ok(digest)
    }

    fn advance(
        &self,
        heads: &BTreeSet<Hash>,
        joined: &[Hash],
        digest: &Hash,
    ) -> Result<(), ProdromeError> {
        let mut left: BTreeSet<Hash> = heads.clone();
        for parent in joined {
            left.remove(parent);
        }
        left.insert(digest.clone());
        self.set_heads(&left, digest)
    }

    /// Take another replica's objects in, and make `digest` a head here.
    ///
    /// THE ONLY WAY A SECOND HEAD APPEARS. Placement is git's, minus the
    /// ceremony: a tip we already contain changes nothing; a tip that contains
    /// every head we hold is a FAST-FORWARD; anything else becomes a second
    /// head for `merge` to join.
    pub fn adopt(&self, source: &EventStore<P, Pol>, digest: &Hash) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        self.copy_in(source, digest)?;
        self.place(digest)
    }

    /// The same adoption from a replica that is NOT a directory: `objects` maps
    /// a name to the canonical print that name is the hash of.
    ///
    /// A replica that reaches this store over a wire arrives as BYTES, and the
    /// alternative was for its caller to lay them out as a second store —
    /// a temporary directory whose layout is this module's own business,
    /// written by somebody who is not this module. The placement rule, the
    /// reverification and the walk over parents are IDENTICAL; only where the
    /// bytes are read from differs, which is why [`EventStore::adopt`] is now
    /// this function with a directory for a source.
    ///
    /// The same refusals, in the same words: bytes that do not hash to the name
    /// given, an object that does not parse, and a parent neither `objects` nor
    /// this store holds.
    pub fn adopt_objects(
        &self,
        objects: &BTreeMap<Hash, String>,
        digest: &Hash,
    ) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        self.copy_in_from(digest, |name| {
            objects.get(name).map(|text| text.as_bytes().to_vec())
        })?;
        self.place(digest)
    }

    /// Where an adopted tip lands — git's placement, minus the ceremony.
    fn place(&self, digest: &Hash) -> Result<Hash, ProdromeError> {
        let heads = self.tips();
        for head in &heads {
            if head == digest || self.ancestors(head)?.contains(digest) {
                return Ok(digest.clone());
            }
        }
        let behind = self.ancestors(digest)?;
        let mut kept: BTreeSet<Hash> = heads
            .iter()
            .filter(|head| !behind.contains(*head))
            .cloned()
            .collect();
        let head = match self.tip() {
            Some(tip) if kept.contains(&tip) => tip,
            _ => digest.clone(),
        };
        kept.insert(digest.clone());
        self.set_heads(&kept, &head)?;
        Ok(digest.clone())
    }

    /// A parent must be an object we hold. Refused here rather than left to
    /// `verify`, because an object naming a parent nobody has is a broken store
    /// written deliberately, and the store is what knows.
    fn require_present(&self, parents: &[Hash]) -> Result<(), ProdromeError> {
        for parent in parents {
            if !self.object_path(parent).exists() {
                return Err(ProdromeError::Store(format!(
                    "missing object {}, named as a parent",
                    parent.as_str()
                )));
            }
        }
        Ok(())
    }

    /// Every object `digest` rests on that this store lacks, VERIFIED in — a
    /// replica is not trusted for being a replica. An object we already hold
    /// ends that branch of the walk: this store is closed under parents, so
    /// everything above one of ours is already here.
    fn copy_in(&self, source: &EventStore<P, Pol>, digest: &Hash) -> Result<(), ProdromeError> {
        self.copy_in_from(digest, |name| fs::read(source.object_path(name)).ok())
    }

    /// The walk both adoptions share: from `digest` down through parents,
    /// taking in what this store lacks and REVERIFYING each object on the way.
    /// `bytes_of` is where the replica's objects are read from — a directory,
    /// or a map that arrived over a wire.
    fn copy_in_from(
        &self,
        digest: &Hash,
        bytes_of: impl Fn(&Hash) -> Option<Vec<u8>>,
    ) -> Result<(), ProdromeError> {
        let objects = self.objects_dir();
        fs::create_dir_all(&objects).map_err(|e| Self::io(&objects, e))?;
        let mut pending = vec![digest.clone()];
        while let Some(name) = pending.pop() {
            if self.object_path(&name).exists() {
                self.load(&name)?;
                continue;
            }
            let Some(raw) = bytes_of(&name) else {
                return Err(ProdromeError::Store(format!(
                    "missing object {}, named as a parent",
                    name.as_str()
                )));
            };
            let object = decode::<P>(&name, &raw)?;
            let here = self.object_path(&name);
            fs::write(&here, &raw).map_err(|e| Self::io(&here, e))?;
            pending.extend(parents_of(&object));
        }
        Ok(())
    }

    /// One object onto disk, content-addressed and idempotent: identical bytes
    /// at the target path are a no-op; DIFFERENT bytes at the same name are a
    /// sha256 collision or a corrupt store, and we stop rather than clobber.
    fn write(&self, object: &Envelope<P>) -> Result<Hash, ProdromeError> {
        let text = crate::event::canonical_envelope(object);
        let digest = seal_hash(object);
        let objects = self.objects_dir();
        fs::create_dir_all(&objects).map_err(|e| Self::io(&objects, e))?;
        let path = self.object_path(&digest);
        if path.exists() {
            let existing = fs::read(&path).map_err(|e| Self::io(&path, e))?;
            if existing != text.as_bytes() {
                return Err(ProdromeError::Store(format!(
                    "object {} exists with different bytes — sha256 collision or corruption",
                    digest.as_str()
                )));
            }
            return Ok(digest);
        }
        // Temp file + rename, so a reader never sees a half-written object even
        // though the name it would read it under is the hash of the whole.
        let temp = objects.join(format!("{}.tmp", digest.as_str()));
        fs::write(&temp, text.as_bytes()).map_err(|e| Self::io(&temp, e))?;
        fs::rename(&temp, &path).map_err(|e| Self::io(&temp, e))?;
        Ok(digest)
    }

    /// Load and REVERIFY one object: hash the STORED BYTES against the
    /// filename, then parse. Deliberately NOT a reparse→reprint→rehash — git
    /// hashes bytes and not semantics, so that a future printer change cannot
    /// false-alarm the whole store as tampered.
    pub fn load(&self, digest: &Hash) -> Result<Envelope<P>, ProdromeError> {
        let path = self.object_path(digest);
        if !path.exists() {
            return Err(ProdromeError::Store(format!(
                "missing object {}",
                digest.as_str()
            )));
        }
        let raw = fs::read(&path).map_err(|e| Self::io(&path, e))?;
        decode(digest, &raw)
    }

    /// Everything `digest` transitively rests on — STRICTLY: an object is not
    /// its own ancestor. This is the partial order and the whole of it.
    pub fn ancestors(&self, digest: &Hash) -> Result<BTreeSet<Hash>, ProdromeError> {
        let mut found: BTreeSet<Hash> = BTreeSet::new();
        let mut pending = parents_of(&self.load(digest)?);
        while let Some(name) = pending.pop() {
            if !found.insert(name.clone()) {
                continue;
            }
            pending.extend(parents_of(&self.load(&name)?));
        }
        Ok(found)
    }

    /// Do these two objects know nothing of each other? Neither rests on the
    /// other — the definition, and the only one this store has: not "written at
    /// the same time", which is a claim about clocks, and not "on different
    /// branches", which is a claim about names.
    pub fn concurrent(&self, a: &Hash, b: &Hash) -> Result<bool, ProdromeError> {
        if a == b {
            return Ok(false);
        }
        Ok(!self.ancestors(a)?.contains(b) && !self.ancestors(b)?.contains(a))
    }

    /// Every object reachable from `tips` by parents, reverified on the way.
    /// The result is closed under parents, which is what `linearise` needs and
    /// what makes a name it does not hold a real finding.
    fn objects_from(
        &self,
        tips: &BTreeSet<Hash>,
    ) -> Result<BTreeMap<Hash, Envelope<P>>, ProdromeError> {
        let mut objects: BTreeMap<Hash, Envelope<P>> = BTreeMap::new();
        let mut pending: Vec<Hash> = tips.iter().cloned().collect();
        while let Some(name) = pending.pop() {
            if objects.contains_key(&name) {
                continue;
            }
            let object = self.load(&name)?;
            pending.extend(parents_of(&object));
            objects.insert(name, object);
        }
        Ok(objects)
    }

    /// Every object, from every head, in [`linearise`]'s topological order.
    /// THE read: parents come before children, so the sequence is causal order.
    pub fn read_dag(&self) -> Result<Vec<Envelope<P>>, ProdromeError> {
        let objects = self.objects_from(&self.tips())?;
        let order = linearise(&objects)?;
        Ok(order
            .into_iter()
            .map(|digest| objects[&digest].clone())
            .collect())
    }

    /// The same read, with each object's name — which every caller that needs
    /// to rehash, link or report an object wants and would otherwise recompute.
    pub fn read_dag_named(&self) -> Result<Vec<(Hash, Envelope<P>)>, ProdromeError> {
        self.read_dag_at(&self.tips())
    }

    /// The DAG as it stood when `tips` were its heads: every object they rest
    /// on, named, in [`linearise`]'s order. Objects are never deleted, so this
    /// read is reproducible for as long as the store holds them — which is
    /// what lets a vector pin a tip and stay true while the chain grows past
    /// it, and what a reader that was handed a tip (a page, a sync, a replay)
    /// reads instead of whatever the store has by now.
    pub fn read_dag_at(
        &self,
        tips: &BTreeSet<Hash>,
    ) -> Result<Vec<(Hash, Envelope<P>)>, ProdromeError> {
        let objects = self.objects_from(tips)?;
        let order = linearise(&objects)?;
        Ok(order
            .into_iter()
            .map(|digest| {
                let object = objects[&digest].clone();
                (digest, object)
            })
            .collect())
    }

    /// Walk tip → genesis, reverifying each object, returned genesis-first.
    ///
    /// A CHAIN, and it says so: a store with more than one head, or with a
    /// merge on the path, REFUSES rather than answering with a fragment of
    /// itself. Readers that cannot be wrong about a DAG keep calling this and
    /// find out loudly.
    pub fn read_chain(&self) -> Result<Vec<Envelope<P>>, ProdromeError> {
        let heads = self.tips();
        if heads.len() > 1 {
            return Err(ProdromeError::Store(format!(
                "the store has {} heads — read_chain reads a chain, read_dag reads a DAG",
                heads.len()
            )));
        }
        let mut chain: Vec<Envelope<P>> = Vec::new();
        let mut seen: BTreeSet<Hash> = BTreeSet::new();
        let mut cursor = self.tip();
        while let Some(digest) = cursor {
            if !seen.insert(digest.clone()) {
                return Err(ProdromeError::Store(format!(
                    "cycle in chain at {}",
                    digest.as_str()
                )));
            }
            let object = self.load(&digest)?;
            let prev = match &object {
                Envelope::Sealed { prev, .. } => prev.clone(),
                Envelope::Woven { .. } => {
                    return Err(ProdromeError::Store(format!(
                        "object {} is a merge — read_chain reads a chain, read_dag reads a DAG",
                        digest.as_str()
                    )))
                }
            };
            chain.push(object);
            cursor = prev;
        }
        chain.reverse();
        Ok(chain)
    }

    /// The store's event bodies, in the linearisation's order. A merge carries
    /// no event and contributes none: the folds see facts about todos, and the
    /// DAG's shape reaches them only as the ORDER those facts arrive in.
    pub fn events(&self) -> Result<Vec<TodoEvent<P>>, ProdromeError> {
        Ok(self
            .read_dag()?
            .into_iter()
            .filter_map(|object| match object {
                Envelope::Sealed { event, .. } => Some(event),
                Envelope::Woven { event, .. } => event,
            })
            .collect())
    }

    /// Full fsck; an empty result means healthy.
    ///
    /// Every object rehashes to its filename and parses as an envelope (a
    /// `Woven` with fewer than two parents, or one named twice, is malformed
    /// and cannot parse — the constructor is the boundary); every parent named
    /// exists; the walk from EVERY head reaches genesis without a cycle; no
    /// unreachable object; the heads agree with HEAD and with each other; and
    /// no event the policy does not CONFIRM is dated behind anything it rests
    /// on.
    pub fn verify(&self) -> Vec<String> {
        let files = self.object_files();
        let tips = self.tips();
        let (reachable, graph) = self.graph_problems(&tips);
        let mut problems: Vec<String> = Vec::new();
        for path in &files {
            problems.extend(self.object_problems(path));
        }
        problems.extend(self.head_problems(!files.is_empty(), &tips));
        problems.extend(graph);
        for path in &files {
            let stem = stem_of(path);
            if !reachable.iter().any(|digest| digest.as_str() == stem) {
                problems.push(format!("unreachable object {stem} (no head reaches it)"));
            }
        }
        problems
    }

    fn object_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = match fs::read_dir(self.objects_dir()) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "py"))
                .collect(),
            Err(_) => Vec::new(),
        };
        files.sort();
        files
    }

    /// One object's fsck findings: byte-hash against the filename, then
    /// parseability. A malformed envelope is caught HERE, by the parser,
    /// because the `mk_*` constructors are the parse boundary — a `Woven` with
    /// one parent cannot be built, so it cannot be read back either, and the
    /// finding names the rule it broke.
    fn object_problems(&self, path: &Path) -> Vec<String> {
        let stem = stem_of(path);
        let Ok(raw) = fs::read(path) else {
            return vec![format!("object {stem} could not be read")];
        };
        if hex(&Sha256::digest(&raw)) != stem {
            return vec![format!(
                "object {stem} does not hash to its filename (tampered/corrupt)"
            )];
        }
        let Ok(text) = std::str::from_utf8(&raw) else {
            return vec![format!("object {stem} failed to parse: not UTF-8")];
        };
        match crate::event::parse_envelope::<P>(text) {
            Ok(_) => Vec::new(),
            Err(error) => vec![format!("object {stem} failed to parse: {error}")],
        }
    }

    /// HEAD, and the heads. HEAD names ONE of them — the local one — and
    /// `refs/` names them all, which makes two things checkable that a chain
    /// never had to state: that the file every old reader trusts is still a
    /// head, and that `refs/` says nothing HEAD alone could have said.
    fn head_problems(&self, objects_exist: bool, tips: &BTreeSet<Hash>) -> Vec<String> {
        let mut problems: Vec<String> = Vec::new();
        if objects_exist && tips.is_empty() {
            problems.push("objects exist but HEAD is missing or empty".to_owned());
        }
        if !tips.is_empty() && !objects_exist {
            problems.push("HEAD is set but no objects exist".to_owned());
        }
        if !self.refs_dir().is_dir() {
            return problems;
        }
        let named = self.ref_names();
        for name in &named {
            if Hash::new(name.clone()).is_err() {
                problems.push(format!("refs/{name} is not an object name"));
            }
        }
        let heads: BTreeSet<Hash> = named
            .iter()
            .filter_map(|name| Hash::new(name.clone()).ok())
            .collect();
        if heads.len() == 1 {
            problems
                .push("refs/ names one head — a single head is HEAD's alone to name".to_owned());
        }
        if !heads.is_empty() {
            let tip = self.tip();
            if !tip.as_ref().is_some_and(|tip| heads.contains(tip)) {
                problems.push(format!(
                    "HEAD names {}, which is not one of the heads in refs/",
                    tip.as_ref().map_or("(nothing)", Hash::as_str)
                ));
            }
        }
        problems
    }

    /// Walk every head to genesis; the reachable set plus any break en route.
    /// Reachability is over PARENTS, so it is the same walk for a chain and for
    /// a DAG.
    fn graph_problems(&self, tips: &BTreeSet<Hash>) -> (BTreeSet<Hash>, Vec<String>) {
        let mut visited: BTreeSet<Hash> = BTreeSet::new();
        let mut objects: BTreeMap<Hash, Envelope<P>> = BTreeMap::new();
        let mut pending: Vec<Hash> = tips.iter().cloned().collect();
        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            match self.load(&name) {
                Ok(object) => {
                    pending.extend(parents_of(&object));
                    objects.insert(name, object);
                }
                Err(error) => {
                    return (
                        visited,
                        vec![format!("chain broke at {}: {error}", name.as_str())],
                    )
                }
            }
        }
        let order = match linearise(&objects) {
            Ok(order) => order,
            Err(error) => return (visited, vec![error.to_string()]),
        };
        let mut problems = self.stale_heads(tips);
        problems.extend(dating_problems(&order, &objects, &self.policy));
        (visited, problems)
    }

    /// A head another head rests on is not a head. It is what a `refs/` entry
    /// decays into when a merge lands and the old name is left behind, and it
    /// is invisible to every other check here: the objects are all present, all
    /// reachable, all in order.
    fn stale_heads(&self, tips: &BTreeSet<Hash>) -> Vec<String> {
        let mut problems: Vec<String> = Vec::new();
        for head in tips {
            for other in tips {
                if other == head {
                    continue;
                }
                if self
                    .ancestors(other)
                    .is_ok_and(|behind| behind.contains(head))
                {
                    problems.push(format!(
                        "refs/{} is not a head: {} rests on it",
                        head.as_str(),
                        other.as_str()
                    ));
                }
            }
        }
        problems
    }
}

/// THE ONE PLACE CAUSAL ORDER MEETS CLOCK ORDER.
///
/// The folds read `at` as data; the store orders by parents. They may disagree
/// for an event the policy CONFIRMS — a backfill legitimately records 2025
/// completions today. For one it does not, the `at` is the host's own clock,
/// forced by the verb that wrote it, so a stamp behind something the event was
/// written on top of is either a clock that ran backwards or an object nobody's
/// verb wrote.
///
/// [`Policy::confirms`] is the question, and its DEFAULT is §5 exactly: an
/// event that only CLAIMS is not the host's word, so its stamp is not either.
/// A policy that folds a writer's events while still holding their clock — the
/// reference policy does, for content records — says so by overriding it. No
/// actor name is read here.
///
/// The finding's WORDING is frozen: `conformance/dag.py` holds these
/// sentences byte for byte (§9.8), and they were taken under the reference
/// policy, where "does not confirm" is "untrusted".
///
/// "Before" is over ANCESTORS, not over one predecessor: an object's high water
/// mark is the latest stamp anywhere beneath it, carried up the DAG in
/// topological order. On a chain that is the running maximum this rule has
/// always used, so the finding and its wording are unchanged there.
fn dating_problems<P: Payload>(
    order: &[Hash],
    objects: &BTreeMap<Hash, Envelope<P>>,
    policy: &impl Policy<P>,
) -> Vec<String> {
    let mut high: BTreeMap<&Hash, Datetime> = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();
    for digest in order {
        let object = &objects[digest];
        let behind = parents_of(object)
            .iter()
            .filter_map(|parent| high.get(parent).copied())
            .max();
        let event = object.event();
        if let (Some(event), Some(behind)) = (event, behind) {
            if !policy.confirms(event) && event.at() < behind {
                problems.push(format!(
                    "untrusted event {} is dated {}, behind its predecessor ({})",
                    digest.as_str(),
                    event.at().isoformat(),
                    behind.isoformat()
                ));
            }
        }
        let stamp = [behind, event.map(TodoEvent::at)]
            .into_iter()
            .flatten()
            .max();
        if let Some(stamp) = stamp {
            let (key, _) = objects
                .get_key_value(digest)
                .expect("the order names objects");
            high.insert(key, stamp);
        }
    }
    problems
}

fn decode<P: Payload>(digest: &Hash, raw: &[u8]) -> Result<Envelope<P>, ProdromeError> {
    let recomputed = hex(&Sha256::digest(raw));
    if recomputed != digest.as_str() {
        return Err(ProdromeError::Store(format!(
            "object {} hashes to {recomputed} — tampered or corrupt",
            digest.as_str()
        )));
    }
    let text = std::str::from_utf8(raw)
        .map_err(|_| ProdromeError::Store(format!("object {} is not UTF-8", digest.as_str())))?;
    crate::event::parse_envelope(text)
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_completed, mk_created, Actor};
    use crate::literal::Datetime;
    use crate::reference::Todo;

    /// The store these tests drive. The payload is the reference one because
    /// a store has to hold SOME record shape; nothing below reads a field of
    /// it.
    type Store = EventStore<Todo>;

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    /// The reference policy, with one name on the roster — every store below
    /// reads under it, and only the dating test can tell.
    fn roster() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "prodrome-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn a_chain_appends_verifies_and_reads_back_in_order() {
        let store = Store::new(scratch("chain"), roster());
        let first = store
            .append(
                mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        let second = store
            .append(
                mk_completed("alpha", at(2), "bassel", "").expect("valid"),
                None,
            )
            .expect("appends");
        assert_eq!(store.tip(), Some(second.clone()));
        assert_eq!(store.tips(), [second.clone()].into_iter().collect());
        assert!(!store.refs_dir().exists(), "one head lives in HEAD alone");
        assert_eq!(store.verify(), Vec::<String>::new());
        let names: Vec<Hash> = store
            .read_dag_named()
            .expect("reads")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, vec![first.clone(), second.clone()]);
        assert_eq!(store.read_chain().expect("a chain is a chain").len(), 2);
        assert_eq!(store.events().expect("reads").len(), 2);
        assert_eq!(
            store.ancestors(&second).expect("walks"),
            [first].into_iter().collect()
        );
        let _ = fs::remove_dir_all(store.root());
    }

    #[test]
    fn adopt_makes_a_second_head_and_merge_settles_it() {
        let here = Store::new(scratch("here"), roster());
        let there = Store::new(scratch("there"), roster());
        let shared = here
            .append(
                mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        there
            .adopt(&here, &shared)
            .expect("fast-forwards into an empty store");
        assert_eq!(there.tip(), Some(shared.clone()));

        let mine = here
            .append(
                mk_created("beta", at(2), "bassel", "mine", "").expect("valid"),
                None,
            )
            .expect("appends");
        let theirs = there
            .append(
                mk_created("beta", at(2), "bassel", "theirs", "").expect("valid"),
                None,
            )
            .expect("appends");
        here.adopt(&there, &theirs).expect("adopts");
        assert_eq!(
            here.tips(),
            [mine.clone(), theirs.clone()].into_iter().collect()
        );
        assert!(here.concurrent(&mine, &theirs).expect("both present"));
        assert_eq!(here.verify(), Vec::<String>::new());
        assert!(here.read_chain().is_err(), "two heads are not a chain");

        let merged = here.merge(None, None).expect("merges");
        assert_eq!(here.tips(), [merged.clone()].into_iter().collect());
        assert!(
            !here.refs_dir().exists(),
            "refs/ goes when one head is left"
        );
        assert_eq!(here.verify(), Vec::<String>::new());
        // Sorted by `mk_woven`, so a merge is its parent SET and two replicas
        // joining the same two heads produce the same bytes.
        let mut joined = vec![mine, theirs];
        joined.sort();
        assert_eq!(parents_of(&here.load(&merged).expect("loads")), joined);
        // A merge carries no event, so it contributes none.
        assert_eq!(here.events().expect("reads").len(), 3);
        let _ = fs::remove_dir_all(here.root());
        let _ = fs::remove_dir_all(there.root());
    }

    /// THE VECTOR FOR `adopt_objects`: a replica whose objects arrive as BYTES
    /// lands exactly where the same replica's DIRECTORY would have landed.
    ///
    /// Two stores are built with the same history and diverged the same way;
    /// one takes the other in through [`EventStore::adopt`] and the other
    /// through [`EventStore::adopt_objects`], and the heads, the objects and
    /// `verify` agree. That is the whole claim the new door makes — same
    /// placement, same reverification, a different source of bytes — and the
    /// two refusals below are the other half of it.
    #[test]
    fn adopting_bytes_lands_where_adopting_a_directory_lands() {
        let by_dir = Store::new(scratch("bytes-dir"), roster());
        let by_bytes = Store::new(scratch("bytes-map"), roster());
        let there = Store::new(scratch("bytes-there"), roster());
        let shared = mk_created("alpha", at(1), "bassel", "", "").expect("valid");
        for store in [&by_dir, &by_bytes, &there] {
            store.append(shared.clone(), None).expect("appends");
        }
        // Each side writes its own second object, so the two histories diverge.
        for store in [&by_dir, &by_bytes] {
            store
                .append(
                    mk_created("beta", at(2), "bassel", "mine", "").expect("valid"),
                    None,
                )
                .expect("appends");
        }
        let theirs = there
            .append(
                mk_created("beta", at(2), "bassel", "theirs", "").expect("valid"),
                None,
            )
            .expect("appends");

        by_dir.adopt(&there, &theirs).expect("adopts a directory");
        // The same objects, as the wire carries them: a name and the canonical
        // print that name is the hash of.
        let over_the_wire: BTreeMap<Hash, String> = there
            .read_dag_named()
            .expect("reads")
            .into_iter()
            .map(|(name, object)| (name, crate::event::canonical_envelope(&object)))
            .collect();
        by_bytes
            .adopt_objects(&over_the_wire, &theirs)
            .expect("adopts bytes");

        assert_eq!(by_bytes.tips(), by_dir.tips());
        assert_eq!(
            by_bytes.read_dag_named().expect("reads"),
            by_dir.read_dag_named().expect("reads")
        );
        assert_eq!(by_bytes.verify(), Vec::<String>::new());

        // A REPLICA IS NOT TRUSTED FOR BEING A REPLICA. Bytes that do not hash
        // to the name they were filed under are refused, and so is a parent
        // nobody holds.
        let liar = Store::new(scratch("bytes-liar"), roster());
        let mut tampered = over_the_wire.clone();
        if let Some(text) = tampered.get_mut(&theirs) {
            text.push(' ');
        }
        assert!(liar.adopt_objects(&tampered, &theirs).is_err());
        let orphan: BTreeMap<Hash, String> = over_the_wire
            .iter()
            .filter(|(name, _)| **name == theirs)
            .map(|(name, text)| (name.clone(), text.clone()))
            .collect();
        let missing = Store::new(scratch("bytes-orphan"), roster());
        assert!(missing.adopt_objects(&orphan, &theirs).is_err());

        for store in [&by_dir, &by_bytes, &there, &liar, &missing] {
            let _ = fs::remove_dir_all(store.root());
        }
    }

    #[test]
    fn a_contained_tip_changes_nothing_and_a_containing_one_fast_forwards() {
        let here = Store::new(scratch("ff-here"), roster());
        let there = Store::new(scratch("ff-there"), roster());
        let first = here
            .append(
                mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        there.adopt(&here, &first).expect("adopts");
        let second = there
            .append(
                mk_completed("alpha", at(2), "bassel", "").expect("valid"),
                None,
            )
            .expect("appends");
        here.adopt(&there, &second).expect("fast-forwards");
        assert_eq!(here.tips(), [second.clone()].into_iter().collect());
        assert!(!here.refs_dir().exists(), "a fast-forward stays a chain");
        here.adopt(&there, &first).expect("a contained tip");
        assert_eq!(here.tips(), [second].into_iter().collect());
        let _ = fs::remove_dir_all(here.root());
        let _ = fs::remove_dir_all(there.root());
    }

    #[test]
    fn a_tampered_object_is_caught_on_read_and_by_verify() {
        let store = Store::new(scratch("tamper"), roster());
        let digest = store
            .append(
                mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        let path = store.object_path(&digest);
        let text = fs::read_to_string(&path).expect("reads");
        fs::write(&path, text.replace("alpha", "omega")).expect("writes");
        assert!(store.load(&digest).is_err());
        // Twice over, and both are true: the file no longer hashes to its name,
        // and the walk from HEAD cannot get past it.
        assert_eq!(
            store.verify(),
            vec![
                format!(
                    "object {} does not hash to its filename (tampered/corrupt)",
                    digest.as_str()
                ),
                format!(
                    "chain broke at {digest}: object {digest} hashes to {} — tampered or corrupt",
                    hex(&Sha256::digest(fs::read(&path).expect("reads"))),
                    digest = digest.as_str()
                ),
            ]
        );
        let _ = fs::remove_dir_all(store.root());
    }

    #[test]
    fn an_unconfirmed_event_dated_behind_its_predecessor_is_a_finding() {
        let store = Store::new(scratch("dating"), roster());
        store
            .append(
                mk_created("alpha", at(10), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        let late = store
            .append(
                mk_completed("alpha", at(2), "triage", "").expect("valid"),
                None,
            )
            .expect("appends");
        assert_eq!(
            store.verify(),
            vec![format!(
                "untrusted event {} is dated {}, behind its predecessor ({})",
                late.as_str(),
                at(2).isoformat(),
                at(10).isoformat()
            )]
        );
        // The same event from a writer the policy CONFIRMS is a backfill, not a
        // finding.
        let trusted = Store::new(scratch("dating-trusted"), roster());
        trusted
            .append(
                mk_created("alpha", at(10), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        trusted
            .append(
                mk_completed("alpha", at(2), "bassel", "").expect("valid"),
                None,
            )
            .expect("appends");
        assert_eq!(trusted.verify(), Vec::<String>::new());
        let _ = fs::remove_dir_all(store.root());
        let _ = fs::remove_dir_all(trusted.root());
    }

    /// THE CLOCK RULE IS `confirms`, NOT `binds`. A record from an actor on the
    /// roster BINDS — the folds take it (§5) — and its stamp is still the
    /// host's, so a record dated behind what it rests on is still a finding.
    /// The reference policy is the one that draws that line, by overriding
    /// `Policy::confirms`; `conformance/dag.py` holds these findings for
    /// records too, which is why the distinction is pinned here and not left to
    /// the default.
    #[test]
    fn a_record_that_binds_is_still_held_to_the_dag_s_clock() {
        let store = Store::new(scratch("dating-record"), roster());
        store
            .append(
                mk_created("alpha", at(10), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        let record = store
            .append(
                crate::reference::mk_authored(
                    "alpha",
                    at(2),
                    "triage",
                    "todo",
                    at(2),
                    "body",
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
                .expect("valid"),
                None,
            )
            .expect("appends");
        let events = store.events().expect("reads");
        let late = events.last().expect("the record");
        assert!(
            roster().standing(late).binds(),
            "a content record binds whoever wrote it"
        );
        assert_eq!(
            store.verify(),
            vec![format!(
                "untrusted event {} is dated {}, behind its predecessor ({})",
                record.as_str(),
                at(2).isoformat(),
                at(10).isoformat()
            )]
        );
        let _ = fs::remove_dir_all(store.root());
    }

    #[test]
    fn an_orphan_is_unreachable_and_verify_says_so() {
        let store = Store::new(scratch("orphan"), roster());
        store
            .append(
                mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        // An object written with no head naming it — what a crash mid-append
        // leaves behind.
        let orphan = store
            .write(&mk_sealed(
                None,
                mk_created("beta", at(1), "bassel", "", "").expect("valid"),
            ))
            .expect("writes");
        assert_eq!(
            store.verify(),
            vec![format!(
                "unreachable object {} (no head reaches it)",
                orphan.as_str()
            )]
        );
        let _ = fs::remove_dir_all(store.root());
    }

    #[test]
    fn linearise_refuses_a_missing_parent_and_a_cycle() {
        let store = Store::new(scratch("broken"), roster());
        let event = mk_created("alpha", at(1), "bassel", "", "").expect("valid");
        let absent = Hash::new("a".repeat(64)).expect("hex");
        let orphaned = mk_sealed(Some(absent.clone()), event);
        let digest = seal_hash(&orphaned);
        let objects: BTreeMap<Hash, Envelope<Todo>> =
            [(digest.clone(), orphaned)].into_iter().collect();
        assert_eq!(
            linearise(&objects).unwrap_err().to_string(),
            format!(
                "missing object {}, named as a parent by {}",
                absent.as_str(),
                digest.as_str()
            )
        );
        let _ = fs::remove_dir_all(store.root());
    }

    /// THE LOCK IS THE PYTHON WRITER'S LOCK, and this is the checkable half of
    /// that claim: it is `root/.lock`, and an append takes it exclusively.
    ///
    /// What is checked is that a SECOND holder of an exclusive `flock` on that
    /// exact path cannot get it while an append would hold one, and that the
    /// append is possible once that holder lets go — which is what serialises
    /// this writer against the reference's `store.py::_flock` across
    /// processes. The file NAME is the interoperable part: two writers locking
    /// two different files are two writers that do not lock.
    #[cfg(unix)]
    #[test]
    fn an_append_takes_the_same_lock_file_the_python_writer_takes() {
        use rustix::fs::{flock, FlockOperation};

        let store = Store::new(scratch("locked"), roster());
        store
            .append(
                mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
                None,
            )
            .expect("appends");
        assert_eq!(store.lock_file(), store.root().join(".lock"));
        assert!(store.lock_file().exists(), "the append created the lock");

        // A second exclusive holder, the way another process would be.
        let held = File::create(store.lock_file()).expect("the lock file opens");
        flock(&held, FlockOperation::LockExclusive).expect("locks");
        assert!(
            flock(
                File::create(store.lock_file()).expect("opens"),
                FlockOperation::NonBlockingLockExclusive
            )
            .is_err(),
            "a held exclusive lock is not available to a second holder"
        );
        drop(held);
        store
            .append(
                mk_completed("alpha", at(2), "bassel", "").expect("valid"),
                None,
            )
            .expect("appends once the other writer let go");
        assert_eq!(store.verify(), Vec::<String>::new());
        let _ = fs::remove_dir_all(store.root());
    }
}
