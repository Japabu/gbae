use crate::bitutil::{get_bit, get_bits, sign_extend};

use super::CPU;

impl CPU {
    pub fn execute_branch(&mut self, instruction: u32) {
        let i = get_bit(instruction, 25);

        if i {
            self.branch_immediate(instruction);
        } else {
            panic!("Immediate must be set for branch");
        }
    }

    fn branch_immediate(&mut self, instruction: u32) {
        let l = get_bit(instruction, 24);
        let signed_immed_24 = get_bits(instruction, 0, 24);
        if l {
            self.r[14] = self.next_instruction_address_from_execution_stage();
        }
        let offset = sign_extend(signed_immed_24, 24) << 2;
        self.r[15] = self.r[15].wrapping_add(offset);
    }
}
