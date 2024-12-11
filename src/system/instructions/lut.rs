use std::fmt::Display;

use crate::system::instructions::{branch, data_processing, load_store};
use crate::{bitutil::get_bits, system::cpu::CPU};

use super::{ctrl_ext, load_store_multiple, DecodedInstruction};

const LUT_SIZE: usize = 1 << 12;

static mut INSTRUCTION_LUT: Option<InstructionLut> = None;

type DecoderFn = fn(u32) -> Box<dyn DecodedInstruction>;

pub struct InstructionLut {
    decoders: [DecoderFn; LUT_SIZE],
}

impl InstructionLut {
    pub fn initialize() {
        let mut lut = Self {
            decoders: [Self::unknown_instruction_decoder; LUT_SIZE],
        };
        lut.setup_patterns();
        unsafe {
            INSTRUCTION_LUT = Some(lut);
        }
    }

    pub fn decode(instruction: u32) -> Box<dyn DecodedInstruction> {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                (lut.decoders[Self::index(instruction)])(instruction)
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
        // data processing immediate shift
        self.add_pattern("000xxxxx xxx0", data_processing::decode_arm);
        // misc
        self.add_pattern("00010xx0 xxx0", Self::unknown_instruction_decoder);
        self.add_pattern("00010x10 0000", ctrl_ext::decode_msr_arm);
        // data processing register shift
        self.add_pattern("000xxxxx 0xx1", data_processing::decode_arm);
        // misc
        self.add_pattern("00010xx0 xxx1", Self::unknown_instruction_decoder);
        self.add_pattern("00010010 0001", branch::decode_bx_arm);
        // multiplies, extra load/stores
        self.add_pattern("000xxxxx 1xx1", Self::unknown_instruction_decoder);
        // data processing immediate
        self.add_pattern("001xxxxx xxxx", data_processing::decode_arm);
        // undefined
        self.add_pattern("00110x00 1xx1", Self::unknown_instruction_decoder);
        // move immediate to status register
        self.add_pattern("00110x10 xxxx", Self::unknown_instruction_decoder);
        // load/store immediate offset
        self.add_pattern("010xxxxx xxxx", load_store::decode_arm);
        // load/store register offset
        self.add_pattern("011xxxxx xxx0", load_store::decode_arm);
        // media instructions
        self.add_pattern("011xxxxx xxx1", load_store::decode_arm);
        // undefined
        self.add_pattern("01111111 1111", Self::unknown_instruction_decoder);
        // load store multiple
        self.add_pattern("100xxxxx xxxx", load_store_multiple::decode_arm);
        // branch
        self.add_pattern("1010xxxx xxxx", branch::decode_b_arm);
        self.add_pattern("1011xxxx xxxx", branch::decode_bl_arm);
        // coprocessor load/store and double register transfers
        self.add_pattern("110xxxxx xxxx", Self::unknown_instruction_decoder);
        // coprocessor data processing
        self.add_pattern("1110xxxx xxx0", Self::unknown_instruction_decoder);
        // coprocessor register transfers
        self.add_pattern("1110xxxx xxx1", Self::unknown_instruction_decoder);
        // software interrupt
        self.add_pattern("1111xxxx xxxx", Self::unknown_instruction_decoder);
    }

    fn add_pattern(&mut self, pattern: &str, decoder: DecoderFn) {
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
            self.decoders[index] = decoder;
        }
    }

    fn unknown_instruction_decoder(instruction: u32) -> Box<dyn DecodedInstruction> {
        Box::new(UnknownInstruction(instruction))
    }
}

#[derive(Debug)]
struct UnknownInstruction(u32);

impl Display for UnknownInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown instruction: {:#08X}", self.0)
    }
}

impl DecodedInstruction for UnknownInstruction {
    fn execute(&self, _cpu: &mut CPU) {
        panic!("Tried to execute unknown instruction: {:#08X}", self.0);
    }
}
