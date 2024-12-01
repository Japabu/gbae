use crate::bitutil::{get_bit, get_bits};

use super::CPU;

impl CPU {
    pub fn execute_data_processing_imm_shift(&mut self, instruction: u32) {
        // set flags bit
        let s = get_bit(instruction, 20);

        // operand 1 register
        let n = get_bits(instruction, 16, 4);

        // destination register
        let d = get_bits(instruction, 12, 4);
        let (so, sco) = self.decode_data_processing_operand(instruction);

        let opcode = get_bits(instruction, 21, 4);
        match opcode {
            0b1001 => self.teq(s, n, d, so, sco),
            0b1101 => self.mov(s, n, d, so, sco),
            _ => panic!("Unknown data processing opcode: {:04b}", opcode),
        }
    }

    pub fn set_flags(&mut self, v: u32) {
        self.r.set_negative_flag(get_bit(v, 31));
        self.r.set_zero_flag(v == 0);
    }

    fn teq(&mut self, s: bool, n: u32, d: u32, so: u32, sco: bool) {
        debug_assert!(s);
        debug_assert_eq!(d, 0);

        let alu_out = self.r[n] ^ so;
        self.set_flags(alu_out);
        self.r.set_carry_flag(sco);
    }

    fn mov(&mut self, s: bool, n: u32, d: u32, so: u32, sco: bool) {
        debug_assert_eq!(n, 0);

        self.r[d] = so;
        if s {
            self.set_flags(self.r[d]);
            self.r.set_carry_flag(sco);
        }
    }
}
