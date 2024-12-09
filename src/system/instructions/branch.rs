use std::fmt::Display;

use crate::{
    bitutil::{get_bit, get_bits, sign_extend},
    system::cpu::CPU,
};

use super::{Condition, DecodedInstruction};

struct Branch {
    cond: Condition,
    opcode: Opcode,
}

enum Opcode {
    B { offset: u32 },
    BL { offset: u32 },
    BX { m: u8 },
}

pub fn decode_b_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits(instruction, 0, 24);
    let offset = sign_extend(signed_immed_24, 24) << 2;
    Box::new(Branch {
        cond: Condition::decode_arm(instruction),
        opcode: Opcode::B { offset },
    })
}

pub fn decode_bl_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits(instruction, 0, 24);
    let offset = sign_extend(signed_immed_24, 24) << 2;
    Box::new(Branch {
        cond: Condition::decode_arm(instruction),
        opcode: Opcode::BL { offset },
    })
}

pub fn decode_bx_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let m = get_bits(instruction, 0, 4) as u8;
    Box::new(Branch {
        cond: Condition::decode_arm(instruction),
        opcode: Opcode::BX { m },
    })
}

impl Display for Branch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Opcode::*;

        match self.opcode {
            B { offset } => write!(f, "B{} #{:#X}", self.cond, offset),
            BL { offset } => write!(f, "BL{} #{:#X}", self.cond, offset),
            BX { m } => write!(f, "BX{} R{}", self.cond, m),
        }
    }
}

impl DecodedInstruction for Branch {
    fn execute(&self, cpu: &mut CPU) {
        if !self.cond.check(cpu) {
            return;
        }

        match self.opcode {
            Opcode::B { offset } => cpu.set_r(15, cpu.get_r(15).wrapping_add(offset)),
            Opcode::BL { offset } => {
                cpu.set_r(14, cpu.next_instruction_address_from_execution_stage());
                cpu.set_r(15, cpu.get_r(15).wrapping_add(offset));
            }
            Opcode::BX { m } => {
                let r_m = cpu.get_r(m as usize);
                cpu.set_thumb_state(get_bit(r_m, 0));
                cpu.set_r(15, r_m & 0xFFFFFFFE);
            }
        }
    }
}
