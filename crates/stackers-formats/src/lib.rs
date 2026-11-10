//! Parsers for the PS1 disc and asset formats used by Tecmo Stackers.
//!
//! No game data lives in this crate: everything is parsed at runtime from the
//! user's own disc image.

pub mod anm;
pub mod chain;
mod exe;
pub mod hud;
pub mod iso;
pub mod music;
pub mod presets;
pub mod sdchr;
pub mod sound;
pub mod tim;
pub mod title;
pub mod vab;
pub mod xa;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    BadMagic(&'static str),
    Unsupported(&'static str),
    NotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Truncated => write!(f, "data truncated"),
            Error::BadMagic(what) => write!(f, "bad magic: {what}"),
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::NotFound => write!(f, "file not found"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn u16le(b: &[u8], at: usize) -> Result<u16, Error> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(Error::Truncated)
}

pub(crate) fn u32le(b: &[u8], at: usize) -> Result<u32, Error> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(Error::Truncated)
}
