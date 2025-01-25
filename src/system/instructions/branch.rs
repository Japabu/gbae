use crate::{
    bitutil::{get_bit, get_bit16, get_bits16, get_bits32, sign_extend32},
    system::{
        cpu::{CPU, INSTRUCTION_LEN_ARM, INSTRUCTION_LEN_THUMB, REGISTER_LR, REGISTER_PC},
        memory::Memory,
    },
};

use super::{Condition, DecodedInstruction};

#[derive(Debug, Clone, Copy)]
enum Opcode {
    BOffset { l: bool, x: bool, offset: u32 },
    BRegister { l: bool, x: bool, m: u8 },
    BCondThumb { cond: Condition, offset: u32 },
    BLThumb { offset: u32 },
}

pub fn decode_b_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits32(instruction, 0, 24);
    let offset = (sign_extend32(signed_immed_24, 24) << 2).wrapping_add(INSTRUCTION_LEN_ARM * 2);
    Box::new(Opcode::BOffset { l: false, x: false, offset })
}

pub fn decode_bl_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits32(instruction, 0, 24);
    let offset = (sign_extend32(signed_immed_24, 24) << 2).wrapping_add(INSTRUCTION_LEN_ARM * 2);
    Box::new(Opcode::BOffset { l: true, x: false, offset })
}

pub fn decode_bx_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    Box::new(Opcode::BRegister {
        l: false,
        x: true,
        m: get_bits32(instruction, 0, 4) as u8,
    })
}

pub fn decode_blx_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    Box::new(Opcode::BRegister {
        l: true,
        x: true,
        m: get_bits32(instruction, 0, 4) as u8,
    })
}

pub fn decode_branch_exchange_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn super::DecodedInstruction> {
    let l = get_bit16(instruction, 7);
    if l {
        panic!("BLX (2) not implemented");
    }
    Box::new(Opcode::BRegister {
        l,
        x: true,
        m: get_bits16(instruction, 3, 4) as u8,
    })
}

pub fn decode_bl_thumb(instruction: u16, next_instruction: u16) -> Box<dyn super::DecodedInstruction> {
    assert_eq!(get_bits16(instruction, 11, 2), 0b10);
    assert_eq!(get_bits16(next_instruction, 11, 2), 0b11);

    let hi = sign_extend32(get_bits16(instruction, 0, 11) as u32, 11) << 12;
    let lo = get_bits16(next_instruction, 0, 11) as u32 * 2;

    let offset = hi.wrapping_add(INSTRUCTION_LEN_THUMB * 2).wrapping_add(lo);

    Box::new(Opcode::BLThumb { offset })
}

pub fn decode_conditional_branch_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_8 = get_bits16(instruction, 0, 8);
    let offset = (sign_extend32(signed_immed_8 as u32, 8) << 1).wrapping_add(INSTRUCTION_LEN_THUMB * 2);
    Box::new(Opcode::BCondThumb {
        cond: Condition::parse(get_bits16(instruction, 8, 4) as u8),
        offset,
    })
}

pub fn decode_unconditional_branch_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_11 = get_bits16(instruction, 0, 11);
    let offset = (sign_extend32(signed_immed_11 as u32, 11) << 1).wrapping_add(INSTRUCTION_LEN_THUMB * 2);
    Box::new(Opcode::BOffset { l: false, x: false, offset })
}

impl DecodedInstruction for Opcode {
    fn execute(&self, cpu: &mut CPU, _mem: &mut Memory) {
        match *self {
            Opcode::BOffset { l, x, offset } => {
                if l {
                    cpu.set_r(REGISTER_LR, cpu.next_instruction_address_from_execution_stage());
                }
                if x {
                    cpu.set_thumb_state(true);
                }
                cpu.set_r(REGISTER_PC, cpu.curr_instruction_address_from_execution_stage().wrapping_add(offset));
            }
            Opcode::BRegister { l, x, m } => {
                if l {
                    cpu.set_r(REGISTER_LR, cpu.next_instruction_address_from_execution_stage());
                }
                let r_m = cpu.get_r(m);
                if x {
                    cpu.set_thumb_state(get_bit(r_m, 0));
                }
                cpu.set_r(REGISTER_PC, r_m & 0xFFFFFFFE);
            }
            Opcode::BCondThumb { cond, offset } => {
                if cond.check(cpu) {
                    cpu.set_r(REGISTER_PC, cpu.curr_instruction_address_from_execution_stage().wrapping_add(offset));
                }
            }
            Opcode::BLThumb { offset } => {
                // Point LR to the next instruction after the suffix
                cpu.set_r(REGISTER_LR, cpu.next_instruction_address_from_execution_stage() + cpu.instruction_len_in_bytes() | 1);
                cpu.set_r(REGISTER_PC, cpu.curr_instruction_address_from_execution_stage().wrapping_add(offset));
            }
        }
    }

    fn disassemble(&self, cond: Condition, base_address: u32) -> String {
        use Opcode::*;
        match *self {
            BOffset { l, x, offset } => format!("B{}{}{} #{:08X}", if l { "L" } else { "" }, if x { "X" } else { "" }, cond, base_address.wrapping_add(offset)),
            BRegister { l, x, m } => format!("B{}{}{} R{}", if l { "L" } else { "" }, if x { "X" } else { "" }, cond, m),
            BCondThumb { cond, offset } => format!("B{} #{:08X}", cond, base_address.wrapping_add(offset)),
            BLThumb { offset } => format!("BL #{:08X}", base_address.wrapping_add(offset)),
        }
    }
}
