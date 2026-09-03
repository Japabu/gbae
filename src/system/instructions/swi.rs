use crate::{
    bits::Bits,
    system::{bios, cpu::CPU, memory::Memory},
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoftwareInterrupt {
    pub comment: u32,
}

#[inline(always)]
pub fn decode_arm(word: u32) -> Instruction {
    Instruction::SoftwareInterrupt(SoftwareInterrupt { comment: word.bits(0..24) })
}

#[inline(always)]
pub fn decode_thumb(word: u16) -> Instruction {
    Instruction::SoftwareInterrupt(SoftwareInterrupt { comment: u32::from(word).bits(0..8) })
}

impl SoftwareInterrupt {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let function = if cpu.thumb() { self.comment } else { self.comment.bits(16..24) };
        bios::call(function, cpu, mem);
    }

    pub fn encode_arm(self) -> Option<u32> {
        (self.comment < 1 << 24).then_some(0b1111 << 24 | self.comment)
    }

    pub fn encode_thumb(self) -> Option<u16> {
        u16::try_from(0b1101_1111 << 8 | self.comment).ok().filter(|_| self.comment < 1 << 8)
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("SWI{} #{:#X}", cond, self.comment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_matches_known_words() {
        assert_eq!(Instruction::decode_arm(0xEF00_0005).encode_arm(Condition::AL), Some(0xEF00_0005));
        assert_eq!(Instruction::decode_thumb(0xDF05).encode_thumb(), Some(0xDF05));
        assert_eq!(Instruction::decode_thumb(0xDF05).encode_arm(Condition::AL), Some(0xEF00_0005));
        assert_eq!(Instruction::decode_arm(0xEF01_0000).encode_thumb(), None);
    }
}
