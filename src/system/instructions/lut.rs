use crate::system::instructions::{branch, ctrl_ext, dp, format_instruction, ls};
use crate::{bitutil::get_bits, system::cpu::CPU};

macro_rules! add_dp_patterns {
    ($lut:expr, $($opcode:expr => ($handler:expr, $decoder:expr)),* $(,)?) => {
        $(
            $lut.add_pattern(&format!("001{}xxxxx", $opcode), dp_handler!(dp::op2_imm, $handler), $decoder);
            $lut.add_pattern(&format!("000{}xxxx0", $opcode), dp_handler!(dp::op2_imm_shift, $handler), $decoder);
            $lut.add_pattern(&format!("000{}xxxx1", $opcode), dp_handler!(dp::op2_reg_shift, $handler), $decoder);
        )*
    };
}

macro_rules! dp_handler {
    ($operand2_decoder:expr, $dp_handler:expr) => {
        |cpu: &mut CPU, instruction: u32| {
            dp::handler(cpu, instruction, $operand2_decoder, $dp_handler)
        }
    };
}

macro_rules! ls_handler {
    ($adress_decoder:expr, $ls_handler:expr) => {
        |cpu: &mut CPU, instruction: u32| {
            ls::handler(cpu, instruction, $adress_decoder, $ls_handler)
        }
    };
}

type InstructionHandlerFn = fn(&mut CPU, instruction: u32);

const LUT_SIZE: usize = 1 << 12;

static mut INSTRUCTION_LUT: Option<InstructionLut> = None;

pub struct InstructionLut {
    handlers: [InstructionHandlerFn; LUT_SIZE],
    decoders: [&'static str; LUT_SIZE],
}

impl InstructionLut {
    pub fn initialize() {
        let mut lut = Self {
            handlers: [Self::unknown_instruction_handler; LUT_SIZE],
            decoders: ["???"; LUT_SIZE],
        };
        lut.setup_patterns();
        unsafe {
            INSTRUCTION_LUT = Some(lut);
        }
    }

    pub fn get_handler(instruction: u32) -> InstructionHandlerFn {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                lut.handlers[Self::index(instruction)]
            } else {
                panic!("Instruction LUT not initialized!");
            }
        }
    }

    pub fn get_decoder(instruction: u32) -> &'static str {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                lut.decoders[Self::index(instruction)]
            } else {
                panic!("Instruction LUT not initialized!");
            }
        }
    }


    fn index(instruction: u32) -> usize {
        // Bits 4-7 and 20-27 can be used to differentiate instructions and then index into the table
        let upper = get_bits(instruction, 20, 8);
        let lower = get_bits(instruction, 4, 4);
        ((upper << 4) | lower) as usize
    }

    fn setup_patterns(&mut self) {
        add_dp_patterns!(
            self,
            "0000" => (dp::and, "and"),
            "0100" => (dp::add, "add"),
            "1101" => (dp::mov, "mov"),
        );
        self.add_pattern("1010xxxx xxxx", branch::imm, "b");
        self.add_pattern("00010x10 0000", ctrl_ext::msr_reg, "msr");
        self.add_pattern("010xxxx1 xxxx", ls_handler!(ls::addr_imm, ls::ldr), "ldr");
        self.add_pattern("010xxxx0 xxxx", ls_handler!(ls::addr_imm, ls::str), "str");
    }

    fn add_pattern(&mut self, pattern: &str, handler: InstructionHandlerFn, decoder: &'static str) {
        let pattern = pattern.to_string().replace(" ", "");
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
            self.handlers[index] = handler;
            self.decoders[index] = decoder;
        }
    }

    fn unknown_instruction_handler(_cpu: &mut CPU, instruction: u32) {
        panic!("Unknown instruction: {}", format_instruction(instruction));
    }
}