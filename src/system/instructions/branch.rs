use crate::{
    bits::Bits,
    system::{
        cpu::{Register, CPU, INSTRUCTION_LEN_ARM, INSTRUCTION_LEN_THUMB},
        memory::Memory,
    },
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Branch {
    pub link: bool,
    pub cond: Condition,
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchExchange {
    pub m: Register,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchLinkPrefix {
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchLinkSuffix {
    pub offset: u32,
}

#[inline(always)]
pub fn decode_b_arm(word: u32) -> Instruction {
    Instruction::Branch(Branch {
        link: false,
        cond: Condition::AL,
        offset: arm_offset(word),
    })
}

#[inline(always)]
pub fn decode_bl_arm(word: u32) -> Instruction {
    Instruction::Branch(Branch {
        link: true,
        cond: Condition::AL,
        offset: arm_offset(word),
    })
}

#[inline(always)]
fn arm_offset(word: u32) -> u32 {
    (word.bits(0..24).sign_extended(24) << 2).wrapping_add(INSTRUCTION_LEN_ARM * 2)
}

#[inline(always)]
pub fn decode_bx_arm(word: u32) -> Instruction {
    Instruction::BranchExchange(BranchExchange { m: Register::from(word.bits(0..4)) })
}

#[inline(always)]
pub fn decode_branch_exchange_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    if word.bit(7) {
        Instruction::Unknown(word)
    } else {
        Instruction::BranchExchange(BranchExchange { m: Register::from(word.bits(3..7)) })
    }
}

#[inline(always)]
pub fn decode_conditional_branch_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::Branch(Branch {
        link: false,
        cond: Condition::from_bits(word.bits(8..12)),
        offset: (word.bits(0..8).sign_extended(8) << 1).wrapping_add(INSTRUCTION_LEN_THUMB * 2),
    })
}

#[inline(always)]
pub fn decode_unconditional_branch_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::Branch(Branch {
        link: false,
        cond: Condition::AL,
        offset: (word.bits(0..11).sign_extended(11) << 1).wrapping_add(INSTRUCTION_LEN_THUMB * 2),
    })
}

#[inline(always)]
pub fn decode_bl_prefix_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::BranchLinkPrefix(BranchLinkPrefix {
        offset: word.bits(0..11).sign_extended(11) << 12,
    })
}

#[inline(always)]
pub fn decode_bl_suffix_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::BranchLinkSuffix(BranchLinkSuffix { offset: word.bits(0..11) << 1 })
}

fn signed_field(value: u32, shift: u32, width: u32) -> Option<u32> {
    if value.bits(0..shift) != 0 {
        return None;
    }
    let field = value.arithmetic_shift_right(shift);
    (field.sign_extended(width) == field).then(|| field.bits(0..width))
}

impl Branch {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        if self.cond.check(cpu) {
            let target = cpu.pc().wrapping_add(self.offset);
            if self.link {
                cpu.set_r(Register::LR, cpu.next_pc());
            }
            cpu.set_r(Register::PC, target);
        }
    }

    pub fn encode_arm(self) -> Option<u32> {
        let field = signed_field(self.offset.wrapping_sub(INSTRUCTION_LEN_ARM * 2), 2, 24)?;
        Some(0b101 << 25 | u32::from(self.link) << 24 | field)
    }

    pub fn encode_thumb(self) -> Option<u16> {
        if self.link {
            return None;
        }
        let offset = self.offset.wrapping_sub(INSTRUCTION_LEN_THUMB * 2);
        let word = match self.cond {
            Condition::AL => 0b11100 << 11 | signed_field(offset, 1, 11)?,
            cond => 0b1101 << 12 | cond.bits() << 8 | signed_field(offset, 1, 8)?,
        };
        u16::try_from(word).ok()
    }

    pub fn disassemble(self, cond: Condition, address: u32) -> String {
        let cond = if self.cond == Condition::AL { cond } else { self.cond };
        format!("B{}{} #{:08X}", if self.link { "L" } else { "" }, cond, address.wrapping_add(self.offset))
    }
}

impl BranchExchange {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        let r_m = cpu.r(self.m);
        cpu.set_thumb(r_m.bit(0));
        cpu.set_pc(r_m);
    }

    pub fn encode_arm(self) -> u32 {
        0b0001_0010_1111_1111_1111_0001 << 4 | self.m.number()
    }

    pub fn encode_thumb(self) -> Option<u16> {
        u16::try_from(0b0100_0111_0 << 7 | self.m.number() << 3).ok()
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("BX{} {}", cond, self.m)
    }
}

impl BranchLinkPrefix {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        cpu.set_r(Register::LR, cpu.r(Register::PC).wrapping_add(self.offset));
    }

    pub fn encode_thumb(self) -> Option<u16> {
        u16::try_from(0b11110 << 11 | signed_field(self.offset, 12, 11)?).ok()
    }

    pub fn disassemble(self, address: u32) -> String {
        format!("BL prefix #{:08X}", address.wrapping_add(INSTRUCTION_LEN_THUMB * 2).wrapping_add(self.offset))
    }

    pub fn target(self, suffix: BranchLinkSuffix, address: u32) -> u32 {
        address.wrapping_add(INSTRUCTION_LEN_THUMB * 2).wrapping_add(self.offset).wrapping_add(suffix.offset)
    }
}

impl BranchLinkSuffix {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        let target = cpu.r(Register::LR).wrapping_add(self.offset);
        cpu.set_r(Register::LR, cpu.next_pc() | 1);
        cpu.set_r(Register::PC, target & !0b1);
    }

    pub fn encode_thumb(self) -> Option<u16> {
        if self.offset.bit(0) || self.offset >= 1 << 12 {
            return None;
        }
        u16::try_from(0b11111 << 11 | self.offset >> 1).ok()
    }

    pub fn disassemble(self) -> String {
        format!("BL suffix #{:#X}", self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_targets() {
        assert_eq!(Instruction::decode_arm(0xEA00_0002).disassemble(Condition::AL, 0x100), "B #00000110");
        assert_eq!(Instruction::decode_arm(0xEBFF_FFFE).disassemble(Condition::NE, 0x100), "BLNE #00000100");
        assert_eq!(Instruction::decode_thumb(0xD0FE).disassemble(Condition::AL, 0x100), "BEQ #00000100");
        assert_eq!(Instruction::decode_thumb(0xE7FE).disassemble(Condition::AL, 0x100), "B #00000100");
        assert_eq!(Instruction::decode_arm(0xE12F_FF11).disassemble(Condition::AL, 0), "BX R1");
    }

    #[test]
    fn test_encoding_matches_known_words() {
        for word in [0xEA00_0002, 0xEBFF_FFFE, 0xE12F_FF11] {
            assert_eq!(Instruction::decode_arm(word).encode_arm(Condition::AL), Some(word), "{:08X}", word);
        }
        assert_eq!(Instruction::decode_arm(0x1BFF_FFFE).encode_arm(Condition::NE), Some(0x1BFF_FFFE));
        for word in [0xD0FE, 0xE7FE, 0x4770, 0xF000, 0xF802, 0xF7FF] {
            assert_eq!(Instruction::decode_thumb(word).encode_thumb(), Some(word), "{:04X}", word);
        }
        assert_eq!(Instruction::decode_thumb(0xD0FE).encode_arm(Condition::AL), Some(0x0AFF_FFFE));
    }
}
