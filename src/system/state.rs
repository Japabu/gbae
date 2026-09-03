use std::fmt::Display;

const MAGIC: &[u8; 8] = b"GBAESTAT";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateError {
    Truncated,
    BadMagic,
    UnsupportedVersion(u32),
    DifferentRom,
    Corrupt,
}

impl Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::Truncated => write!(f, "state file is truncated"),
            StateError::BadMagic => write!(f, "not a gbae state file"),
            StateError::UnsupportedVersion(version) => write!(f, "unsupported state version {}", version),
            StateError::DifferentRom => write!(f, "state was saved with a different ROM"),
            StateError::Corrupt => write!(f, "state file is corrupt"),
        }
    }
}

pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        let mut writer = Writer { bytes: Vec::new() };
        writer.bytes(MAGIC);
        writer.u32(VERSION);
        writer
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn bool(&mut self, value: bool) {
        self.u8(value as u8);
    }

    pub fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn i32(&mut self, value: i32) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn i64(&mut self, value: i64) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn u128(&mut self, value: u128) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }

    pub fn u16s(&mut self, values: &[u16]) {
        for value in values {
            self.u16(*value);
        }
    }

    pub fn u32s(&mut self, values: &[u32]) {
        for value in values {
            self.u32(*value);
        }
    }

    pub fn i32s(&mut self, values: &[i32]) {
        for value in values {
            self.i32(*value);
        }
    }

    pub fn bools(&mut self, values: &[bool]) {
        for value in values {
            self.bool(*value);
        }
    }

    pub fn sized_bytes(&mut self, value: &[u8]) {
        self.usize(value.len());
        self.bytes(value);
    }
}

pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Reader<'a>, StateError> {
        let mut reader = Reader { bytes };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(StateError::BadMagic);
        }
        let version = reader.u32()?;
        if version != VERSION {
            return Err(StateError::UnsupportedVersion(version));
        }
        Ok(reader)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], StateError> {
        if self.bytes.len() < length {
            return Err(StateError::Truncated);
        }
        let (taken, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(taken)
    }

    pub fn bytes_into(&mut self, target: &mut [u8]) -> Result<(), StateError> {
        target.copy_from_slice(self.take(target.len())?);
        Ok(())
    }

    pub fn u8(&mut self) -> Result<u8, StateError> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool, StateError> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16, StateError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> Result<u32, StateError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> Result<i32, StateError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn u64(&mut self) -> Result<u64, StateError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub fn i64(&mut self) -> Result<i64, StateError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub fn u128(&mut self) -> Result<u128, StateError> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().unwrap()))
    }

    pub fn usize(&mut self) -> Result<usize, StateError> {
        usize::try_from(self.u64()?).map_err(|_| StateError::Corrupt)
    }

    pub fn u16s(&mut self, target: &mut [u16]) -> Result<(), StateError> {
        for value in target {
            *value = self.u16()?;
        }
        Ok(())
    }

    pub fn u32s(&mut self, target: &mut [u32]) -> Result<(), StateError> {
        for value in target {
            *value = self.u32()?;
        }
        Ok(())
    }

    pub fn i32s(&mut self, target: &mut [i32]) -> Result<(), StateError> {
        for value in target {
            *value = self.i32()?;
        }
        Ok(())
    }

    pub fn bools(&mut self, target: &mut [bool]) -> Result<(), StateError> {
        for value in target {
            *value = self.bool()?;
        }
        Ok(())
    }

    pub fn sized_bytes(&mut self) -> Result<&'a [u8], StateError> {
        let length = self.usize()?;
        self.take(length)
    }
}
