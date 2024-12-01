use crate::cpu::instructions::{branch, ctrl_ext, dp, ls};
use crate::{
    bitutil::{format_instruction, get_bits},
    cpu::CPU,
};

macro_rules! add_dp_patterns {
    ($lut:expr, $($opcode:expr => $handler:expr),* $(,)?) => {
        $(
            $lut.add_pattern(&format!("001{}xxxxx", $opcode), dp_handler!(dp::op2_imm, $handler));
            $lut.add_pattern(&format!("000{}xxxx0", $opcode), dp_handler!(dp::op2_imm_shift, $handler));
            $lut.add_pattern(&format!("000{}xxxx1", $opcode), dp_handler!(dp::op2_reg_shift, $handler));
        )*
    };
}

macro_rules! dp_handler {
    ($operand2_decoder:expr, $dp_handler:expr) => {
        |cpu: &mut CPU, instruction: u32| {
            dp::handler(
                cpu,
                instruction,
                $operand2_decoder,
                $dp_handler,
            );
        }
    };
}

macro_rules! ls_handler {
    ($adress_decoder:expr, $ls_handler:expr) => {
        |cpu: &mut CPU, instruction: u32| {
            ls::handler(
                cpu,
                instruction,
                $adress_decoder,
                $ls_handler,
            );
        }
    };
}

type InstructionFn = fn(&mut CPU, instruction: u32);

const LUT_SIZE: usize = 1 << 12;

static mut INSTRUCTION_LUT: Option<InstructionLut> = None;

pub struct InstructionLut {
    table: [InstructionFn; LUT_SIZE],
}

impl InstructionLut {
    pub fn initialize() {
        let mut lut = Self {
            table: [Self::unknown_instruction_handler; LUT_SIZE],
        };
        lut.setup_patterns();
        unsafe {
            INSTRUCTION_LUT = Some(lut);
        }
    }

    pub fn get(instruction: u32) -> InstructionFn {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                // Bits 4-7 and 20-27 can be used to differentiate instructions and then index into the table
                let upper = get_bits(instruction, 20, 8);
                let lower = get_bits(instruction, 4, 4);
                let index = (upper << 4) | lower;
                lut.table[index as usize]
            } else {
                panic!("Instruction LUT not initialized!");
            }
        }
    }

    fn setup_patterns(&mut self) {
        add_dp_patterns!(
            self,
            "0000" => dp::and,
            "0100" => dp::add,
            "1101" => dp::mov,
        );
        self.add_pattern("1010xxxxxxxx", branch::imm);
        self.add_pattern("00010x100000", ctrl_ext::msr_reg);
        self.add_pattern("010xxxx1xxxx", ls_handler!(ls::addr_imm, ls::ldr));
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

    fn unknown_instruction_handler(_cpu: &mut CPU, instruction: u32) {
        panic!("Unknown instruction: {}", format_instruction(instruction));
    }
}
