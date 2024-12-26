use crate::system::instructions::{branch, data_processing, load_store};
use crate::{bitutil::get_bits32, system::cpu::CPU};

use super::{ctrl_ext, load_store_multiple, Condition, DecodedInstruction};

const LUT_ARM_SIZE: usize = 1 << 12;
const LUT_THUMB_SIZE: usize = 1 << 8;

static mut INSTRUCTION_LUT: Option<InstructionLut> = None;

type DecoderArmFn = fn(u32) -> Box<dyn DecodedInstruction>;
type DecoderThumbFn = fn(u16, u16) -> Box<dyn DecodedInstruction>;

enum DecoderFn {
    Arm(DecoderArmFn),
    Thumb(DecoderThumbFn),
}

pub struct InstructionLut {
    decoders_arm: [DecoderArmFn; LUT_ARM_SIZE],
    decoders_thumb: [DecoderThumbFn; LUT_THUMB_SIZE],
}

impl InstructionLut {
    pub fn initialize() {
        let mut lut = Self {
            decoders_arm: [UnknownInstruction::decode_arm; LUT_ARM_SIZE],
            decoders_thumb: [UnknownInstruction::decode_thumb; LUT_THUMB_SIZE],
        };
        lut.setup_patterns();
        unsafe {
            INSTRUCTION_LUT = Some(lut);
        }
    }

