//! Paths: the free monoid on segments (design §6.3).

use std::fmt;

use crate::literal::ProdromeError;

/// One step of a path: an entity's key, or a register's name. Not empty,
/// and no `/`, so a path prints as its segments joined by one and parses
/// back to them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Segment(String);

impl Segment {
    /// # Errors
    ///
    /// An empty segment, or one holding a `/`.
    pub fn new(text: impl Into<String>) -> Result<Segment, ProdromeError> {
        let text = text.into();
        if text.is_empty() || text.contains('/') {
            Err(ProdromeError::invalid(format!(
                "a path segment is not empty and holds no '/', got {text:?}"
            )))
        } else {
            Ok(Segment(text))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A KEY IN A NEST: where an object sits, the path of the register that
/// held its history and then its own entity's key. Paths under
/// concatenation are the FREE MONOID on segments: [`Path::root`] is the
/// unit and [`Path::then`] is associative, so `a/(b/c)` and `(a/b)/c` are
/// one path, which is why flattening three levels in either order gives one
/// history.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(Vec<Segment>);

impl Path {
    /// The empty path, the monoid's unit: where a history's own objects sit
    /// before it is nested in anything.
    #[must_use]
    pub fn root() -> Path {
        Path(Vec::new())
    }

    pub fn of(segments: impl IntoIterator<Item = Segment>) -> Path {
        Path(segments.into_iter().collect())
    }

    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.0
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// `self` then `rest`: the monoid's operation.
    #[must_use]
    pub fn then(&self, rest: &Path) -> Path {
        Path(self.0.iter().chain(&rest.0).cloned().collect())
    }

    /// What follows `prefix` in this path, where this path begins with it.
    #[must_use]
    pub fn strip(&self, prefix: &Path) -> Option<Path> {
        self.0
            .strip_prefix(prefix.0.as_slice())
            .map(|rest| Path(rest.to_vec()))
    }
}

impl From<Segment> for Path {
    fn from(segment: Segment) -> Path {
        Path(vec![segment])
    }
}

/// Its segments joined by `/`; the root prints as nothing.
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, segment) in self.0.iter().enumerate() {
            if n > 0 {
                f.write_str("/")?;
            }
            f.write_str(&segment.0)?;
        }
        Ok(())
    }
}
