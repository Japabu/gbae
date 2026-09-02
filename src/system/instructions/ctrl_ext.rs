use crate::{
    bitutil::{get_bit, get_bits32},
    system::cpu::CPU,
};

use super::{Condition, Instruction};

const USER_MASK: u32 = 0xF0000000;
const PRIV_MASK: u32 = 0x000000DF;
const STATE_MASK: u32 = 0x00000020;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mrs {
    pub d: u8,
    pub r: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Msr {
    pub operand: MsrOperand,
    pub field_mask: u8,
    pub r: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsrOperand {
    Immediate(u32),
    Register(u8),
}

#[inline(always)]
pub fn decode_mrs_arm(instruction: u32) -> Instruction {
    Instruction::Mrs(Mrs {
        d: get_bits32(instruction, 12, 4) as u8,
        r: get_bit(instruction, 22),
    })
}

#[inline(always)]
pub fn decode_msr_arm(instruction: u32) -> Instruction {
    Instruction::Msr(Msr {
        operand: if get_bit(instruction, 25) {
            MsrOperand::Immediate(get_bits32(instruction, 0, 8).rotate_right(get_bits32(instruction, 8, 4) * 2))
        } else {
            MsrOperand::Register(get_bits32(instruction, 0, 4) as u8)
        },
        field_mask: get_bits32(instruction, 16, 4) as u8,
        r: get_bit(instruction, 22),
    })
}

impl Mrs {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU) {
        cpu.set_r(self.d, if self.r { cpu.get_spsr() } else { cpu.get_cpsr() });
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("MRS{} R{}, {}", cond, self.d, if self.r { "SPSR" } else { "CPSR" })
    }
}

impl Msr {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU) {
        let operand = match self.operand {
            MsrOperand::Immediate(immediate) => immediate,
            MsrOperand::Register(m) => cpu.get_r(m),
        };

        let mut mask = 0u32;
        for i in 0..4 {
            if get_bit(self.field_mask as u32, i) {
                mask |= 0xFF << (8 * i);
            }
        }

        if self.r {
            if cpu.current_mode_has_spsr() {
                mask &= USER_MASK | PRIV_MASK | STATE_MASK;
                cpu.set_spsr((cpu.get_spsr() & !mask) | (operand & mask));
            }
        } else {
            mask &= if cpu.in_a_privileged_mode() { USER_MASK | PRIV_MASK } else { USER_MASK };
            cpu.set_cpsr((cpu.get_cpsr() & !mask) | (operand & mask));
        }
    }

    pub fn disassemble(self, cond: Condition) -> String {
        let field_mask = self.field_mask as u32;
        format!(
            "MSR{} {}_{}{}{}{}, {}",
            cond,
            if self.r { "SPSR" } else { "CPSR" },
            if get_bit(field_mask, 0) { "c" } else { "" },
            if get_bit(field_mask, 1) { "x" } else { "" },
            if get_bit(field_mask, 2) { "s" } else { "" },
            if get_bit(field_mask, 3) { "f" } else { "" },
            match self.operand {
                MsrOperand::Immediate(immediate) => format!("#{:#X}", immediate),
                MsrOperand::Register(m) => format!("R{}", m),
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msr() {
        assert_eq!(Instruction::decode_arm(0xE129F000).disassemble(Condition::AL, 0), "MSR CPSR_cf, R0");
        assert_eq!(Instruction::decode_arm(0xE321F0DF).disassemble(Condition::AL, 0), "MSR CPSR_c, #0xDF");
        assert_eq!(Instruction::decode_arm(0xE14F0000).disassemble(Condition::AL, 0), "MRS R0, SPSR");
    }
}
