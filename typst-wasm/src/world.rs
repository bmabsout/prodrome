//! The world a compilation sees: the files the caller handed over, the fonts
//! the page loaded, and nothing else.
//!
//! NO FILESYSTEM, NO NETWORK, NO CLOCK. A path the caller did not supply is
//! `NotFound`; `@preview` packages are not downloaded (a package is found only
//! if its files were passed in, under its own spec); `datetime.today()` is
//! `none`, because the moment a page is read at is data — it arrives in the
//! JSON the document is given, as it does everywhere else in this repository.
//!
//! ONE WORLD MAY OUTLIVE MANY COMPILATIONS, as typst-cli's watch mode keeps
//! its own: a file's id is interned from its key, so it is the same id on
//! every compilation; a file set again with the same text keeps the very
//! `Bytes` and `Source` it had, so their hashes are not recomputed and every
//! memoised result that read them still hits; and a file set with new text
//! that has been parsed is `Source::replace`d, so only the edited part is
//! reparsed.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, OnceLock, PoisonError, RwLock};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::package::PackageSpec;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Library, LibraryExt, World};

/// The standard library, with HTML export switched on — the same thing
/// `typst compile --features html` builds. Built once per module instance:
/// it holds no state a compilation could leave behind.
static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| {
    LazyHash::new(Library::builder().with_features([Feature::Html].into_iter().collect()).build())
});

/// The fonts added so far, and the book indexing them. Replaced whole on each
/// addition, so a world holding the old set keeps a consistent one.
struct Fonts {
    fonts: Vec<Font>,
    book: LazyHash<FontBook>,
}

static FONTS: LazyLock<RwLock<Arc<Fonts>>> = LazyLock::new(|| {
    RwLock::new(Arc::new(Fonts { fonts: Vec::new(), book: LazyHash::new(FontBook::new()) }))
});

fn fonts() -> Arc<Fonts> {
    FONTS.read().map_or_else(|poisoned| poisoned.into_inner().clone(), |set| set.clone())
}

/// Add every face in one TTF, OTF or collection file; answers how many faces
/// it held, so zero means the bytes were not a font.
pub fn add_font(data: Vec<u8>) -> usize {
    let faces: Vec<Font> = Font::iter(Bytes::new(data)).collect();
    let added = faces.len();
    if added > 0 {
        let mut set = FONTS.write().unwrap_or_else(PoisonError::into_inner);
        let mut all = set.fonts.clone();
        all.extend(faces);
        let book = LazyHash::new(FontBook::from_fonts(&all));
        *set = Arc::new(Fonts { fonts: all, book });
    }
    added
}

/// A file's key as the caller spells it: `/path` in the project, or
/// `@namespace/name:version/path` inside a package.
pub fn key_of(id: FileId) -> String {
    let path = id.get();
    let vpath = path.vpath().get_with_slash();
    match path.root() {
        VirtualRoot::Package(spec) => format!("{spec}{vpath}"),
        VirtualRoot::Project => vpath.to_owned(),
    }
}

/// The id a caller's key names, or why it names none. Interned: the same key
/// is the same id for the life of the module.
pub fn id_of(key: &str) -> Result<FileId, String> {
    let (root, path) = if key.starts_with('@') {
        let slash = key
            .find(':')
            .and_then(|colon| key[colon..].find('/').map(|at| colon + at))
            .ok_or_else(|| format!("{key:?}: a package file is @namespace/name:version/path"))?;
        let spec = PackageSpec::from_str(&key[..slash]).map_err(|e| format!("{key:?}: {e}"))?;
        (VirtualRoot::Package(spec), &key[slash..])
    } else {
        (VirtualRoot::Project, key)
    };
    let vpath = VirtualPath::new(path).map_err(|e| format!("{key:?}: {e:?}"))?;
    Ok(RootedPath::new(root, vpath).intern())
}

/// One file: its text as bytes, and its parse once a compilation asked for
/// one — a JSON state is read as bytes and never parsed as Typst.
struct Slot {
    bytes: Bytes,
    source: OnceLock<Source>,
}

impl Slot {
    fn text(&self) -> &str {
        // Every slot is made from a `&str` by `Sandbox::set`, so this is
        // `Ok` without a check: `Bytes::from_string` skips the validation.
        self.bytes.as_str().unwrap_or_default()
    }
}

