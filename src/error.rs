use std::fmt;
use std::path::{Path, PathBuf};
use std::result;

#[derive(Debug)]
pub struct Error {
    path: PathBuf,
    message: String
}

impl Error {
    pub fn new(path: PathBuf, message: &str) -> Error {
        Error{path: path, message: message.to_string()}
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

pub type Result<T> = result::Result<T, Error>;
