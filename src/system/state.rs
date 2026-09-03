use std::fmt::Display;

const MAGIC: &[u8; 8] = b"GBAESTAT";
pub const VERSION: u32 = 2;

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

pub struct Reader<'a> {
    bytes: &'a [u8],
}

macro_rules! numbers {
    ($($name:ident $many:ident: $number:ty),* $(,)?) => {
        impl Writer {
            $(
                pub fn $name(&mut self, value: $number) {
                    self.bytes(&value.to_le_bytes());
                }

                pub fn $many(&mut self, values: &[$number]) {
                    for value in values {
                        self.$name(*value);
                    }
                }
            )*
        }

        impl Reader<'_> {
            $(
                pub fn $name(&mut self) -> Result<$number, StateError> {
                    Ok(<$number>::from_le_bytes(self.take(std::mem::size_of::<$number>())?.try_into().unwrap()))
                }

                pub fn $many(&mut self, target: &mut [$number]) -> Result<(), StateError> {
                    for value in target {
                        *value = self.$name()?;
                    }
                    Ok(())
                }
            )*
        }
    };
}

numbers!(u8 u8s: u8, u16 u16s: u16, u32 u32s: u32, i32 i32s: i32, u64 u64s: u64, i64 i64s: i64, u128 u128s: u128);

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

    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub fn bools(&mut self, values: &[bool]) {
        for value in values {
            self.bool(*value);
        }
    }

    pub fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }

    pub fn sized_bytes(&mut self, value: &[u8]) {
        self.usize(value.len());
        self.bytes(value);
    }
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

    pub fn bool(&mut self) -> Result<bool, StateError> {
        Ok(self.u8()? != 0)
    }

    pub fn bools(&mut self, target: &mut [bool]) -> Result<(), StateError> {
        for value in target {
            *value = self.bool()?;
        }
        Ok(())
    }

    pub fn usize(&mut self) -> Result<usize, StateError> {
        usize::try_from(self.u64()?).map_err(|_| StateError::Corrupt)
    }

    pub fn sized_bytes(&mut self) -> Result<&'a [u8], StateError> {
        let length = self.usize()?;
        self.take(length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_values_round_trip() {
        let mut writer = Writer::new();
        writer.u8(1);
        writer.bool(true);
        writer.u16s(&[2, 3]);
        writer.i32(-4);
        writer.u64(5);
        writer.u128(6);
        writer.sized_bytes(b"seven");
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes).unwrap();
        assert_eq!(reader.u8().unwrap(), 1);
        assert!(reader.bool().unwrap());
        let mut pair = [0; 2];
        reader.u16s(&mut pair).unwrap();
        assert_eq!(pair, [2, 3]);
        assert_eq!(reader.i32().unwrap(), -4);
        assert_eq!(reader.u64().unwrap(), 5);
        assert_eq!(reader.u128().unwrap(), 6);
        assert_eq!(reader.sized_bytes().unwrap(), b"seven");
        assert_eq!(reader.u8(), Err(StateError::Truncated));
    }

    #[test]
    fn test_header_is_checked() {
        assert_eq!(Reader::new(b"nope").err(), Some(StateError::Truncated));
        assert_eq!(Reader::new(b"GBAESTAT\x09\x00\x00\x00").err(), Some(StateError::UnsupportedVersion(9)));
        assert_eq!(Reader::new(b"XXXXXXXX\x02\x00\x00\x00").err(), Some(StateError::BadMagic));
    }
}
