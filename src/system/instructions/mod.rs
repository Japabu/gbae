use std::fmt::{Debug, Display};

use super::{cpu::CPU, memory::Memory};
use crate::bitutil::{get_bit, get_bits32};

mod branch;
mod ctrl_ext;
mod data_processing;
mod load_store;
mod load_store_multiple;
mod multiply;
mod swi;
pub mod lut;

pub type ArmHandler = fn(&mut CPU, &mut Memory, u32);
pub type ThumbHandler = fn(&mut CPU, &mut Memory, u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    DataProcessing(data_processing::DataProcessing),
    Multiply(multiply::Multiply),
    LoadStore(load_store::LoadStore),
    Swap(load_store::Swap),
    LoadStoreMultiple(load_store_multiple::LoadStoreMultiple),
    Branch(branch::Branch),
    BranchExchange(branch::BranchExchange),
    BranchLinkPrefix(branch::BranchLinkPrefix),
    BranchLinkSuffix(branch::BranchLinkSuffix),
    Mrs(ctrl_ext::Mrs),
    Msr(ctrl_ext::Msr),
    SoftwareInterrupt(swi::SoftwareInterrupt),
    Unknown(u32),
}

const fn pattern_bits(pattern: &str, ones: bool) -> u32 {
    let bytes = pattern.as_bytes();
    let mut length = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b' ' {
            length += 1;
        }
        i += 1;
    }
    let mut result = 0;
    let mut position = length;
    i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' => {}
            b'0' | b'1' | b'x' => {
                position -= 1;
                let set = if ones { bytes[i] == b'1' } else { bytes[i] != b'x' };
                if set {
                    result |= 1 << position;
                }
            }
            _ => panic!("Pattern may only contain 0, 1, x and spaces"),
        }
        i += 1;
    }
    result
}

pub const fn pattern_mask(pattern: &str) -> u32 {
    pattern_bits(pattern, false)
}

pub const fn pattern_value(pattern: &str) -> u32 {
    pattern_bits(pattern, true)
}

macro_rules! match_pattern {
    ($bits:expr, { $($pattern:literal => $result:expr,)* _ => $default:expr $(,)? }) => {{
        let bits = $bits;
        match () {
            $(_ if bits & const { pattern_mask($pattern) } == const { pattern_value($pattern) } => $result,)*
            _ => $default,
        }
    }};
}

impl Instruction {
    #[inline(always)]
    pub fn decode_arm(instruction: u32) -> Instruction {
        match_pattern!(lut::index_arm(instruction) as u32, {
            "00010x00 0000" => ctrl_ext::decode_mrs_arm(instruction),
            "00010x10 0000" => ctrl_ext::decode_msr_arm(instruction),
            "00010010 0001" => branch::decode_bx_arm(instruction),
            "000000xx 1001" => multiply::decode_arm(instruction),
            "00001xxx 1001" => multiply::decode_arm(instruction),
            "00010x00 1001" => load_store::decode_swap_arm(instruction),
            "000xxxxx 1xx1" => load_store::decode_extra_arm(instruction),
            "00010xx0 xxxx" => Instruction::Unknown(instruction),
            "000xxxxx xxxx" => data_processing::decode_arm(instruction),
            "00110x00 xxxx" => Instruction::Unknown(instruction),
            "00110x10 xxxx" => ctrl_ext::decode_msr_arm(instruction),
            "001xxxxx xxxx" => data_processing::decode_arm(instruction),
            "010xxxxx xxxx" => load_store::decode_arm(instruction),
            "011xxxxx xxx0" => load_store::decode_arm(instruction),
            "100xxxxx xxxx" => load_store_multiple::decode_arm(instruction),
            "1010xxxx xxxx" => branch::decode_b_arm(instruction),
            "1011xxxx xxxx" => branch::decode_bl_arm(instruction),
            "1111xxxx xxxx" => swi::decode_arm(instruction),
            _ => Instruction::Unknown(instruction),
        })
    }

