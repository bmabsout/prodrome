//! §3 — the files around a [`Dag`]: objects on disk, the lock, durable
//! placement, quarantine, `adopt`/`merge`.
//!
//! Objects live at `root/objects/<name>.py`, one canonical constructor
//! expression per file, the filename being the sha256 of its own bytes. THAT IS
//! THE WHOLE STORE. Its tips are DERIVED — the objects no object names as a
//! parent ([`Dag::tips`]) — so nothing on disk names a head, and two copies of a
//! store that each only added files are merged by putting their files in one
//! directory: a `git merge` of two clones IS the union, and nothing in it can
//! conflict. A store written before 0.9 also holds `HEAD` (and `refs/` while it
//! had several heads). Nothing reads or writes them any more — the objects
//! imply what they named (SPEC §9.17) — and `verify` asks for them to go.
//!
//! Effect shell, not pure core: the filesystem lives here and `literal`/`event`
//! stay pure. Two rules the reference states and this keeps:
//!
//! - a read VERIFIES, hashing the STORED BYTES against the filename before
//!   parsing — tamper-evidence on read — and never a reprint→rehash, which
//!   would couple every old object's validity to the current printer. A
//!   handle verifies each object once and holds it (`store::memory`); a held
//!   object is read again only when its file no longer looks as it did, and
//!   `verify` rehashes everything, on purpose;
//! - a write is content-addressed and idempotent, through a randomly named
//!   temp file synced before it is renamed, so the object appearing under its
//!   name IS the append: there is no second file to move after it, and no
//!   crash leaves a store half-written or an object empty.

mod memory;

use std::collections::hash_map::RandomState;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use crate::change::mk_change;
use crate::dag::{decode, Dag, Finding, Unread};
use crate::event::{canonical, mk_woven, parents_of, seal_hash, Envelope, Hash};
use crate::genesis::mk_genesis;
use crate::literal::ProdromeError;
use crate::policy::{Policy, Untrusted};
use crate::registers::{deps_for, fold, Folded};
use crate::schema::Schema;
use crate::snapshot::mk_snapshot;

use memory::{Memory, Seen, Verified};

/// A chain rooted at `root`, of the schema `E`, read under one host
/// [`Policy`] (§5).
///
/// THE STORE HOLDS THE POLICY, so callers do not each have to remember it and
/// `verify` has the one it is asked about. The type parameter defaults to
/// [`Untrusted`], the reference policy, because a store that names no policy
/// at all is a store nobody has configured — and `Untrusted::none()`, which
/// stands behind every writer, is the only honest thing for that to mean.
#[derive(Debug, Clone)]
pub struct EventStore<E: Schema, Pol = Untrusted> {
    root: PathBuf,
    policy: Pol,
    /// WHAT THIS HANDLE (OR A CLONE OF IT) HAS READ: every object, verified
    /// once, and their fold. A read or an append looks at `objects/` and
    /// reads only what the memory does not hold, so it costs the objects
    /// new since the last look and not the store.
    ///
    /// The schema is the memory's: a store is parsed against one closed
    /// vocabulary (§2), the envelopes' and its schema's, so the schema is
    /// part of what a store IS, not an argument to each read. No object
    /// carries it.
    ///
    /// A `Mutex`, because every read may extend it, and a store is shared
    /// between threads: `EventStore` is `Send` and `Sync` wherever its
    /// schema's events and its policy are.
    memory: Arc<Mutex<Memory<E>>>,
    /// The prodrome this handle writes into, for a store that holds several.
    genesis: Option<Hash>,
}

/// `objects/`, read once: the entries named as objects, and every other
/// entry's file name.
#[derive(Debug, Default)]
struct Listing {
    objects: Vec<(Hash, fs::DirEntry)>,
    garbage: Vec<String>,
}

impl<E: Schema, Pol: Policy<E>> EventStore<E, Pol> {
    pub fn new(root: impl Into<PathBuf>, policy: Pol) -> EventStore<E, Pol> {
        EventStore {
            root: root.into(),
            policy,
            memory: Arc::new(Mutex::new(Memory::empty())),
            genesis: None,
        }
    }

