//! Loading the content directory (D27): the only place content files are
//! read. Parsing and validation live in `sim::content`.

use std::path::{Path, PathBuf};

use sim::Content;

/// Where the content directory is looked for, relative to the working
/// directory, when `--content` is not given.
pub const DEFAULT_CONTENT_DIRS: [&str; 3] = ["content", "../content", "../../content"];

/// Why content could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// No content directory was found.
    #[error("no content directory found (tried {0}); use --content <DIR>")]
    NotFound(String),
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file.
        path: PathBuf,
        /// The error.
        #[source]
        source: std::io::Error,
    },
    /// A file is invalid.
    #[error("invalid content in {dir}: {source}")]
    Invalid {
        /// The directory.
        dir: PathBuf,
        /// What is wrong.
        #[source]
        source: Box<sim::ContentError>,
    },
}

/// The content directory: the one given, or the first default that exists.
pub fn find_dir(given: Option<&Path>) -> Result<PathBuf, LoadError> {
    if let Some(dir) = given {
        return Ok(dir.to_path_buf());
    }
    DEFAULT_CONTENT_DIRS
        .iter()
        .map(PathBuf::from)
        .find(|dir| dir.join("materials.ron").is_file())
        .ok_or_else(|| LoadError::NotFound(DEFAULT_CONTENT_DIRS.join(", ")))
}

/// Reads and validates the content files of a directory.
pub fn load(dir: &Path) -> Result<Content, LoadError> {
    let read = |name: &str| {
        let path = dir.join(name);
        std::fs::read_to_string(&path).map_err(|source| LoadError::Read { path, source })
    };
    let materials = read("materials.ron")?;
    let spawns = read("spawns.ron")?;
    let movement = read("movement.ron")?;
    Content::from_ron(&materials, &spawns, &movement).map_err(|source| LoadError::Invalid {
        dir: dir.to_path_buf(),
        source: Box::new(source),
    })
}