    #[inline(always)]
    pub fn decode_thumb(instruction: u16) -> Instruction {
        match_pattern!((instruction >> 8) as u32, {
            "000 11 0 xx" => data_processing::decode_add_sub_register_thumb(instruction),
            "000 11 1 xx" => data_processing::decode_add_sub_immediate_thumb(instruction),
            "000 xx x xx" => data_processing::decode_shift_imm_thumb(instruction),
            "001 xxxxx" => data_processing::decode_mov_cmp_add_sub_immediate_thumb(instruction),
            "010000 xx" => data_processing::decode_register_thumb(instruction),
            "010001 11" => branch::decode_branch_exchange_thumb(instruction),
            "010001 xx" => data_processing::decode_special_thumb(instruction),
            "01001 xxx" => load_store::decode_load_from_literal_pool_thumb(instruction),
            "0101 xxxx" => load_store::decode_register_offset_thumb(instruction),
            "011x xxxx" => load_store::decode_word_byte_thumb(instruction),
            "1000 xxxx" => load_store::decode_halfword_thumb(instruction),
            "1001 xxxx" => load_store::decode_stack_thumb(instruction),
            "1010 xxxx" => data_processing::decode_add_sp_pc_thumb(instruction),
            "1011 0000" => data_processing::decode_adjust_sp_thumb(instruction),
            "1011 010x" => load_store_multiple::decode_push_thumb(instruction),
            "1011 110x" => load_store_multiple::decode_pop_thumb(instruction),
            "1100 xxxx" => load_store_multiple::decode_ldm_stm_thumb(instruction),
            "1101 1110" => Instruction::Unknown(instruction as u32),
            "1101 1111" => swi::decode_thumb(instruction),
            "1101 xxxx" => branch::decode_conditional_branch_thumb(instruction),
            "11100 xxx" => branch::decode_unconditional_branch_thumb(instruction),
            "11110 xxx" => branch::decode_bl_prefix_thumb(instruction),
            "11111 xxx" => branch::decode_bl_suffix_thumb(instruction),
            _ => Instruction::Unknown(instruction as u32),
        })
    }

    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        match self {
            Instruction::DataProcessing(instruction) => instruction.execute(cpu, mem),
            Instruction::Multiply(instruction) => instruction.execute(cpu, mem),
            Instruction::LoadStore(instruction) => instruction.execute(cpu, mem),
            Instruction::Swap(instruction) => instruction.execute(cpu, mem),
            Instruction::LoadStoreMultiple(instruction) => instruction.execute(cpu, mem),
            Instruction::Branch(instruction) => instruction.execute(cpu, mem),
            Instruction::BranchExchange(instruction) => instruction.execute(cpu, mem),
            Instruction::BranchLinkPrefix(instruction) => instruction.execute(cpu, mem),
            Instruction::BranchLinkSuffix(instruction) => instruction.execute(cpu, mem),
            Instruction::Mrs(instruction) => instruction.execute(cpu, mem),
            Instruction::Msr(instruction) => instruction.execute(cpu, mem),
            Instruction::SoftwareInterrupt(instruction) => instruction.execute(cpu, mem),
            Instruction::Unknown(encoding) => panic!("Tried to execute unknown instruction {:08X} at {:08X}", encoding, cpu.curr_instruction_address_from_execution_stage()),
        }
    }

    pub fn disassemble(self, cond: Condition, address: u32) -> String {
        match self {
            Instruction::DataProcessing(instruction) => instruction.disassemble(cond),
            Instruction::Multiply(instruction) => instruction.disassemble(cond),
            Instruction::LoadStore(instruction) => instruction.disassemble(cond),
            Instruction::Swap(instruction) => instruction.disassemble(cond),
            Instruction::LoadStoreMultiple(instruction) => instruction.disassemble(cond),
            Instruction::Branch(instruction) => instruction.disassemble(cond, address),
            Instruction::BranchExchange(instruction) => instruction.disassemble(cond),
            Instruction::BranchLinkPrefix(instruction) => instruction.disassemble(address),
            Instruction::BranchLinkSuffix(instruction) => instruction.disassemble(),
            Instruction::Mrs(instruction) => instruction.disassemble(cond),
            Instruction::Msr(instruction) => instruction.disassemble(cond),
            Instruction::SoftwareInterrupt(instruction) => instruction.disassemble(cond),
            Instruction::Unknown(encoding) => format!("???: {:08X}", encoding),
        }
    }
}

