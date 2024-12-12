use crate::{
    bitutil::{get_bit, get_bits16, get_bits32, sign_extend32},
    system::cpu::{CPU, INSTRUCTION_LEN_ARM, INSTRUCTION_LEN_THUMB, REGISTER_LR, REGISTER_PC},
};

use super::{Condition, DecodedInstruction};

#[derive(Debug, Clone, Copy)]
enum Opcode {
    B { offset: u32 },
    BL { offset: u32 },
    BX { m: u8 },
    BThumb { cond: Condition, offset: u32 },
}

pub fn decode_b_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits32(instruction, 0, 24);
    let offset = sign_extend32(signed_immed_24, 24) << 2;
    Box::new(Opcode::B { offset })
}

pub fn decode_bl_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits32(instruction, 0, 24);
    let offset = sign_extend32(signed_immed_24, 24) << 2;
    Box::new(Opcode::BL { offset })
}

pub fn decode_bx_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    Box::new(Opcode::BX {
        m: get_bits32(instruction, 0, 4) as u8,
    })
}

pub fn decode_branch_exchange_thumb(instruction: u16) -> Box<dyn super::DecodedInstruction> {
    Box::new(Opcode::BX {
        m: get_bits16(instruction, 3, 4) as u8,
    })
}

pub fn decode_conditional_branch_thumb(instruction: u16) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_8 = get_bits16(instruction, 0, 8);
    let offset = sign_extend32((signed_immed_8 as u32) << 1, 8);
    Box::new(Opcode::BThumb {
        cond: Condition::parse(get_bits16(instruction, 8, 4) as u8),
        offset,
    })
}

impl DecodedInstruction for Opcode {
    fn execute(&self, cpu: &mut CPU) {
        match *self {
            Opcode::B { offset } => cpu.set_r(REGISTER_PC, cpu.get_r(REGISTER_PC).wrapping_add(offset)),
            Opcode::BL { offset } => {
                cpu.set_r(REGISTER_LR, cpu.next_instruction_address_from_execution_stage());
                cpu.set_r(REGISTER_PC, cpu.get_r(REGISTER_PC).wrapping_add(offset));
            }
            Opcode::BX { m } => {
                let r_m = cpu.get_r(m);
                cpu.set_thumb_state(get_bit(r_m, 0));
                cpu.set_r(REGISTER_PC, r_m & 0xFFFFFFFE);
            }
            Opcode::BThumb { cond, offset } => {
                if cond.check(cpu) {
                    cpu.set_r(REGISTER_PC, cpu.get_r(REGISTER_PC).wrapping_add(offset));
                }
            }
        }
    }

    fn disassemble(&self, cond: Condition) -> String {
        use Opcode::*;

        match self {
            B { offset } => format!("B{} #{:#X}", cond, offset + INSTRUCTION_LEN_ARM),
            BL { offset } => format!("BL{} #{:#X}", cond, offset + INSTRUCTION_LEN_ARM),
            BX { m } => format!("BX{} R{}", cond, m),
            BThumb { cond, offset } => format!("B{} #{:#X}", cond, offset + INSTRUCTION_LEN_THUMB),
        }
    }
}
