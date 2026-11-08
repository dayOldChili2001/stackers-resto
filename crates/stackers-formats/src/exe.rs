//! Reading data tables out of the PS-X executable (`SLUS_003.15`) by RAM address.

use crate::{u32le, Error};

/// Maps RAM addresses to offsets in a PS-X EXE.
pub(crate) struct Exe<'a> {
    data: &'a [u8],
    text: u32,
}

impl<'a> Exe<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> Result<Self, Error> {
        if data.get(0..8) != Some(b"PS-X EXE") {
            return Err(Error::BadMagic("PS-X EXE"));
        }
        Ok(Exe { data, text: u32le(data, 0x18)? })
    }

    pub(crate) fn at(&self, addr: u32, len: usize) -> Result<&'a [u8], Error> {
        let off = addr.checked_sub(self.text).ok_or(Error::Truncated)? as usize + 0x800;
        self.data.get(off..off + len).ok_or(Error::Truncated)
    }
}