/// A world: files by id, and which of them is the one compiled.
pub struct Sandbox {
    main: Option<FileId>,
    slots: HashMap<FileId, Slot>,
    fonts: Arc<Fonts>,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        Sandbox { main: None, slots: HashMap::new(), fonts: fonts() }
    }

    /// Set or replace the file at `key` (see [`id_of`]). The same text again
    /// changes nothing; new text over a parsed file is an incremental edit.
    pub fn set(&mut self, key: &str, text: &str) -> Result<FileId, String> {
        let id = id_of(key)?;
        match self.slots.get_mut(&id) {
            Some(slot) if slot.text() == text => {}
            Some(slot) => {
                slot.bytes = Bytes::from_string(text.to_owned());
                if let Some(source) = slot.source.get_mut() {
                    source.replace(text);
                }
            }
            None => {
                let slot =
                    Slot { bytes: Bytes::from_string(text.to_owned()), source: OnceLock::new() };
                self.slots.insert(id, slot);
            }
        }
        Ok(id)
    }

    /// Forget the file at `key`; answers whether there was one.
    pub fn remove(&mut self, key: &str) -> bool {
        id_of(key).is_ok_and(|id| self.slots.remove(&id).is_some())
    }

    /// Make the file at `key` the one compiled, and take up the fonts added
    /// since the last compilation.
    pub fn prepare(&mut self, key: &str) -> Result<(), String> {
        let id = id_of(key)?;
        if !self.slots.contains_key(&id) {
            return Err(format!("{key} is not set, so it cannot be compiled"));
        }
        self.main = Some(id);
        self.fonts = fonts();
        Ok(())
    }

    /// The text behind an id, for turning a diagnostic's bytes into the
    /// caller's units.
    pub fn text(&self, id: FileId) -> Option<&str> {
        self.slots.get(&id).map(Slot::text)
    }

    fn missing(id: FileId) -> FileError {
        FileError::Other(Some(format!("{} was not handed to this compilation", key_of(id)).into()))
    }
}

impl World for Sandbox {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.fonts.book
    }

    fn main(&self) -> FileId {
        // `prepare` runs before every compilation; before the first, a
        // detached id names no file, and compiling it is an error, not a
        // panic.
        self.main.unwrap_or_else(|| Source::detached("").id())
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        let slot = self.slots.get(&id).ok_or_else(|| Self::missing(id))?;
        Ok(slot.source.get_or_init(|| Source::new(id, slot.text().to_owned())).clone())
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.slots.get(&id).map(|slot| slot.bytes.clone()).ok_or_else(|| Self::missing(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(feature = "ide")]
impl typst_ide::IdeWorld for Sandbox {
    fn upcast(&self) -> &dyn World {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for key in ["/main.typ", "/data.json", "@local/prodrome-typst:0.1.0/lib.typ"] {
            assert_eq!(key_of(id_of(key).expect("a key")), key);
        }
    }

    #[test]
    fn a_package_key_needs_a_path() {
        assert!(id_of("@local/prodrome-typst:0.1.0").is_err());
    }

    #[test]
    fn the_same_text_keeps_the_same_file() {
        let mut world = Sandbox::new();
        let id = world.set("/a.typ", "= A").expect("a key");
        let (bytes, source) = (world.file(id).expect("set"), world.source(id).expect("set"));
        world.set("/a.typ", "= A").expect("a key");
        assert_eq!(world.file(id).expect("set"), bytes);
        assert!(std::ptr::eq(world.source(id).expect("set").root(), source.root()));
    }

    #[test]
    fn new_text_is_an_edit_of_the_parse() {
        let mut world = Sandbox::new();
        let id = world.set("/a.typ", "= A\n\nOne *two* three.").expect("a key");
        let before = world.source(id).expect("set");
        world.set("/a.typ", "= A\n\nOne *two* four.").expect("a key");
        let after = world.source(id).expect("set");
        assert_eq!(after.text(), "= A\n\nOne *two* four.");
        assert_eq!(after.id(), before.id());
        assert!(world.remove("/a.typ"));
        assert!(world.source(id).is_err());
    }
}