    pub fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                (lut.decoders_arm[Self::index_arm(instruction)])(instruction)
            } else {
                panic!("Instruction LUT not initialized!");
            }
        }
    }

    pub fn decode_thumb(instruction: u16, next_instruction: u16) -> Box<dyn DecodedInstruction> {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                (lut.decoders_thumb[Self::index_thumb(instruction)])(instruction, next_instruction)
            } else {
                panic!("Instruction LUT not initialized!");
            }
        }
    }

    fn index_arm(instruction: u32) -> usize {
        // Bits 4-7 and 20-27 can be used to differentiate instructions and then index into the table
        let upper = get_bits32(instruction, 20, 8);
        let lower = get_bits32(instruction, 4, 4);
        ((upper << 4) | lower) as usize
    }

    fn index_thumb(instruction: u16) -> usize {
        (instruction >> 8) as usize
    }

    fn setup_patterns(&mut self) {
        use DecoderFn::*;
        // arm
        // data processing immediate shift
        self.add_pattern("000xxxxx xxx0", Arm(data_processing::decode_arm));
        // misc
        self.add_pattern("00010xx0 xxx0", Arm(UnknownInstruction::decode_arm));
        self.add_pattern("00010x00 0000", Arm(ctrl_ext::mrs::decode_arm));
        self.add_pattern("00010x10 0000", Arm(ctrl_ext::msr::decode_arm));
        // data processing register shift
        self.add_pattern("000xxxxx 0xx1", Arm(data_processing::decode_arm));
        // misc
        self.add_pattern("00010xx0 xxx1", Arm(UnknownInstruction::decode_arm));
        self.add_pattern("00010010 0001", Arm(branch::decode_bx_arm));
        self.add_pattern("00010010 0011", Arm(branch::decode_blx_arm));
        // multiplies, extra load/stores
        self.add_pattern("000xxxxx 1xx1", Arm(load_store::decode_extra_arm));
        // data processing immediate
        self.add_pattern("001xxxxx xxxx", Arm(data_processing::decode_arm));
        // undefined
        self.add_pattern("00110x00 1xx1", Arm(UnknownInstruction::decode_arm));
        // move immediate to status register
        self.add_pattern("00110x10 xxxx", Arm(UnknownInstruction::decode_arm));
        // load/store immediate offset
        self.add_pattern("010xxxxx xxxx", Arm(load_store::decode_arm));
        // load/store register offset
        self.add_pattern("011xxxxx xxx0", Arm(load_store::decode_arm));
        // media instructions
        self.add_pattern("011xxxxx xxx1", Arm(load_store::decode_arm));
        // undefined
        self.add_pattern("01111111 1111", Arm(UnknownInstruction::decode_arm));
        // load store multiple
        self.add_pattern("100xxxxx xxxx", Arm(load_store_multiple::decode_arm));
        // branch
        self.add_pattern("1010xxxx xxxx", Arm(branch::decode_b_arm));
        self.add_pattern("1011xxxx xxxx", Arm(branch::decode_bl_arm));
        // coprocessor load/store and double register transfers
        self.add_pattern("110xxxxx xxxx", Arm(UnknownInstruction::decode_arm));
        // coprocessor data processing
        self.add_pattern("1110xxxx xxx0", Arm(UnknownInstruction::decode_arm));
        // coprocessor register transfers
        self.add_pattern("1110xxxx xxx1", Arm(UnknownInstruction::decode_arm));
        // software interrupt
        self.add_pattern("1111xxxx xxxx", Arm(UnknownInstruction::decode_arm));

        // thumb
        // shift by immediate
        self.add_pattern("000 xx x xx", Thumb(data_processing::decode_shift_imm_thumb));
        // add/subtract register
        self.add_pattern("000 11 0 xx", Thumb(data_processing::decode_add_sub_register_thumb));
        // add/subtract immediate
        self.add_pattern("000 11 1 xx", Thumb(data_processing::decode_add_sub_immediate_thumb));
        // add/subtract/compare/move immediate
        self.add_pattern("001 xxxxx", Thumb(data_processing::decode_mov_cmp_add_sub_immediate_thumb));
        // data processing register
        self.add_pattern("010000 xx", Thumb(data_processing::decode_register_thumb));
        // special data processing
        self.add_pattern("010001 xx", Thumb(data_processing::decode_special_thumb));
        // branch/exchange
        self.add_pattern("010001 11", Thumb(branch::decode_branch_exchange_thumb));
        // load from literal pool
        self.add_pattern("01001x xx", Thumb(load_store::decode_load_from_literal_pool_thumb));
        // load/store register offset
        self.add_pattern("0101 xxxx", Thumb(load_store::decode_register_offset_thumb));
        // load/store word/byte immediate offset
        self.add_pattern("011x xxxx", Thumb(UnknownInstruction::decode_thumb));
        // load/store halfword immediate offset
        self.add_pattern("1000 xxxx", Thumb(load_store::decode_halfword_thumb));
        // load/store to/from stack
        self.add_pattern("1001 xxxx", Thumb(load_store::decode_stack_thumb));
        // add sp or pc
        self.add_pattern("1010 xxxx", Thumb(UnknownInstruction::decode_thumb));
        // misc
        self.add_pattern("1011 xxxx", Thumb(UnknownInstruction::decode_thumb));
        self.add_pattern("1011 0000", Thumb(data_processing::decode_adjust_sp_thumb));
        self.add_pattern("1011 010x", Thumb(load_store_multiple::decode_push_thumb));
        self.add_pattern("1011 110x", Thumb(load_store_multiple::decode_pop_thumb));
        // load/store multiple
        self.add_pattern("1100 xxxx", Thumb(UnknownInstruction::decode_thumb));
        // conditional branch
        self.add_pattern("1101 xxxx", Thumb(branch::decode_conditional_branch_thumb));
        // undefined
        self.add_pattern("1101 1110", Thumb(UnknownInstruction::decode_thumb));
        // software interrupt
        self.add_pattern("1101 1111", Thumb(UnknownInstruction::decode_thumb));
        // unconditional branch
        self.add_pattern("11100 xxx", Thumb(branch::decode_unconditional_branch_thumb));
        // blx suffix
        self.add_pattern("11101 xxx", Thumb(UnknownInstruction::decode_thumb));
        // bl/blx prefix
        self.add_pattern("11110 xxx", Thumb(branch::decode_bl_thumb));
        // bl suffix
        self.add_pattern("11111 xxx", Thumb(UnknownInstruction::decode_thumb));
    }

    fn add_pattern(&mut self, pattern: &str, decoder: DecoderFn) {
        use DecoderFn::*;

        let pattern = pattern.to_string().replace(" ", "");
        let pattern_len = match decoder {
            Arm(_) => 12,
            Thumb(_) => 8,
        };

        if pattern.len() != pattern_len {
            panic!("Pattern must be {} bits long", pattern_len);
        }

        // Determine which bits are fixed and which are wildcards
        let mut base_index = 0usize;
        let mut wildcard_positions = Vec::new();

        for (i, c) in pattern.chars().enumerate() {
            match c {
                '0' => {}
                '1' => base_index |= 1 << (pattern_len - 1 - i),
                'x' => wildcard_positions.push(pattern_len - 1 - i),
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

            match decoder {
                Arm(decoder) => self.decoders_arm[index] = decoder,
                Thumb(decoder) => self.decoders_thumb[index] = decoder,
            };
        }
    }
}

#[derive(Debug)]
enum UnknownInstruction {
    Arm(u32),
    Thumb(u16, u16),
}
impl UnknownInstruction {
    fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
        Box::new(UnknownInstruction::Arm(instruction))
    }
    fn decode_thumb(instruction: u16, next_instruction: u16) -> Box<dyn DecodedInstruction> {
        Box::new(UnknownInstruction::Thumb(instruction, next_instruction))
    }
}
impl DecodedInstruction for UnknownInstruction {
    fn execute(&self, _cpu: &mut CPU) {
        match self {
            UnknownInstruction::Arm(instruction) => panic!("Tried to execute unknown arm instruction: {:#08X}", instruction),
            UnknownInstruction::Thumb(instruction, next_instruction) => panic!("Tried to execute unknown thumb instruction: {:#04X}, next: {:#04X}", instruction, next_instruction),
        }
    }

    fn disassemble(&self, _cond: Condition, _base_address: u32) -> String {
        match self {
            UnknownInstruction::Arm(instruction) => format!("???: {:#08X}", instruction),
            UnknownInstruction::Thumb(instruction, _) => format!("???: {:#04X}", instruction),
        }
    }
}
