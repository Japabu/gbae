use crate::{
    bitutil::{get_bit, get_bit16, get_bits16, get_bits32, sign_extend32},
    system::{
        cpu::{CPU, INSTRUCTION_LEN_ARM, INSTRUCTION_LEN_THUMB, REGISTER_LR, REGISTER_PC},
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
    pub m: u8,
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
pub fn decode_b_arm(instruction: u32) -> Instruction {
    Instruction::Branch(Branch {
        link: false,
        cond: Condition::AL,
        offset: arm_offset(instruction),
    })
}

#[inline(always)]
pub fn decode_bl_arm(instruction: u32) -> Instruction {
    Instruction::Branch(Branch {
        link: true,
        cond: Condition::AL,
        offset: arm_offset(instruction),
    })
}

#[inline(always)]
fn arm_offset(instruction: u32) -> u32 {
    (sign_extend32(get_bits32(instruction, 0, 24), 24) << 2).wrapping_add(INSTRUCTION_LEN_ARM * 2)
}

#[inline(always)]
pub fn decode_bx_arm(instruction: u32) -> Instruction {
    Instruction::BranchExchange(BranchExchange {
        m: get_bits32(instruction, 0, 4) as u8,
    })
}

#[inline(always)]
pub fn decode_branch_exchange_thumb(instruction: u16) -> Instruction {
    if get_bit16(instruction, 7) {
        Instruction::Unknown(instruction as u32)
    } else {
        Instruction::BranchExchange(BranchExchange {
            m: get_bits16(instruction, 3, 4) as u8,
        })
    }
}

#[inline(always)]
pub fn decode_conditional_branch_thumb(instruction: u16) -> Instruction {
    Instruction::Branch(Branch {
        link: false,
        cond: Condition::parse(get_bits16(instruction, 8, 4) as u8),
        offset: (sign_extend32(get_bits16(instruction, 0, 8) as u32, 8) << 1).wrapping_add(INSTRUCTION_LEN_THUMB * 2),
    })
}

#[inline(always)]
pub fn decode_unconditional_branch_thumb(instruction: u16) -> Instruction {
    Instruction::Branch(Branch {
        link: false,
        cond: Condition::AL,
        offset: (sign_extend32(get_bits16(instruction, 0, 11) as u32, 11) << 1).wrapping_add(INSTRUCTION_LEN_THUMB * 2),
    })
}

#[inline(always)]
pub fn decode_bl_prefix_thumb(instruction: u16) -> Instruction {
    Instruction::BranchLinkPrefix(BranchLinkPrefix {
        offset: sign_extend32(get_bits16(instruction, 0, 11) as u32, 11) << 12,
    })
}

#[inline(always)]
pub fn decode_bl_suffix_thumb(instruction: u16) -> Instruction {
    Instruction::BranchLinkSuffix(BranchLinkSuffix {
        offset: (get_bits16(instruction, 0, 11) as u32) << 1,
    })
}

impl Branch {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        if self.cond.check(cpu) {
            let target = cpu.curr_instruction_address_from_execution_stage().wrapping_add(self.offset);
            if self.link {
                cpu.set_r(REGISTER_LR, cpu.next_instruction_address_from_execution_stage());
            }
            cpu.set_r(REGISTER_PC, target);
        }
    }

    pub fn disassemble(self, cond: Condition, address: u32) -> String {
        let cond = if self.cond == Condition::AL { cond } else { self.cond };
        format!("B{}{} #{:08X}", if self.link { "L" } else { "" }, cond, address.wrapping_add(self.offset))
    }
}

impl BranchExchange {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        let r_m = cpu.get_r(self.m);
        cpu.set_thumb_state(get_bit(r_m, 0));
        cpu.set_pc(r_m);
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("BX{} R{}", cond, self.m)
    }
}

impl BranchLinkPrefix {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        cpu.set_r(REGISTER_LR, cpu.get_r(REGISTER_PC).wrapping_add(self.offset));
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
        let target = cpu.get_r(REGISTER_LR).wrapping_add(self.offset);
        cpu.set_r(REGISTER_LR, cpu.next_instruction_address_from_execution_stage() | 1);
        cpu.set_r(REGISTER_PC, target & !0b1);
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
        assert_eq!(Instruction::decode_arm(0xEA000002).disassemble(Condition::AL, 0x100), "B #00000110");
        assert_eq!(Instruction::decode_arm(0xEBFFFFFE).disassemble(Condition::NE, 0x100), "BLNE #00000100");
        assert_eq!(Instruction::decode_thumb(0xD0FE).disassemble(Condition::AL, 0x100), "BEQ #00000100");
        assert_eq!(Instruction::decode_thumb(0xE7FE).disassemble(Condition::AL, 0x100), "B #00000100");
        assert_eq!(Instruction::decode_arm(0xE12FFF11).disassemble(Condition::AL, 0), "BX R1");
    }
}
