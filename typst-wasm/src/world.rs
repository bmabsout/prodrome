//! The world a compilation sees: one main source, the files the caller handed
//! over, the fonts the page loaded, and nothing else.
//!
//! NO FILESYSTEM, NO NETWORK, NO CLOCK. A path the caller did not supply is
//! `NotFound`; `@preview` packages are not downloaded (a package is found only
//! if its files were passed in, under its own spec); `datetime.today()` is
//! `none`, because the moment a page is read at is data — it arrives in the
//! JSON the document is given, as it does everywhere else in this repository.

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::{Arc, LazyLock, RwLock};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::package::PackageSpec;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Library, LibraryExt, World};
use typst_ide::IdeWorld;

/// The main file's path. Every diagnostic in it is reported against the
/// `source` the caller sent, whatever the caller calls it.
pub const MAIN: &str = "/main.typ";

/// The standard library, with HTML export switched on — the same thing
/// `typst compile --features html` builds. Built once per module instance:
/// it holds no state a compilation could leave behind.
static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| {
    LazyHash::new(
        Library::builder()
            .with_features([Feature::Html].into_iter().collect())
            .build(),
    )
});

/// The fonts added so far, and the book indexing them. Replaced whole on each
/// addition, so a world holding the old set keeps a consistent one.
pub struct Fonts {
    fonts: Vec<Font>,
    book: LazyHash<FontBook>,
}

static FONTS: LazyLock<RwLock<Arc<Fonts>>> = LazyLock::new(|| {
    RwLock::new(Arc::new(Fonts {
        fonts: Vec::new(),
        book: LazyHash::new(FontBook::new()),
    }))
});

fn fonts() -> Arc<Fonts> {
    FONTS.read().map(|set| set.clone()).unwrap_or_else(|poisoned| poisoned.into_inner().clone())
}

/// Add every face in one TTF, OTF or collection file; answers how many faces
/// it held, so zero means the bytes were not a font.
pub fn add_font(data: Vec<u8>) -> usize {
    let faces: Vec<Font> = Font::iter(Bytes::new(data)).collect();
    let added = faces.len();
    if added > 0 {
        let mut set = FONTS.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut all = set.fonts.clone();
        all.extend(faces);
        let book = LazyHash::new(FontBook::from_fonts(&all));
        *set = Arc::new(Fonts { fonts: all, book });
    }
    added
}

/// A file's key as the caller spells it: `/path` in the project, or
/// `@namespace/name:version/path` inside a package.
fn key_of(id: FileId) -> String {
    let path = id.get();
    let vpath = path.vpath().get_with_slash();
    match path.root() {
        VirtualRoot::Package(spec) => format!("{spec}{vpath}"),
        VirtualRoot::Project => vpath.to_owned(),
    }
}

/// The id a caller's key names, or why it names none.
pub fn id_of(key: &str) -> Result<FileId, String> {
    let (root, path) = if key.starts_with('@') {
        let slash = key
            .find(":")
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

/// One compilation's world.
pub struct Sandbox {
    main: Source,
    files: HashMap<FileId, String>,
    fonts: Arc<Fonts>,
}

impl Sandbox {
    /// `files` maps a key (see [`id_of`]) to that file's text. A key that does
    /// not parse is refused here, before anything is compiled.
    pub fn new(source: &str, files: BTreeMap<String, String>) -> Result<Sandbox, String> {
        let main = id_of(MAIN)?;
        let files = files
            .into_iter()
            .map(|(key, text)| id_of(&key).map(|id| (id, text)))
            .collect::<Result<_, _>>()?;
        Ok(Sandbox {
            main: Source::new(main, source.to_owned()),
            files,
            fonts: fonts(),
        })
    }

    pub fn main_source(&self) -> &Source {
        &self.main
    }

    /// The text behind an id, for turning a diagnostic's bytes into the
    /// caller's units.
    pub fn text(&self, id: FileId) -> Option<&str> {
        if id == self.main.id() {
            Some(self.main.text())
        } else {
            self.files.get(&id).map(String::as_str)
        }
    }

    /// The caller's key for an id.
    pub fn key(&self, id: FileId) -> String {
        if id == self.main.id() {
            MAIN.to_owned()
        } else {
            key_of(id)
        }
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
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        self.files
            .get(&id)
            .map(|text| Source::new(id, text.clone()))
            .ok_or_else(|| Self::missing(id))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() {
            return Ok(Bytes::from_string(self.main.text().to_owned()));
        }
        self.files
            .get(&id)
            .map(|text| Bytes::from_string(text.clone()))
            .ok_or_else(|| Self::missing(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

impl IdeWorld for Sandbox {
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
}