    /// The memory, whoever held it last. A panic while it was held may have
    /// left it half extended, so a poisoned memory is forgotten and the next
    /// look reads the store as a fresh handle does.
    fn memory(&self) -> MutexGuard<'_, Memory<E>> {
        self.memory.lock().unwrap_or_else(|poisoned| {
            let mut memory = poisoned.into_inner();
            *memory = Memory::empty();
            self.memory.clear_poison();
            memory
        })
    }

    /// Bring `memory` level with `objects/`. A name listed and not held is
    /// read and verified; a held name no longer listed is forgotten; a held
    /// file that no longer looks as it did when it was verified ([`Seen`]) is
    /// read and verified again. Nothing else is read, and nothing held is
    /// rehashed.
    ///
    /// Refuses as a cold read does, with the first file by name that is not
    /// an object; what did verify is held all the same.
    fn look(&self, memory: &mut Memory<E>) -> Result<(), ProdromeError> {
        let listed = self.listing()?.objects;
        let now = SystemTime::now();
        let gone: Vec<Hash> = {
            let listed: BTreeSet<&Hash> = listed.iter().map(|(name, _)| name).collect();
            memory
                .files()
                .keys()
                .filter(|name| !listed.contains(name))
                .cloned()
                .collect()
        };
        let mut fresh = Vec::new();
        let mut unread: BTreeMap<Hash, Unread> = BTreeMap::new();
        for (name, entry) in listed {
            let seen = Seen::of(stat(&entry), now);
            let held = match memory.files().get(&name) {
                Some(Some(was)) if seen.as_ref() == Some(was) => continue,
                held => held.is_some(),
            };
            match self
                .raw(&name)
                .map_err(Unread::Io)
                .and_then(|bytes| Verified::read(&name, &bytes))
            {
                Err(why) => {
                    unread.insert(name, why);
                }
                Ok(_) if held => memory.reseen(&name, seen),
                Ok(verified) => fresh.push((verified, seen)),
            }
        }
        memory.admit(&gone, fresh)?;
        match unread.into_iter().next() {
            Some((name, why)) => Err(why.refusal(&name)),
            None => Ok(()),
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

    fn object_path(&self, digest: &Hash) -> PathBuf {
        self.objects_dir().join(format!("{}.py", digest.as_str()))
    }

    /// Every head: the objects no object in the store names as a parent —
    /// [`Dag::tips`] over the store's objects, and nothing on disk besides.
    ///
    /// FALLIBLE, because it reads every object it does not hold: a store
    /// holding a file that does not verify has no honest answer to "what
    /// are its heads", since the file it cannot read might name any of them.
    /// The refusal does not guess, and it does not leave the store stuck
    /// either: a file failing its hash is named, with the
    /// [`EventStore::quarantine`] that sets it aside.
    /// Empty for an empty store. The tips are the memory's, so a steady store
    /// costs a directory listing.
    pub fn tips(&self) -> Result<BTreeSet<Hash>, ProdromeError> {
        let mut memory = self.memory();
        self.look(&mut memory)?;
        Ok(memory.tips().clone())
    }

    /// What `objects/` holds, SPLIT BY NAME: an entry named `<name>.py` for a
    /// well-formed object name is an object, and every other entry is not one
    /// — a temp an interrupted write left, or a stray somebody put there.
    ///
    /// The reads take the objects and pass over the rest, so a crash that left
    /// a temp stops no read and no write; `verify` reports the rest, every
    /// entry of it, so nothing in the directory goes unsaid. In no particular
    /// order — nothing that reads it depends on one (SPEC §9.17). A store
    /// with no `objects/` yet holds nothing.
    fn listing(&self) -> Result<Listing, ProdromeError> {
        let dir = self.objects_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Listing::default())
            }
            Err(error) => return Err(Self::io(&dir, error)),
        };
        let mut listing = Listing::default();
        for entry in entries {
            let entry = entry.map_err(|e| Self::io(&dir, e))?;
            let file = entry.file_name().to_string_lossy().into_owned();
            match file.strip_suffix(".py").map(Hash::new) {
                Some(Ok(name)) => listing.objects.push((name, entry)),
                _ => listing.garbage.push(file),
            }
        }
        Ok(listing)
    }

    /// Every object the store holds, refusing the first file that is not an
    /// object: what the memory holds, after reading what it does not.
    ///
    /// Shared, not copied: the memory goes on from this value, and copies it
    /// only if a look extends it while the caller still holds this one.
    pub fn dag(&self) -> Result<Arc<Dag<E>>, ProdromeError> {
        let mut memory = self.memory();
        self.look(&mut memory)?;
        Ok(Arc::clone(memory.dag()))
    }

    /// The fold of every object the store holds, across a parent set aside
    /// (what an append decides on): `fold(&dag.nodes_across_gaps()?)` of
    /// [`EventStore::dag`], extended by each object as it is read and never
    /// refolded from the files.
    pub fn folded(&self) -> Result<Arc<Folded<E>>, ProdromeError> {
        let mut memory = self.memory();
        self.look(&mut memory)?;
        memory.folded()
    }

    fn read(&self, names: &[Hash]) -> Dag<E> {
        Dag::from_prints(names.iter().map(|name| (name.clone(), self.raw(name))))
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
    /// write-the-object, and two of those interleaved fork the chain — which
    /// derived tips make harmless (the next write weaves both) but not
    /// intended.
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

    /// This handle, writing into the prodrome `genesis` begins; without one
    /// a handle writes into the store's only genesis.
    pub fn in_genesis(self, genesis: Hash) -> EventStore<E, Pol> {
        EventStore {
            genesis: Some(genesis),
            ..self
        }
    }

    fn genesis(&self, dag: &Dag<E>) -> Result<Hash, ProdromeError> {
        let geneses = dag.geneses();
        let mut sole = geneses.iter();
        match (&self.genesis, sole.next(), sole.next()) {
            (Some(mine), ..) if geneses.contains(mine) => Ok(mine.clone()),
            (Some(mine), ..) => Err(ProdromeError::Store(format!(
                "the store holds no genesis {}",
                mine.as_str()
            ))),
            (None, Some(one), None) => Ok(one.clone()),
            (None, None, _) => Err(ProdromeError::Store(
                "the store has no genesis: `init` it first".to_owned(),
            )),
            (None, Some(_), Some(_)) => Err(ProdromeError::Store(format!(
                "the store holds {} geneses: name the writer's with `in_genesis`",
                geneses.len()
            ))),
        }
    }

    /// Begin a prodrome: write a `Genesis(label, nonce)` into a store that
    /// has none, and answer its name.
    pub fn init(&self, label: &str) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        if let Some(genesis) = self.dag()?.geneses().first() {
            return Err(ProdromeError::Store(format!(
                "the store already has a genesis, {}",
                genesis.as_str()
            )));
        }
        let nonce = format!("{:016x}{:016x}", random(), random());
        self.write(&Envelope::Genesis(mk_genesis(label, &nonce)?))
    }

    /// Write `event` as a `Change` over the frontiers of the registers it
    /// writes, or, when the writer's prodrome already holds an object whose
    /// event prints the same, write nothing and answer the first such. A
    /// write that would take an inflationary register below the reading it
    /// supersedes is refused, and nothing is written.
    pub fn append(&self, event: E) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        let dag = self.dag()?;
        let genesis = self.genesis(&dag)?;
        let prodrome = dag.key(&genesis);
        let nodes = dag.nodes_across_gaps()?;
        let print = canonical(&event);
        let held = nodes.iter().find(|node| {
            node.genesis == prodrome && node.event.as_ref().is_some_and(|e| canonical(e) == print)
        });
        if let Some(node) = held {
            return Ok(node.name.clone());
        }
        let deps = deps_for(&fold(&nodes), &prodrome, &event)?;
        self.write(&Envelope::Change(mk_change(genesis, deps, event)?))
    }

    /// Attest the writer's prodrome: a `Snapshot` of its tips, chained to the
    /// last snapshot among them. With nothing new since that one, it is the
    /// answer and nothing is written.
    pub fn snapshot(&self) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        let dag = self.dag()?;
        let genesis = self.genesis(&dag)?;
        let tips = dag.tips_in(&dag.key(&genesis));
        let previous = dag.linearise()?.into_iter().rev().find(|name| {
            tips.contains(name) && matches!(dag.get(name), Some(Envelope::Snapshot(_)))
        });
        let tips: Vec<Hash> = tips
            .into_iter()
            .filter(|tip| Some(tip) != previous.as_ref())
            .collect();
        match previous {
            Some(previous) if tips.is_empty() => Ok(previous),
            previous => self.write(&Envelope::Snapshot(mk_snapshot(genesis, tips, previous)?)),
        }
    }

    /// Join two or more tips into one `Woven`, which becomes a tip in their
    /// place: a legacy store's merge, which a store of changes never needs.
    ///
    /// `parents` of `None` means every tip of the legacy prodrome. NO
    /// who/when/why: a merge asserts structure, not a fact about a todo. A
    /// caller that wants the act attributed passes an ordinary `event`, and
    /// the object is a write and a join at once.
    pub fn merge(&self, parents: Option<&[Hash]>, event: Option<E>) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        let on: Vec<Hash> = match parents {
            Some(named) => named.to_vec(),
            None => self.dag()?.tips_in(&None).into_iter().collect(),
        };
        self.require_present(&on)?;
        self.write(&mk_woven(on, event)?)
    }

    /// Take in everything another replica's `digest` rests on, verified.
    ///
    /// THE ONLY WAY A SECOND TIP APPEARS, and there is no placement to do: the
    /// tips are derived, so git's three cases fall out of the objects. A tip we
    /// already contain changes nothing; a tip that contains every tip we hold
    /// names them all beneath it, so they stop being tips (a FAST-FORWARD);
    /// anything else is a second tip. Answers `digest`.
    pub fn adopt(&self, source: &EventStore<E, Pol>, digest: &Hash) -> Result<Hash, ProdromeError> {
        let _locked = self.lock()?;
        self.copy_in(source, digest)?;
        Ok(digest.clone())
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
    fn copy_in(&self, source: &EventStore<E, Pol>, digest: &Hash) -> Result<(), ProdromeError> {
        self.copy_in_from(digest, |name| fs::read(source.object_path(name)).ok())
    }

    /// The walk both adoptions share: from `digest` down through parents,
    /// taking in what this store lacks and REVERIFYING each object on the way.
    /// `bytes_of` is where the replica's objects are read from — a directory,
    /// or a map that arrived over a wire.
    ///
    /// NOTHING IS WRITTEN UNTIL EVERYTHING HAS VERIFIED, and then PARENTS
    /// FIRST. With derived tips an object is part of the store the moment its
    /// file appears, so a child written before its parent — by a crash between
    /// the two, or read by another process in between — would be a store
    /// naming an object it does not hold. Written in this order, every prefix
    /// of the adoption is a store closed under parents.
    fn copy_in_from(
        &self,
        digest: &Hash,
        bytes_of: impl Fn(&Hash) -> Option<Vec<u8>>,
    ) -> Result<(), ProdromeError> {
        /// A depth-first walk, emitting an object once its parents are.
        enum Visit {
            Enter(Hash),
            Leave(Hash, Vec<u8>),
        }
        let mut entered: BTreeSet<Hash> = BTreeSet::new();
        let mut taken: Vec<(Hash, Vec<u8>)> = Vec::new();
        let mut pending = vec![Visit::Enter(digest.clone())];
        while let Some(visit) = pending.pop() {
            let name = match visit {
                Visit::Leave(name, raw) => {
                    taken.push((name, raw));
                    continue;
                }
                Visit::Enter(name) => name,
            };
            if !entered.insert(name.clone()) {
                continue;
            }
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
            let object = decode::<E>(&name, &raw).map_err(|why| why.refusal(&name))?;
            pending.push(Visit::Leave(name, raw));
            let named = parents_of(&object)
                .into_iter()
                .chain(object.genesis().cloned());
            pending.extend(named.map(Visit::Enter));
        }
        let objects = self.objects_dir();
        fs::create_dir_all(&objects).map_err(|e| Self::io(&objects, e))?;
        self.place(taken.iter().map(|(name, raw)| (name, raw.as_slice())))
    }

    /// Objects onto disk under their names, in the order given, DURABLY: each
    /// is [`EventStore::stage`]d and renamed over its name, and the directory
    /// is synced once when all of them are, so a batch — one adoption, one
    /// append — costs one directory sync and not one per object.
    ///
    /// The file is synced BEFORE its rename, which is the order that matters:
    /// a rename can reach the disk before the bytes it names, and a power loss
    /// between the two would leave a zero-length file under a name that
    /// promises content. Synced first, a crash anywhere leaves either no
    /// object or the whole one, and at worst a temp that `verify` reports.
    fn place<'a>(
        &self,
        objects: impl IntoIterator<Item = (&'a Hash, &'a [u8])>,
    ) -> Result<(), ProdromeError> {
        for (digest, bytes) in objects {
            let temp = self.stage(bytes)?;
            fs::rename(&temp, self.object_path(digest)).map_err(|e| Self::io(&temp, e))?;
        }
        sync_dir(&self.objects_dir())
    }

    /// Bytes into a fresh temp file in `objects/`, synced to disk: the first
    /// half of a placement, and all a crash before the rename leaves behind.
    ///
    /// The name is RANDOM and the file is created EXCLUSIVELY, never a fixed
    /// `<name>.tmp`: two writers placing the same object would otherwise
    /// write into one temp together, and one of them rename the other's half.
    /// A temp is not named like an object, so no read takes it for one.
    fn stage(&self, bytes: &[u8]) -> Result<PathBuf, ProdromeError> {
        loop {
            let temp = self.objects_dir().join(temp_name());
            match File::options().write(true).create_new(true).open(&temp) {
                Ok(mut file) => {
                    file.write_all(bytes)
                        .and_then(|()| file.sync_all())
                        .map_err(|e| Self::io(&temp, e))?;
                    return Ok(temp);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(Self::io(&temp, error)),
            }
        }
    }

    /// One object onto disk, content-addressed and idempotent: identical bytes
    /// at the target path are a no-op; DIFFERENT bytes at the same name are a
    /// sha256 collision or a corrupt store, and we stop rather than clobber.
    fn write(&self, object: &Envelope<E>) -> Result<Hash, ProdromeError> {
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
        self.place([(&digest, text.as_bytes())])?;
        Ok(digest)
    }

    /// Load and REVERIFY one object: hash the STORED BYTES against the
    /// filename, then parse. Deliberately NOT a reparse→reprint→rehash — git
    /// hashes bytes and not semantics, so that a future printer change cannot
    /// false-alarm the whole store as tampered.
    pub fn load(&self, digest: &Hash) -> Result<Envelope<E>, ProdromeError> {
        decode(digest, &self.raw(digest)?).map_err(|why| why.refusal(digest))
    }

    /// One object file's bytes, unchecked.
    fn raw(&self, digest: &Hash) -> Result<Vec<u8>, ProdromeError> {
        let path = self.object_path(digest);
        if !path.exists() {
            return Err(ProdromeError::Store(format!(
                "missing object {}",
                digest.as_str()
            )));
        }
        fs::read(&path).map_err(|e| Self::io(&path, e))
    }

    fn quarantine_dir(&self) -> PathBuf {
        self.root.join("quarantine")
    }

    fn quarantined_path(&self, digest: &Hash) -> PathBuf {
        self.quarantine_dir()
            .join(format!("{}.py", digest.as_str()))
    }

    /// SET ASIDE AN OBJECT FILE THAT FAILS ITS HASH: move it from `objects/`
    /// to `quarantine/`, so the store answers again. Answers where it went.
    ///
    /// A file whose bytes are not the hash of its name has no honest reading,
    /// and [`EventStore::tips`] refuses to guess one — so one damaged file
    /// would stop every write. Moving it out makes the store what it holds
    /// without it: the tips derive, an append writes, and `verify` still says
    /// what is missing, as the receipt for the quarantined file. Bringing the
    /// object back from a replica (an `adopt`, or a copy of its file) is the
    /// repair; the move only stops the damage from spreading to every reader.
    ///
    /// ⚠️ What that answer is, stated: the tips of what the store HOLDS. The
    /// set-aside object's parents are named by nothing readable, so they may
    /// be tips again, and the next append writes over the frontiers of what
    /// is held.
    ///
    /// ONLY A FILE THAT FAILS ITS HASH. One that hashes to its name is that
    /// object, whatever it holds — one this reader cannot parse may be a newer
    /// writer's — and setting it aside would be hiding it, so that is refused.
    /// Moved under the store's lock, and both directories synced after.
    pub fn quarantine(&self, digest: &Hash) -> Result<PathBuf, ProdromeError> {
        let _locked = self.lock()?;
        let path = self.object_path(digest);
        if Hash::of_bytes(&self.raw(digest)?) == *digest {
            return Err(ProdromeError::Store(format!(
                "object {} hashes to its name: there is nothing to quarantine",
                digest.as_str()
            )));
        }
        let aside = self.quarantine_dir();
        fs::create_dir_all(&aside).map_err(|e| Self::io(&aside, e))?;
        let to = self.quarantined_path(digest);
        fs::rename(&path, &to).map_err(|e| Self::io(&path, e))?;
        sync_dir(&aside)?;
        sync_dir(&self.objects_dir())?;
        Ok(to)
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

    /// The store's events, in the linearisation's order.
    pub fn events(&self) -> Result<Vec<E>, ProdromeError> {
        Ok(self
            .dag()?
            .nodes()?
            .into_iter()
            .filter_map(|node| node.event)
            .collect())
    }

    /// Full fsck; an empty result means healthy.
    ///
    /// The [`Dag`]'s findings, with what only the files can say beside them:
    /// every entry of `objects/` that is not an object, a `HEAD` or `refs/`
    /// left from before the tips were derived, and a receipt for each file in
    /// `quarantine/`, which stands in for the missing parent it would be.
    pub fn verify(&self) -> Vec<Finding> {
        let Listing {
            objects,
            mut garbage,
        } = match self.listing() {
            Ok(listing) => listing,
            Err(error) => return vec![Finding::Io(error)],
        };
        let mut objects: Vec<Hash> = objects.into_iter().map(|(name, _)| name).collect();
        objects.sort();
        garbage.sort();
        let (mut findings, graph): (Vec<Finding>, Vec<Finding>) = self
            .read(&objects)
            .verify(&self.policy)
            .into_iter()
            .partition(|finding| matches!(finding, Finding::Unread { .. }));
        findings.extend(garbage.into_iter().map(Finding::Garbage));
        findings.extend(
            ["HEAD", "refs/"]
                .into_iter()
                .filter(|file| self.root.join(file.trim_end_matches('/')).exists())
                .map(Finding::Leftover),
        );
        findings.extend(self.quarantined());
        findings.extend(graph.into_iter().filter(|finding| {
            !matches!(finding, Finding::Broken { at, .. } if self.quarantined_path(at).exists())
        }));
        findings
    }

    fn quarantined(&self) -> Vec<Finding> {
        let dir = self.quarantine_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(error) => return vec![Finding::Io(Self::io(&dir, error))],
        };
        let mut files: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        files.into_iter().map(Finding::Quarantined).collect()
    }
}

