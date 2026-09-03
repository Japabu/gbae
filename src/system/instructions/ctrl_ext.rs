use crate::{
    bits::Bits,
    system::{
        cpu::{Psr, Register, CPU},
        memory::Memory,
    },
};

use super::{data_processing::ShifterOperand, Condition, Instruction};

const USER_MASK: u32 = 0xF000_0000;
const PRIVILEGED_MASK: u32 = 0x0000_00DF;
const STATE_MASK: u32 = 0x0000_0020;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mrs {
    pub d: Register,
    pub spsr: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Msr {
    pub operand: MsrOperand,
    pub fields: u32,
    pub spsr: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsrOperand {
    Immediate(u32),
    Register(Register),
}

#[inline(always)]
pub fn decode_mrs_arm(word: u32) -> Instruction {
    Instruction::Mrs(Mrs {
        d: Register::from(word.bits(12..16)),
        spsr: word.bit(22),
    })
}

#[inline(always)]
pub fn decode_msr_arm(word: u32) -> Instruction {
    Instruction::Msr(Msr {
        operand: if word.bit(25) {
            MsrOperand::Immediate(word.bits(0..8).rotate_right(word.bits(8..12) * 2))
        } else {
            MsrOperand::Register(Register::from(word.bits(0..4)))
        },
        fields: word.bits(16..20),
        spsr: word.bit(22),
    })
}

fn psr_name(spsr: bool) -> &'static str {
    if spsr {
        "SPSR"
    } else {
        "CPSR"
    }
}

impl Mrs {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        cpu.set_r(self.d, if self.spsr { cpu.spsr() } else { cpu.cpsr() }.bits());
    }

    pub fn encode_arm(self) -> u32 {
        0b00010 << 23 | u32::from(self.spsr) << 22 | 0b001111 << 16 | self.d.number() << 12
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("MRS{} {}, {}", cond, self.d, psr_name(self.spsr))
    }
}

impl Msr {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        let operand = match self.operand {
            MsrOperand::Immediate(immediate) => immediate,
            MsrOperand::Register(m) => cpu.r(m),
        };
        let mut mask = (0..4).filter(|byte| self.fields.bit(*byte)).fold(0, |mask, byte| mask | 0xFF << (8 * byte));
        if self.spsr {
            if cpu.has_spsr() {
                mask &= USER_MASK | PRIVILEGED_MASK | STATE_MASK;
                cpu.set_spsr(Psr::from(cpu.spsr().bits() & !mask | operand & mask));
            }
        } else {
            mask &= if cpu.mode().is_privileged() { USER_MASK | PRIVILEGED_MASK } else { USER_MASK };
            cpu.set_cpsr(Psr::from(cpu.cpsr().bits() & !mask | operand & mask));
        }
    }

    pub fn encode_arm(self) -> Option<u32> {
        let common = u32::from(self.spsr) << 22 | 0b10 << 20 | self.fields << 16 | 0b1111 << 12;
        Some(match self.operand {
            MsrOperand::Immediate(value) => {
                let (byte, rotate) = ShifterOperand::arm_immediate(value, 0)?;
                0b00110 << 23 | common | rotate << 8 | byte
            }
            MsrOperand::Register(m) => 0b00010 << 23 | common | m.number(),
        })
    }

    pub fn disassemble(self, cond: Condition) -> String {
        let fields: String = ['c', 'x', 's', 'f']
            .into_iter()
            .enumerate()
            .filter(|(index, _)| self.fields.bit(*index as u32))
            .map(|(_, letter)| letter)
            .collect();
        let operand = match self.operand {
            MsrOperand::Immediate(immediate) => format!("#{:#X}", immediate),
            MsrOperand::Register(m) => m.to_string(),
        };
        format!("MSR{} {}_{}, {}", cond, psr_name(self.spsr), fields, operand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msr() {
        assert_eq!(Instruction::decode_arm(0xE129_F000).disassemble(Condition::AL, 0), "MSR CPSR_cf, R0");
        assert_eq!(Instruction::decode_arm(0xE321_F0DF).disassemble(Condition::AL, 0), "MSR CPSR_c, #0xDF");
        assert_eq!(Instruction::decode_arm(0xE14F_0000).disassemble(Condition::AL, 0), "MRS R0, SPSR");
    }

    #[test]
    fn test_encoding_matches_known_words() {
        for word in [0xE129_F000, 0xE321_F0DF, 0xE14F_0000, 0xE10F_0000] {
            assert_eq!(Instruction::decode_arm(word).encode_arm(Condition::AL), Some(word), "{:08X}", word);
        }
    }
}
