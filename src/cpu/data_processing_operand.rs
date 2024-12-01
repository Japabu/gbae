use crate::bitutil::{get_bit, get_bits};

use super::CPU;

impl CPU {
    pub fn decode_data_processing_operand(&self, instruction: u32) -> (u32, bool) {
        let imm_bit = get_bit(instruction, 25);
        if imm_bit {
            self.ddpo_immediate(instruction)
        } else if get_bits(instruction, 4, 8) == 0 {
            self.ddpo_register(instruction)
        } else {
            self.ddpo_shift(instruction)
        }
    }

    fn ddpo_immediate(&self, instruction: u32) -> (u32, bool) {
        let immed_8 = get_bits(instruction, 0, 8);
        let rotate_imm = get_bits(instruction, 8, 4);
        let shifter_operand = immed_8.rotate_right(2 * rotate_imm);
        let carry: bool;
        if rotate_imm == 0 {
            carry = self.r.get_carry_flag()
        } else {
            carry = get_bit(shifter_operand, 31)
        };
        (shifter_operand, carry)
    }

    fn ddpo_register(&self, instruction: u32) -> (u32, bool) {
        let r_m = self.r[get_bits(instruction, 0, 4)];
        (r_m, self.r.get_carry_flag())
    }

    fn ddpo_shift(&self, instruction: u32) -> (u32, bool) {
        let register_bit = get_bit(instruction, 4);
        let shift_type = get_bits(instruction, 5, 2);
        match (register_bit, shift_type) {
            _ => panic!(
                "Unknown shift, reg_bit: {}, shift_type: {:02b}",
                register_bit, shift_type
            ),
        }
    }
}