/// A listed file's metadata, through a symlink to what it names, since
/// that is what a read of it reads.
fn stat(entry: &fs::DirEntry) -> Option<fs::Metadata> {
    match entry.file_type() {
        Ok(kind) if !kind.is_symlink() => entry.metadata().ok(),
        _ => fs::metadata(entry.path()).ok(),
    }
}

/// A temp file's name: sixteen random hex digits and `.tmp`, which is never
/// `<name>.py`. The randomness is the standard library's per-process hash
/// keys, advanced on every draw, so no dependency is taken for it; the
/// exclusive create in [`EventStore::stage`] is what makes a clash harmless.
fn temp_name() -> String {
    format!("{:016x}.tmp", random())
}

fn random() -> u64 {
    RandomState::new().build_hasher().finish()
}

/// Sync a directory, so the renames into it are on disk and not only in the
/// page cache.
fn sync_dir(dir: &Path) -> Result<(), ProdromeError> {
    File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|error| ProdromeError::Io {
            path: dir.display().to_string(),
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Change;
    use crate::event::{mk_completed, mk_created, mk_reopened, mk_sealed, Actor, TodoEvent};
    use crate::literal::Datetime;
    use crate::reference::Todo;
    use proptest::prelude::*;

    /// The store these tests drive. The schema is the todo one, with the
    /// reference payload, because a store has to hold SOME schema; nothing
    /// below reads a field of a record.
    type Store = EventStore<TodoEvent<Todo>>;

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    /// The reference policy, with one name on the roster — every store below
    /// reads under it, and only the dating test can tell.
    fn roster() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    fn tips(store: &Store) -> BTreeSet<Hash> {
        store.tips().expect("the tips derive")
    }

    fn findings(store: &Store) -> Vec<String> {
        store.verify().iter().map(ToString::to_string).collect()
    }

    fn order(store: &Store) -> Result<Vec<Hash>, ProdromeError> {
        store.dag()?.linearise()
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

    /// The pre-Draft-A append, for the tests of a legacy store: seal on
    /// every tip.
    fn seal(store: &Store, event: TodoEvent<Todo>) -> Hash {
        let on: Vec<Hash> = tips(store).into_iter().collect();
        let object = if on.len() > 1 {
            mk_woven(on, Some(event)).expect("distinct tips")
        } else {
            mk_sealed(on.into_iter().next(), event)
        };
        store.write(&object).expect("writes")
    }

    fn change(store: &Store, name: &Hash) -> Change<TodoEvent<Todo>> {
        match store.load(name).expect("loads") {
            Envelope::Change(change) => change,
            other => panic!("{} is not a change", other.name()),
        }
    }

    /// An append writes a `Change` over the frontiers its event supersedes,
    /// and appending the same event again writes nothing.
    #[test]
    fn an_append_writes_a_change_over_what_it_supersedes() {
        let store = Store::new(scratch("chain"), roster());
        let created = mk_created("alpha", at(1), "bassel", "", "").expect("valid");
        assert!(store.append(created.clone()).is_err(), "no genesis yet");
        let genesis = store.init("chain").expect("begins");
        assert!(store.init("again").is_err(), "a store begins once");
        let first = store.append(created).expect("appends");
        let done = mk_completed("alpha", at(2), "bassel", "").expect("valid");
        let second = store.append(done.clone()).expect("appends");
        let third = store
            .append(mk_reopened("alpha", at(3), "bassel", "").expect("valid"))
            .expect("appends");
        assert_eq!(change(&store, &first).genesis, genesis);
        assert!(
            change(&store, &second).deps.is_empty(),
            "Created writes no register"
        );
        assert_eq!(change(&store, &third).deps, std::slice::from_ref(&second));
        assert_eq!(
            tips(&store),
            [genesis, first, third.clone()].into_iter().collect()
        );
        assert_eq!(
            fs::read_dir(store.root())
                .expect("the store is a directory")
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name != ".lock")
                .collect::<Vec<_>>(),
            vec!["objects".to_owned()],
            "the objects are the whole store: no file names a head"
        );
        assert_eq!(findings(&store), Vec::<String>::new());
        assert_eq!(store.events().expect("reads").len(), 3);
        assert_eq!(
            store.ancestors(&third).expect("walks"),
            [second.clone()].into_iter().collect()
        );
        let held = order(&store).expect("reads");
        assert_eq!(store.append(done).expect("replays"), second);
        assert_eq!(
            order(&store).expect("reads"),
            held,
            "a replay writes nothing"
        );
        let _ = fs::remove_dir_all(store.root());
    }

    #[test]
    fn adopt_makes_a_second_head_and_merge_settles_it() {
        let here = Store::new(scratch("here"), roster());
        let there = Store::new(scratch("there"), roster());
        let shared = seal(
            &here,
            mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
        );
        there
            .adopt(&here, &shared)
            .expect("fast-forwards into an empty store");
        assert_eq!(tips(&there), [shared.clone()].into_iter().collect());

        let mine = seal(
            &here,
            mk_created("beta", at(2), "bassel", "mine", "").expect("valid"),
        );
        let theirs = seal(
            &there,
            mk_created("beta", at(2), "bassel", "theirs", "").expect("valid"),
        );
        here.adopt(&there, &theirs).expect("adopts");
        assert_eq!(
            tips(&here),
            [mine.clone(), theirs.clone()].into_iter().collect()
        );
        assert!(here.concurrent(&mine, &theirs).expect("both present"));
        assert_eq!(findings(&here), Vec::<String>::new());

        let merged = here.merge(None, None).expect("merges");
        assert_eq!(tips(&here), [merged.clone()].into_iter().collect());
        assert_eq!(findings(&here), Vec::<String>::new());
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

    /// THE NEXT WRITE SETTLES A UNION. Two replicas' files put in one
    /// directory, which is what a `git merge` of two clones does, hold a
    /// conflict; the next write to that register depends on both sides, and
    /// nothing else is written.
    #[test]
    fn the_next_append_settles_a_union() {
        let here = Store::new(scratch("weave-here"), roster());
        let there = Store::new(scratch("weave-there"), roster());
        let genesis = here.init("union").expect("begins");
        there.adopt(&here, &genesis).expect("adopts the genesis");
        let mine = here
            .append(mk_completed("alpha", at(2), "bassel", "mine").expect("valid"))
            .expect("appends");
        let theirs = there
            .append(mk_completed("alpha", at(2), "bassel", "theirs").expect("valid"))
            .expect("appends");
        for entry in fs::read_dir(there.objects_dir()).expect("lists") {
            let path = entry.expect("an entry").path();
            let file = path.file_name().expect("a file name");
            fs::copy(&path, here.objects_dir().join(file)).expect("copies");
        }
        let settled = here
            .append(mk_reopened("alpha", at(3), "bassel", "both").expect("valid"))
            .expect("appends");
        let mut both = vec![mine, theirs];
        both.sort();
        assert_eq!(change(&here, &settled).deps, both);
        assert_eq!(tips(&here), [genesis, settled].into_iter().collect());
        assert_eq!(findings(&here), Vec::<String>::new());
        let _ = fs::remove_dir_all(here.root());
        let _ = fs::remove_dir_all(there.root());
    }

    /// NO STORED BYTE MOVES, AND THE OLD FILES ARE SAID TO GO. A store laid out
    /// the way 0.8 wrote it — HEAD naming the tip, `refs/` naming both heads
    /// while there were two — derives exactly the heads those files named, and
    /// `verify` names each file as a leftover rather than deleting it.
    #[test]
    fn an_old_store_derives_what_its_head_named_and_is_told_to_drop_it() {
        let store = Store::new(scratch("legacy"), roster());
        let first = seal(
            &store,
            mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
        );
        let second = seal(
            &store,
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
        );
        let aside = store
            .write(&mk_sealed(
                Some(first),
                mk_created("beta", at(2), "bassel", "", "").expect("valid"),
            ))
            .expect("writes");
        fs::write(store.root().join("HEAD"), second.as_str()).expect("writes HEAD");
        let refs = store.root().join("refs");
        fs::create_dir_all(&refs).expect("creates refs/");
        for head in [&second, &aside] {
            fs::write(refs.join(head.as_str()), head.as_str()).expect("writes a ref");
        }
        assert_eq!(tips(&store), [second, aside].into_iter().collect());
        assert_eq!(
            findings(&store),
            vec![
                "HEAD is left from before tips were derived (SPEC §3): nothing reads it, delete it",
                "refs/ is left from before tips were derived (SPEC §3): nothing reads it, delete it",
            ]
        );
        fs::remove_file(store.root().join("HEAD")).expect("removes HEAD");
        fs::remove_dir_all(&refs).expect("removes refs/");
        assert_eq!(findings(&store), Vec::<String>::new());
        let _ = fs::remove_dir_all(store.root());
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
            seal(store, shared.clone());
        }
        // Each side writes its own second object, so the two histories diverge.
        for store in [&by_dir, &by_bytes] {
            seal(
                store,
                mk_created("beta", at(2), "bassel", "mine", "").expect("valid"),
            );
        }
        let theirs = seal(
            &there,
            mk_created("beta", at(2), "bassel", "theirs", "").expect("valid"),
        );

        by_dir.adopt(&there, &theirs).expect("adopts a directory");
        // The same objects, as the wire carries them: a name and the canonical
        // print that name is the hash of.
        let over_the_wire: BTreeMap<Hash, String> = there
            .dag()
            .expect("reads")
            .objects()
            .iter()
            .map(|(name, object)| (name.clone(), crate::event::canonical_envelope(object)))
            .collect();
        by_bytes
            .adopt_objects(&over_the_wire, &theirs)
            .expect("adopts bytes");

        assert_eq!(tips(&by_bytes), tips(&by_dir));
        assert_eq!(by_bytes.dag().expect("reads"), by_dir.dag().expect("reads"));
        assert_eq!(findings(&by_bytes), Vec::<String>::new());

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
        // And a refused adoption WROTE NOTHING: with derived tips an object is
        // in the store the moment its file is, so the tip whose parent never
        // arrived must not have landed on its own.
        assert_eq!(tips(&liar), BTreeSet::new());
        assert_eq!(tips(&missing), BTreeSet::new());

        for store in [&by_dir, &by_bytes, &there, &liar, &missing] {
            let _ = fs::remove_dir_all(store.root());
        }
    }

    #[test]
    fn a_contained_tip_changes_nothing_and_a_containing_one_fast_forwards() {
        let here = Store::new(scratch("ff-here"), roster());
        let there = Store::new(scratch("ff-there"), roster());
        let first = seal(
            &here,
            mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
        );
        there.adopt(&here, &first).expect("adopts");
        let second = seal(
            &there,
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
        );
        here.adopt(&there, &second).expect("fast-forwards");
        assert_eq!(tips(&here), [second.clone()].into_iter().collect());
        here.adopt(&there, &first).expect("a contained tip");
        assert_eq!(tips(&here), [second].into_iter().collect());
        let _ = fs::remove_dir_all(here.root());
        let _ = fs::remove_dir_all(there.root());
    }

    #[test]
    fn a_tampered_object_is_caught_on_read_and_by_verify() {
        let store = Store::new(scratch("tamper"), roster());
        let digest = seal(
            &store,
            mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
        );
        seal(
            &store,
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
        );
        let path = store.object_path(&digest);
        let text = fs::read_to_string(&path).expect("reads");
        fs::write(&path, text.replace("alpha", "omega")).expect("writes");
        assert!(store.load(&digest).is_err());
        // The reads REFUSE: a store holding a file that does not verify has no
        // honest tips, since that file could name any object as its parent.
        let fresh = Store::new(store.root(), roster());
        assert!(fresh.tips().is_err());
        assert!(store.dag().is_err());
        // A FILE CHANGED UNDER A HELD NAME IS READ AGAIN, so the handle that
        // verified the object before it was tampered with refuses too.
        assert!(store.tips().is_err());
        // `verify` REPORTS, twice over, and both are true: the file no longer
        // hashes to its name, and the object resting on it rests on nothing.
        assert_eq!(
            findings(&store),
            vec![
                format!(
                    "object {} does not hash to its filename (tampered/corrupt)",
                    digest.as_str()
                ),
                format!(
                    "chain broke at {digest}: object {digest} hashes to {} — tampered or \
                     corrupt: `prodrome quarantine {digest}` (`EventStore::quarantine`) sets it \
                     aside so the store answers again",
                    Hash::of_bytes(&fs::read(&path).expect("reads")).as_str(),
                    digest = digest.as_str()
                ),
            ]
        );
        let _ = fs::remove_dir_all(store.root());
    }

    #[test]
    fn an_unconfirmed_event_dated_behind_its_predecessor_is_a_finding() {
        let store = Store::new(scratch("dating"), roster());
        seal(
            &store,
            mk_created("alpha", at(10), "bassel", "", "").expect("valid"),
        );
        let late = seal(
            &store,
            mk_completed("alpha", at(2), "triage", "").expect("valid"),
        );
        assert_eq!(
            findings(&store),
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
        seal(
            &trusted,
            mk_created("alpha", at(10), "bassel", "", "").expect("valid"),
        );
        seal(
            &trusted,
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
        );
        assert_eq!(findings(&trusted), Vec::<String>::new());
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
        seal(
            &store,
            mk_created("alpha", at(10), "bassel", "", "").expect("valid"),
        );
        let record = seal(
            &store,
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
        );
        let events = store.events().expect("reads");
        let late = events.last().expect("the record");
        assert!(
            roster().standing(late).binds(),
            "a content record binds whoever wrote it"
        );
        assert_eq!(
            findings(&store),
            vec![format!(
                "untrusted event {} is dated {}, behind its predecessor ({})",
                record.as_str(),
                at(2).isoformat(),
                at(10).isoformat()
            )]
        );
        let _ = fs::remove_dir_all(store.root());
    }

    /// THE MEMORY FOLLOWS THE LISTING. What the directory no longer lists
    /// the memory forgets, so a host that deletes an object — the one removal
    /// a host does, a rejected proposal nothing rests on — sees its parent
    /// become a tip again through the same handle.
    #[test]
    fn the_memory_follows_the_listing() {
        let store = Store::new(scratch("index"), roster());
        let first = seal(
            &store,
            mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
        );
        let second = seal(
            &store,
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
        );
        assert_eq!(tips(&store), [second.clone()].into_iter().collect());
        fs::remove_file(store.object_path(&second)).expect("removes");
        assert_eq!(tips(&store), [first].into_iter().collect());
        let _ = fs::remove_dir_all(store.root());
    }

    /// THERE ARE NO ORPHANS. An object nothing names is not unreachable, it is
    /// a TIP: here a second genesis, what adopting an unrelated replica makes.
    /// It is folded like any other, and the next append joins it.
    #[test]
    fn an_object_nothing_names_is_a_tip() {
        let store = Store::new(scratch("orphan"), roster());
        let first = seal(
            &store,
            mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
        );
        let other = store
            .write(&mk_sealed(
                None,
                mk_created("beta", at(1), "bassel", "", "").expect("valid"),
            ))
            .expect("writes");
        assert_eq!(tips(&store), [first, other].into_iter().collect());
        assert_eq!(findings(&store), Vec::<String>::new());
        assert_eq!(store.events().expect("reads").len(), 2);
        let _ = fs::remove_dir_all(store.root());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        /// A CRASH BETWEEN WRITE AND RENAME LEAVES NO OBJECT, only a temp that
        /// `verify` reports. A chain of `crash` events is written, and the
        /// next object's placement stops after its temp is written and synced
        /// and before the rename — the crash. The store is then exactly the
        /// chain: the tips derive from it, the object the crash interrupted is
        /// not there under any name, and `verify`'s one finding is the temp,
        /// by name.
        #[test]
        fn a_crash_before_the_rename_leaves_only_a_temp(
            crash in 0usize..5
        ) {
            let store = Store::new(scratch("crash"), roster());
            let mut written: Vec<Hash> = Vec::new();
            for day in 0..crash {
                let day = u32::try_from(day).expect("a few days") + 1;
                written.push(
                    seal(&store, mk_reopened("alpha", at(day), "bassel", "").expect("valid")),
                );
            }
            let day = u32::try_from(crash).expect("a few days") + 1;
            let next: Envelope<TodoEvent<Todo>> = mk_sealed(
                written.last().cloned(),
                mk_reopened("alpha", at(day), "bassel", "").expect("valid"),
            );
            fs::create_dir_all(store.objects_dir()).expect("creates objects/");
            let temp = store
                .stage(crate::event::canonical_envelope(&next).as_bytes())
                .expect("stages");
            let file = temp.file_name().expect("a name").to_string_lossy().into_owned();

            let fresh = Store::new(store.root(), roster());
            let tips = fresh.tips();
            let landed = store.object_path(&seal_hash(&next)).exists();
            let found = findings(&store);
            let _ = fs::remove_dir_all(store.root());
            prop_assert_eq!(
                tips.expect("the tips derive"),
                written.last().cloned().into_iter().collect::<BTreeSet<_>>()
            );
            prop_assert!(!landed, "the interrupted object is not in the store");
            prop_assert_eq!(
                found,
                vec![format!(
                    "objects/{file} is not an object: a temp an interrupted write left, or a \
                     stray (SPEC §3); delete it"
                )]
            );
        }
    }

    /// An entry of `objects/` that could be taken for an object and is not
    /// one: a `.tmp`, a name one character short or in capitals, a `.py` of
    /// anything, a file with no suffix. Drawn and then filtered, so a draw
    /// that happens to be a well-formed object name is not a stray.
    fn a_stray() -> impl Strategy<Value = String> {
        prop_oneof![
            "[0-9a-f]{64}\\.tmp",
            "[0-9a-f]{63}\\.py",
            "[0-9A-F]{64}\\.py",
            "[a-z0-9_.-]{1,12}\\.py",
            "[a-zA-Z0-9_-]{1,70}",
        ]
        .prop_filter("an object name is not a stray", |file| {
            file.strip_suffix(".py")
                .map(Hash::new)
                .is_none_or(|name| name.is_err())
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// VERIFY NAMES EVERY STRAY, and the reads pass over them. Whatever
        /// lands in `objects/` beside a healthy store's objects, `verify`
        /// reports each such entry once, by name, in name order, and nothing
        /// else; the tips and the objects read exactly as they did.
        #[test]
        fn verify_names_every_stray(strays in prop::collection::btree_set(a_stray(), 1..6)) {
            let store = Store::new(scratch("strays"), roster());
            seal(&store, mk_created("alpha", at(1), "bassel", "", "").expect("valid"));
            let tip = seal(&store, mk_completed("alpha", at(2), "bassel", "").expect("valid"));
            for file in &strays {
                fs::write(store.objects_dir().join(file), "stray").expect("writes a stray");
            }
            let found = findings(&store);
            let expected: Vec<String> = strays
                .iter()
                .map(|file| {
                    format!(
                        "objects/{file} is not an object: a temp an interrupted write left, or a \
                         stray (SPEC §3); delete it"
                    )
                })
                .collect();
            let fresh = Store::new(store.root(), roster());
            let tips = fresh.tips();
            let read = fresh.dag().map(|dag| dag.objects().len());
            let _ = fs::remove_dir_all(store.root());
            prop_assert_eq!(found, expected);
            prop_assert_eq!(tips.expect("the tips derive"), [tip].into_iter().collect::<BTreeSet<_>>());
            prop_assert_eq!(read.expect("the store reads"), 2);
        }
    }

    /// ONE BAD OBJECT DOES NOT STOP EVERYTHING. A chain of three whose middle
    /// file is damaged: a fresh handle's `tips` refuses, naming the object and
    /// the command that sets it aside, and so does every append. Once it is
    /// quarantined the store answers again — the tips derive, the damaged
    /// object's parent among them, and an append writes a change over what is
    /// held — and after that append as before it, `verify` is clean but for the receipt,
    /// which stands in for the "chain broke" the tip resting on it would be.
    /// A healthy object is not quarantined: that would be hiding it.
    #[test]
    fn after_quarantine_the_store_answers_and_verify_holds_the_receipt() {
        let store = Store::new(scratch("quarantine"), roster());
        let first = seal(
            &store,
            mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
        );
        let middle = seal(
            &store,
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
        );
        let last = seal(
            &store,
            mk_reopened("alpha", at(3), "bassel", "").expect("valid"),
        );
        let path = store.object_path(&middle);
        let text = fs::read_to_string(&path).expect("reads");
        fs::write(&path, text.replace("alpha", "omega")).expect("damages");

        let fresh = Store::new(store.root(), roster());
        let refusal = fresh.tips().expect_err("no honest tips").to_string();
        assert!(refusal.contains(middle.as_str()), "{refusal}");
        assert!(
            refusal.contains(&format!("`prodrome quarantine {}`", middle.as_str())),
            "{refusal}"
        );
        assert!(fresh
            .append(mk_completed("alpha", at(4), "bassel", "").expect("valid"))
            .is_err());
        assert!(fresh.quarantine(&first).is_err(), "a healthy object stays");

        let aside = fresh.quarantine(&middle).expect("sets it aside");
        assert_eq!(
            aside,
            store
                .root()
                .join("quarantine")
                .join(format!("{}.py", middle.as_str()))
        );
        assert!(!path.exists());
        // Nothing the store can read names `first` any more, so it is a tip
        // beside `last`: the tips of what the store holds, exactly.
        assert_eq!(
            tips(&fresh),
            [first.clone(), last.clone()].into_iter().collect()
        );
        let receipt = format!(
            "quarantine/{}.py failed its hash and was set aside (SPEC §3): restore the object \
             from a replica, then delete this file",
            middle.as_str()
        );
        assert_eq!(findings(&fresh), vec![receipt.clone()]);
        let next = fresh
            .append(mk_completed("alpha", at(4), "bassel", "").expect("valid"))
            .expect("the store writes again");
        assert_eq!(change(&fresh, &next).genesis, first);
        assert_eq!(change(&fresh, &next).deps, [last]);
        assert_eq!(tips(&fresh), [first, next].into_iter().collect());
        assert_eq!(findings(&fresh), vec![receipt]);

        // The repair: the object back from a replica, the receipt deleted.
        fs::write(&path, text).expect("restores");
        fs::remove_dir_all(store.root().join("quarantine")).expect("deletes the receipt");
        assert_eq!(findings(&fresh), Vec::<String>::new());
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
        store.init("locked").expect("begins");
        assert_eq!(store.lock_file(), store.root().join(".lock"));
        assert!(store.lock_file().exists(), "the write created the lock");

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
            .append(mk_completed("alpha", at(2), "bassel", "").expect("valid"))
            .expect("appends once the other writer let go");
        assert_eq!(findings(&store), Vec::<String>::new());
        let _ = fs::remove_dir_all(store.root());
    }
}
