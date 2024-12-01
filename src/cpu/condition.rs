use crate::bitutil::get_bits;

use super::CPU;

impl CPU {
    pub fn evaluate_condition(&self, instruction: u32) -> bool {
        let condition = get_bits(instruction, 28, 4);
        match condition {
            0b0000 => self.r.get_zero_flag(),
            0b0010 => self.r.get_carry_flag(),
            0b0100 => self.r.get_negative_flag(),
            0b1110 => true,
            _ => panic!("Unknown condition: {:04b}", condition),
        }
    }
}
