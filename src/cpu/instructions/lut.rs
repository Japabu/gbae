use crate::{bitutil::{format_instruction, get_bit, get_bits}, cpu::CPU};

use super::dp_lut::DataProcessingLut;

macro_rules! dp_handler {
    ($operand2_decoder:ident) => {
        |lut: &InstructionLut, cpu: &mut CPU, instruction: u32| {
            InstructionLut::data_processing_handler(lut, cpu, instruction, $operand2_decoder);
        }
    };
}

type InstructionFn = fn(&InstructionLut, &mut CPU, instruction: u32);
type Operand2Fn = fn(&mut CPU, u32) -> (u32, bool);

const LUT_SIZE: usize = 1 << 12;

pub struct InstructionLut {
    table: [InstructionFn; 1 << 12],
    dp_lut: DataProcessingLut,
}

impl InstructionLut {
    pub fn new() -> Self {
        let mut lut = Self {
            table: [Self::unknown_instruction_handler; LUT_SIZE],
            dp_lut: DataProcessingLut::new(),
        };
        lut.add_pattern("001xxxxxxxx0", dp_handler!(op2_imm));
        lut.add_pattern("000xxxxxxxx0", dp_handler!(op2_imm_shift));
        lut.add_pattern("000xxxxxxxx1", dp_handler!(op2_reg_shift));
        lut
    }

    pub fn get(&self, instruction: u32) -> InstructionFn {
        // Bits 4-7 and 20-27 can be used to differentiate instructions and then index into the table
        let upper = get_bits(instruction, 20, 8);
        let lower = get_bits(instruction, 4, 4);
        let index = (upper << 4) | lower;
        self.table[index as usize]
    }

    fn add_pattern(&mut self, pattern: &str, handler: InstructionFn) {
        assert_eq!(pattern.len(), 12, "Pattern must be 12 bits long");

        // Determine which bits are fixed and which are wildcards
        let mut base_index = 0usize;
        let mut wildcard_positions = Vec::new();

        for (i, c) in pattern.chars().enumerate() {
            match c {
                '0' => {}
                '1' => base_index |= 1 << (11 - i),
                'x' => wildcard_positions.push(11 - i),
                _ => panic!("Invalid character in pattern: {}", c),
            }
        }

        // Generate all possible combinations of the wildcard bits
        let num_wildcards = wildcard_positions.len();
        let num_combinations = 1 << num_wildcards;

        for i in 0..num_combinations {
            let mut index = base_index;
            for (j, &pos) in wildcard_positions.iter().enumerate() {
                if (i & (1 << j)) != 0 {
                    index |= 1 << pos;
                } else {
                    index &= !(1 << pos);
                }
            }
            self.table[index] = handler;
        }
    }

    fn unknown_instruction_handler(&self, _cpu: &mut CPU, instruction: u32) {
        panic!("Unknown instruction: {}", format_instruction(instruction));
    }
    
    fn data_processing_handler(&self, cpu: &mut CPU, instruction: u32, operand2_decoder: Operand2Fn) {
        // set flags bit
        let s = get_bit(instruction, 20);
    
        // operand 1 register
        let n = get_bits(instruction, 16, 4);
    
        // destination register
        let d = get_bits(instruction, 12, 4);
        let (so, sco) = operand2_decoder(cpu, instruction);
    
        let opcode = get_bits(instruction, 21, 4);
        self.dp_lut.get(opcode)(cpu, s, n, d, so, sco);
    }
}

fn op2_imm(cpu: &mut CPU, instruction: u32) -> (u32, bool) {
    let immed_8 = get_bits(instruction, 0, 8);
    let rotate_imm = get_bits(instruction, 8, 4);
    let shifter_operand = immed_8.rotate_right(2 * rotate_imm);
    let carry: bool;
    if rotate_imm == 0 {
        carry = cpu.r.get_carry_flag()
    } else {
        carry = get_bit(shifter_operand, 31)
    };
    (shifter_operand, carry)
}

fn op2_imm_shift(cpu: &mut CPU, instruction: u32) -> (u32, bool) {
    panic!("op2_imm_shift");
}

fn op2_reg_shift(cpu: &mut CPU, instruction: u32) -> (u32, bool) {
    panic!("op2_reg_shift");
}
