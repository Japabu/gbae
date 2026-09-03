use crate::{
    bits::Bits,
    system::{
        cpu::{Register, CPU},
        memory::Memory,
    },
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multiply {
    pub opcode: Opcode,
    pub set_flags: bool,
    pub d: Register,
    pub n: Register,
    pub s: Register,
    pub m: Register,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    MUL,
    MLA,
    UMULL,
    UMLAL,
    SMULL,
    SMLAL,
}

#[inline(always)]
pub fn decode_arm(word: u32) -> Instruction {
    Instruction::Multiply(Multiply {
        opcode: match (word.bit(23), word.bit(22), word.bit(21)) {
            (false, _, false) => Opcode::MUL,
            (false, _, true) => Opcode::MLA,
            (true, false, false) => Opcode::UMULL,
            (true, false, true) => Opcode::UMLAL,
            (true, true, false) => Opcode::SMULL,
            (true, true, true) => Opcode::SMLAL,
        },
        set_flags: word.bit(20),
        d: Register::from(word.bits(16..20)),
        n: Register::from(word.bits(12..16)),
        s: Register::from(word.bits(8..12)),
        m: Register::from(word.bits(0..4)),
    })
}

#[inline(always)]
pub fn decode_mul_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let d = Register::from(word.bits(0..3));
    Instruction::Multiply(Multiply {
        opcode: Opcode::MUL,
        set_flags: true,
        d,
        n: Register::R0,
        s: d,
        m: Register::from(word.bits(3..6)),
    })
}

impl Opcode {
    fn is_long(self) -> bool {
        !matches!(self, Opcode::MUL | Opcode::MLA)
    }

    fn is_signed(self) -> bool {
        matches!(self, Opcode::MUL | Opcode::MLA | Opcode::SMULL | Opcode::SMLAL)
    }

    fn accumulates(self) -> bool {
        matches!(self, Opcode::MLA | Opcode::UMLAL | Opcode::SMLAL)
    }

    fn internal_cycles(self, multiplier: u32) -> u32 {
        let significant = if self.is_signed() && multiplier.bit(31) { !multiplier } else { multiplier };
        let bytes = match significant {
            0..=0xFF => 1,
            0x100..=0xFFFF => 2,
            0x1_0000..=0xFF_FFFF => 3,
            _ => 4,
        };
        match self {
            Opcode::MUL => bytes,
            Opcode::MLA | Opcode::UMULL | Opcode::SMULL => bytes + 1,
            Opcode::UMLAL | Opcode::SMLAL => bytes + 2,
        }
    }
}

impl Multiply {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let r_m = cpu.r(self.m);
        let r_s = cpu.r(self.s);
        mem.idle(self.opcode.internal_cycles(r_s));
        if self.opcode.is_long() {
            let product = if self.opcode.is_signed() {
                i64::from(r_m as i32).wrapping_mul(i64::from(r_s as i32)) as u64
            } else {
                u64::from(r_m) * u64::from(r_s)
            };
            let accumulator = if self.opcode.accumulates() {
                u64::from(cpu.r(self.d)) << 32 | u64::from(cpu.r(self.n))
            } else {
                0
            };
            let result = product.wrapping_add(accumulator);
            cpu.set_r(self.n, result as u32);
            cpu.set_r(self.d, (result >> 32) as u32);
            if self.set_flags {
                cpu.set_negative_zero(result.bit(63), result == 0);
            }
        } else {
            let accumulator = if self.opcode.accumulates() { cpu.r(self.n) } else { 0 };
            let result = r_m.wrapping_mul(r_s).wrapping_add(accumulator);
            cpu.set_r(self.d, result);
            if self.set_flags {
                cpu.set_nz(result);
            }
        }
    }

    pub fn encode_arm(self) -> u32 {
        u32::from(self.opcode.is_long()) << 23
            | u32::from(self.opcode.is_long() && self.opcode.is_signed()) << 22
            | u32::from(self.opcode.accumulates()) << 21
            | u32::from(self.set_flags) << 20
            | self.d.number() << 16
            | self.n.number() << 12
            | self.s.number() << 8
            | 0b1001 << 4
            | self.m.number()
    }

    pub fn encode_thumb(self) -> Option<u16> {
        let fits = self.opcode == Opcode::MUL && self.set_flags && self.d.is_low() && self.m.is_low() && self.s == self.d && self.n == Register::R0;
        fits.then(|| u16::try_from(0b01_0000_1101 << 6 | self.m.number() << 3 | self.d.number()).ok()).flatten()
    }

    pub fn disassemble(self, cond: Condition) -> String {
        let s = if self.set_flags { "S" } else { "" };
        match self.opcode {
            Opcode::MUL => format!("MUL{}{} {}, {}, {}", cond, s, self.d, self.m, self.s),
            Opcode::MLA => format!("MLA{}{} {}, {}, {}, {}", cond, s, self.d, self.m, self.s, self.n),
            _ => format!("{:?}{}{} {}, {}, {}, {}", self.opcode, cond, s, self.n, self.d, self.m, self.s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiply_cycles() {
        assert_eq!(Opcode::MUL.internal_cycles(0x0000_0012), 1);
        assert_eq!(Opcode::MUL.internal_cycles(0xFFFF_FFF0), 1);
        assert_eq!(Opcode::MUL.internal_cycles(0x0000_1234), 2);
        assert_eq!(Opcode::MUL.internal_cycles(0x0012_3456), 3);
        assert_eq!(Opcode::MUL.internal_cycles(0x1234_5678), 4);
        assert_eq!(Opcode::UMULL.internal_cycles(0xFFFF_FFF0), 5);
        assert_eq!(Opcode::SMLAL.internal_cycles(0xFFFF_FFF0), 3);
    }

    #[test]
    fn test_multiply() {
        assert_eq!(Instruction::decode_arm(0xE001_0392).disassemble(Condition::AL, 0), "MUL R1, R2, R3");
        assert_eq!(Instruction::decode_arm(0xE0C3_2190).disassemble(Condition::AL, 0), "SMULL R2, R3, R0, R1");
        assert_eq!(Instruction::decode_thumb(0x4348).disassemble(Condition::AL, 0), "MULS R0, R1, R0");
    }

    #[test]
    fn test_encoding_matches_known_words() {
        for word in [0xE001_0392, 0xE0C3_2190, 0xE0E3_2190, 0xE023_1392] {
            assert_eq!(Instruction::decode_arm(word).encode_arm(Condition::AL), Some(word), "{:08X}", word);
        }
        assert_eq!(Instruction::decode_thumb(0x4348).encode_thumb(), Some(0x4348));
    }
}
