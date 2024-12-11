use crate::{
    bitutil::{get_bit, get_bits, sign_extend},
    system::cpu::CPU,
};

use super::{Condition, DecodedInstruction};

#[derive(Debug)]
enum Opcode {
    B { offset: u32 },
    BL { offset: u32 },
    BX { m: u8 },
}

pub fn decode_b_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits(instruction, 0, 24);
    let offset = sign_extend(signed_immed_24, 24) << 2;
    Box::new(Opcode::B { offset })
}

pub fn decode_bl_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let signed_immed_24 = get_bits(instruction, 0, 24);
    let offset = sign_extend(signed_immed_24, 24) << 2;
    Box::new(Opcode::BL { offset })
}

pub fn decode_bx_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let m = get_bits(instruction, 0, 4) as u8;
    Box::new(Opcode::BX { m })
}

impl DecodedInstruction for Opcode {
    fn execute(&self, cpu: &mut CPU) {
        match *self {
            Opcode::B { offset } => cpu.set_r(15, cpu.get_r(15).wrapping_add(offset)),
            Opcode::BL { offset } => {
                cpu.set_r(14, cpu.next_instruction_address_from_execution_stage());
                cpu.set_r(15, cpu.get_r(15).wrapping_add(offset));
            }
            Opcode::BX { m } => {
                let r_m = cpu.get_r(m);
                cpu.set_thumb_state(get_bit(r_m, 0));
                cpu.set_r(15, r_m & 0xFFFFFFFE);
            }
        }
    }

    fn disassemble(&self, cond: Condition) -> String {
        use Opcode::*;

        match self {
            B { offset } => format!("B{} #{:#X}", cond, offset + 8),
            BL { offset } => format!("BL{} #{:#X}", cond, offset + 8),
            BX { m } => format!("BX{} R{}", cond, m),
        }
    }
}