pub fn format_instruction_arm(instruction: u32, base_address: u32) -> String {
    format!(
        "{} ({:08X})\n\
            Bit Index:   27 26 25 24 23 22 21 20   07 06 05 04\n\
            Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<4} {:<2} {:<2} {:<2} {:<2}",
        Instruction::decode_arm(instruction).disassemble(Condition::decode_arm(instruction), base_address),
        instruction,
        get_bit(instruction, 27) as u32,
        get_bit(instruction, 26) as u32,
        get_bit(instruction, 25) as u32,
        get_bit(instruction, 24) as u32,
        get_bit(instruction, 23) as u32,
        get_bit(instruction, 22) as u32,
        get_bit(instruction, 21) as u32,
        get_bit(instruction, 20) as u32,
        get_bit(instruction, 7) as u32,
        get_bit(instruction, 6) as u32,
        get_bit(instruction, 5) as u32,
        get_bit(instruction, 4) as u32,
    )
}

pub fn format_instruction_thumb(instruction: u16, next_instruction: u16, base_address: u32) -> String {
    let decoded = Instruction::decode_thumb(instruction);
    let text = match (decoded, Instruction::decode_thumb(next_instruction)) {
        (Instruction::BranchLinkPrefix(prefix), Instruction::BranchLinkSuffix(suffix)) => format!("BL #{:08X}", prefix.target(suffix, base_address)),
        _ => decoded.disassemble(Condition::AL, base_address),
    };
    format!(
        "{} ({:04X}, next: {:04X})\n\
            Bit Index:   15 14 13 12 11 10 09 08 07 06 05 04 03 02 01 00\n\
            Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2}",
        text,
        instruction,
        next_instruction,
        get_bit(instruction as u32, 15) as u32,
        get_bit(instruction as u32, 14) as u32,
        get_bit(instruction as u32, 13) as u32,
        get_bit(instruction as u32, 12) as u32,
        get_bit(instruction as u32, 11) as u32,
        get_bit(instruction as u32, 10) as u32,
        get_bit(instruction as u32, 9) as u32,
        get_bit(instruction as u32, 8) as u32,
        get_bit(instruction as u32, 7) as u32,
        get_bit(instruction as u32, 6) as u32,
        get_bit(instruction as u32, 5) as u32,
        get_bit(instruction as u32, 4) as u32,
        get_bit(instruction as u32, 3) as u32,
        get_bit(instruction as u32, 2) as u32,
        get_bit(instruction as u32, 1) as u32,
        get_bit(instruction as u32, 0) as u32,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    EQ,
    NE,
    CS,
    CC,
    MI,
    PL,
    VS,
    VC,
    HI,
    LS,
    GE,
    LT,
    GT,
    LE,
    AL,
    NV,
}

impl Condition {
    pub const fn parse(cond: u8) -> Condition {
        match cond {
            0b0000 => Condition::EQ,
            0b0001 => Condition::NE,
            0b0010 => Condition::CS,
            0b0011 => Condition::CC,
            0b0100 => Condition::MI,
            0b0101 => Condition::PL,
            0b0110 => Condition::VS,
            0b0111 => Condition::VC,
            0b1000 => Condition::HI,
            0b1001 => Condition::LS,
            0b1010 => Condition::GE,
            0b1011 => Condition::LT,
            0b1100 => Condition::GT,
            0b1101 => Condition::LE,
            0b1110 => Condition::AL,
            _ => Condition::NV,
        }
    }

    pub const fn decode_arm(instruction: u32) -> Condition {
        Condition::parse(get_bits32(instruction, 28, 4) as u8)
    }

    pub const fn passes(self, nzcv: u32) -> bool {
        let n = nzcv & 0b1000 != 0;
        let z = nzcv & 0b0100 != 0;
        let c = nzcv & 0b0010 != 0;
        let v = nzcv & 0b0001 != 0;
        match self {
            Condition::EQ => z,
            Condition::NE => !z,
            Condition::CS => c,
            Condition::CC => !c,
            Condition::MI => n,
            Condition::PL => !n,
            Condition::VS => v,
            Condition::VC => !v,
            Condition::HI => c && !z,
            Condition::LS => !c || z,
            Condition::GE => n == v,
            Condition::LT => n != v,
            Condition::GT => !z && n == v,
            Condition::LE => z || n != v,
            Condition::AL => true,
            Condition::NV => false,
        }
    }

    #[inline(always)]
    pub fn check(self, cpu: &CPU) -> bool {
        self.passes(cpu.get_cpsr() >> 28)
    }
}

static CONDITION_LUT: [u16; 16] = build_condition_lut();

const fn build_condition_lut() -> [u16; 16] {
    let mut lut = [0u16; 16];
    let mut cond = 0;
    while cond < 16 {
        let mut flags = 0;
        while flags < 16 {
            if Condition::parse(cond as u8).passes(flags) {
                lut[cond] |= 1 << flags;
            }
            flags += 1;
        }
        cond += 1;
    }
    lut
}

#[inline(always)]
pub fn condition_passed(cond: u32, cpsr: u32) -> bool {
    CONDITION_LUT[cond as usize] >> (cpsr >> 28) & 1 != 0
}

impl Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Condition::AL => Ok(()),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_arm() {
        assert_eq!(Condition::decode_arm(0b0000_0000_0000_0000_0000_0000_0000_0000), Condition::EQ);
        assert_eq!(Condition::decode_arm(0b0001_0000_0000_0000_0000_0000_0000_0000), Condition::NE);
        assert_eq!(Condition::decode_arm(0b0010_0000_0000_0000_0000_0000_0000_0000), Condition::CS);
        assert_eq!(Condition::decode_arm(0b0011_0000_0000_0000_0000_0000_0000_0000), Condition::CC);
        assert_eq!(Condition::decode_arm(0b0100_0000_0000_0000_0000_0000_0000_0000), Condition::MI);
        assert_eq!(Condition::decode_arm(0b0101_0000_0000_0000_0000_0000_0000_0000), Condition::PL);
        assert_eq!(Condition::decode_arm(0b0110_0000_0000_0000_0000_0000_0000_0000), Condition::VS);
        assert_eq!(Condition::decode_arm(0b0111_0000_0000_0000_0000_0000_0000_0000), Condition::VC);
        assert_eq!(Condition::decode_arm(0b1000_0000_0000_0000_0000_0000_0000_0000), Condition::HI);
        assert_eq!(Condition::decode_arm(0b1001_0000_0000_0000_0000_0000_0000_0000), Condition::LS);
        assert_eq!(Condition::decode_arm(0b1010_0000_0000_0000_0000_0000_0000_0000), Condition::GE);
        assert_eq!(Condition::decode_arm(0b1011_0000_0000_0000_0000_0000_0000_0000), Condition::LT);
        assert_eq!(Condition::decode_arm(0b1100_0000_0000_0000_0000_0000_0000_0000), Condition::GT);
        assert_eq!(Condition::decode_arm(0b1101_0000_0000_0000_0000_0000_0000_0000), Condition::LE);
        assert_eq!(Condition::decode_arm(0b1110_0000_0000_0000_0000_0000_0000_0000), Condition::AL);
        assert_eq!(Condition::decode_arm(0x39_00_00_00), Condition::CC);
    }

    #[test]
    fn test_condition_lut_matches_passes() {
        for cond in 0..16u32 {
            for flags in 0..16u32 {
                assert_eq!(condition_passed(cond, flags << 28), Condition::parse(cond as u8).passes(flags));
            }
        }
    }

    #[test]
    fn test_arm_decoder_covers_families() {
        assert!(matches!(Instruction::decode_arm(0xE1A0_1000), Instruction::DataProcessing(_)));
        assert!(matches!(Instruction::decode_arm(0xE5C3_3208), Instruction::LoadStore(_)));
        assert!(matches!(Instruction::decode_arm(0xE1D0_00B0), Instruction::LoadStore(_)));
        assert!(matches!(Instruction::decode_arm(0xE8BD_8000), Instruction::LoadStoreMultiple(_)));
        assert!(matches!(Instruction::decode_arm(0xEA00_0000), Instruction::Branch(_)));
        assert!(matches!(Instruction::decode_arm(0xE12F_FF11), Instruction::BranchExchange(_)));
        assert!(matches!(Instruction::decode_arm(0xE10F_0000), Instruction::Mrs(_)));
        assert!(matches!(Instruction::decode_arm(0xE129_F000), Instruction::Msr(_)));
        assert!(matches!(Instruction::decode_arm(0xE321_F0DF), Instruction::Msr(_)));
        assert!(matches!(Instruction::decode_arm(0xE000_0091), Instruction::Multiply(_)));
        assert!(matches!(Instruction::decode_arm(0xE080_0091), Instruction::Multiply(_)));
        assert!(matches!(Instruction::decode_arm(0xE100_0090), Instruction::Swap(_)));
        assert!(matches!(Instruction::decode_arm(0xEF00_0000), Instruction::SoftwareInterrupt(_)));
        assert!(matches!(Instruction::decode_arm(0xEE00_0000), Instruction::Unknown(_)));
        assert!(matches!(Instruction::decode_arm(0xE7F0_00F0), Instruction::Unknown(_)));
    }

    #[test]
    fn test_thumb_decoder_covers_formats() {
        assert!(matches!(Instruction::decode_thumb(0x0040), Instruction::DataProcessing(_)));
        assert!(matches!(Instruction::decode_thumb(0x1C40), Instruction::DataProcessing(_)));
        assert!(matches!(Instruction::decode_thumb(0x2001), Instruction::DataProcessing(_)));
        assert!(matches!(Instruction::decode_thumb(0x4340), Instruction::Multiply(_)));
        assert!(matches!(Instruction::decode_thumb(0x4770), Instruction::BranchExchange(_)));
        assert!(matches!(Instruction::decode_thumb(0x4801), Instruction::LoadStore(_)));
        assert!(matches!(Instruction::decode_thumb(0x5800), Instruction::LoadStore(_)));
        assert!(matches!(Instruction::decode_thumb(0x8021), Instruction::LoadStore(_)));
        assert!(matches!(Instruction::decode_thumb(0xA001), Instruction::DataProcessing(_)));
        assert!(matches!(Instruction::decode_thumb(0xB081), Instruction::DataProcessing(_)));
        assert!(matches!(Instruction::decode_thumb(0xB500), Instruction::LoadStoreMultiple(_)));
        assert!(matches!(Instruction::decode_thumb(0xC9A0), Instruction::LoadStoreMultiple(_)));
        assert!(matches!(Instruction::decode_thumb(0xD0FE), Instruction::Branch(_)));
        assert!(matches!(Instruction::decode_thumb(0xDF05), Instruction::SoftwareInterrupt(_)));
        assert!(matches!(Instruction::decode_thumb(0xE7FE), Instruction::Branch(_)));
        assert!(matches!(Instruction::decode_thumb(0xF000), Instruction::BranchLinkPrefix(_)));
        assert!(matches!(Instruction::decode_thumb(0xF800), Instruction::BranchLinkSuffix(_)));
        assert!(matches!(Instruction::decode_thumb(0xBE00), Instruction::Unknown(_)));
        assert!(matches!(Instruction::decode_thumb(0xDE00), Instruction::Unknown(_)));
    }

    #[test]
    fn test_pattern_parsing() {
        assert_eq!(pattern_mask("000xxxxx xxx0"), 0b1110_0000_0001);
        assert_eq!(pattern_value("000xxxxx xxx0"), 0);
        assert_eq!(pattern_mask("00010010 0001"), 0xFFF);
        assert_eq!(pattern_value("00010010 0001"), 0x121);
        assert_eq!(pattern_mask("1011 010x"), 0xFE);
        assert_eq!(pattern_value("1011 010x"), 0xB4);
    }

    #[test]
    fn test_thumb_bl_pair_is_formatted_as_one_branch() {
        assert!(format_instruction_thumb(0xF000, 0xF802, 0x0800_0000).starts_with("BL #08000008"));
    }
}
