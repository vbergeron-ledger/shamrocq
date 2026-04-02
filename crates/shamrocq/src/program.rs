use crate::arena::ArenaError;
use shamrocq_bytecode::{MAGIC, BYTECODE_VERSION};

const MIN_BYTECODE_VERSION: u16 = BYTECODE_VERSION;
const MAX_BYTECODE_VERSION: u16 = BYTECODE_VERSION;

#[derive(Debug)]
pub enum VmError {
    Oom,
    MatchFailure { scrutinee_tag: u8, pc: usize },
    InvalidBytecode,
    UnsupportedVersion { version: u16 },
    NotCallable,
    IndexOutOfBounds,
    BytesOverflow,
    NotRegistered,
}

impl From<ArenaError> for VmError {
    fn from(_: ArenaError) -> Self {
        VmError::Oom
    }
}

pub struct Program<'a> {
    pub n_globals: u16,
    pub global_names: &'a [u8],
    pub code: &'a [u8],
}

impl<'a> Program<'a> {
    pub fn from_blob(blob: &'a [u8]) -> Result<Self, VmError> {
        if blob.len() < 4 + 2 + 2 {
            return Err(VmError::InvalidBytecode);
        }
        if blob[0..4] != MAGIC {
            return Err(VmError::InvalidBytecode);
        }
        let version = u16::from_le_bytes([blob[4], blob[5]]);
        if version < MIN_BYTECODE_VERSION || version > MAX_BYTECODE_VERSION {
            return Err(VmError::UnsupportedVersion { version });
        }
        let n_globals = u16::from_le_bytes([blob[6], blob[7]]);
        let mut pos = 8usize;
        for _ in 0..n_globals {
            if pos >= blob.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = blob[pos] as usize;
            pos += 1 + name_len + 2;
        }
        let globals_end = pos;
        if pos + 2 > blob.len() {
            return Err(VmError::InvalidBytecode);
        }
        let n_tags = u16::from_le_bytes([blob[pos], blob[pos + 1]]) as usize;
        pos += 2;
        for _ in 0..n_tags {
            if pos >= blob.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = blob[pos] as usize;
            pos += 1 + name_len;
        }
        Ok(Program {
            n_globals,
            global_names: &blob[8..globals_end],
            code: &blob[pos..],
        })
    }

    pub fn global_code_offset(&self, idx: u16) -> Result<u16, VmError> {
        let mut pos = 0usize;
        for i in 0..self.n_globals {
            if pos >= self.global_names.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = self.global_names[pos] as usize;
            pos += 1 + name_len;
            if i == idx {
                return Ok(u16::from_le_bytes([
                    self.global_names[pos],
                    self.global_names[pos + 1],
                ]));
            }
            pos += 2;
        }
        Err(VmError::InvalidBytecode)
    }

    pub fn global_index(&self, name: &str) -> Option<u16> {
        let name_bytes = name.as_bytes();
        let mut pos = 0usize;
        for i in 0..self.n_globals {
            if pos >= self.global_names.len() {
                return None;
            }
            let name_len = self.global_names[pos] as usize;
            let entry_name = &self.global_names[pos + 1..pos + 1 + name_len];
            if entry_name == name_bytes {
                return Some(i);
            }
            pos += 1 + name_len + 2;
        }
        None
    }
}
