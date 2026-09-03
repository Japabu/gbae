use std::fmt::Display;

use crate::bits::Bits;

use super::{
    cpu::{Psr, CPU},
    memory::Memory,
};

pub mod branch;
pub mod ctrl_ext;
pub mod data_processing;
pub mod load_store;
pub mod load_store_multiple;
pub mod lut;
pub mod multiply;
pub mod swi;

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
    pub fn decode_arm(word: u32) -> Instruction {
        match_pattern!(lut::index_arm(word) as u32, {
            "00010x00 0000" => ctrl_ext::decode_mrs_arm(word),
            "00010x10 0000" => ctrl_ext::decode_msr_arm(word),
            "00010010 0001" => branch::decode_bx_arm(word),
            "000000xx 1001" => multiply::decode_arm(word),
            "00001xxx 1001" => multiply::decode_arm(word),
            "00010x00 1001" => load_store::decode_swap_arm(word),
            "000xxxxx 1xx1" => load_store::decode_extra_arm(word),
            "00010xx0 xxxx" => Instruction::Unknown(word),
            "000xxxxx xxxx" => data_processing::decode_arm(word),
            "00110x00 xxxx" => Instruction::Unknown(word),
            "00110x10 xxxx" => ctrl_ext::decode_msr_arm(word),
            "001xxxxx xxxx" => data_processing::decode_arm(word),
            "010xxxxx xxxx" => load_store::decode_arm(word),
            "011xxxxx xxx0" => load_store::decode_arm(word),
            "100xxxxx xxxx" => load_store_multiple::decode_arm(word),
            "1010xxxx xxxx" => branch::decode_b_arm(word),
            "1011xxxx xxxx" => branch::decode_bl_arm(word),
            "1111xxxx xxxx" => swi::decode_arm(word),
            _ => Instruction::Unknown(word),
        })
    }

    #[inline(always)]
    pub fn decode_thumb(word: u16) -> Instruction {
        match_pattern!(u32::from(word).bits(8..), {
            "000 11 0 xx" => data_processing::decode_add_sub_register_thumb(word),
            "000 11 1 xx" => data_processing::decode_add_sub_immediate_thumb(word),
            "000 xx x xx" => data_processing::decode_shift_imm_thumb(word),
            "001 xxxxx" => data_processing::decode_mov_cmp_add_sub_immediate_thumb(word),
            "010000 xx" => data_processing::decode_register_thumb(word),
            "010001 11" => branch::decode_branch_exchange_thumb(word),
            "010001 xx" => data_processing::decode_special_thumb(word),
            "01001 xxx" => load_store::decode_load_from_literal_pool_thumb(word),
            "0101 xxxx" => load_store::decode_register_offset_thumb(word),
            "011x xxxx" => load_store::decode_word_byte_thumb(word),
            "1000 xxxx" => load_store::decode_halfword_thumb(word),
            "1001 xxxx" => load_store::decode_stack_thumb(word),
            "1010 xxxx" => data_processing::decode_add_sp_pc_thumb(word),
            "1011 0000" => data_processing::decode_adjust_sp_thumb(word),
            "1011 010x" => load_store_multiple::decode_push_thumb(word),
            "1011 110x" => load_store_multiple::decode_pop_thumb(word),
            "1100 xxxx" => load_store_multiple::decode_ldm_stm_thumb(word),
            "1101 1110" => Instruction::Unknown(u32::from(word)),
            "1101 1111" => swi::decode_thumb(word),
            "1101 xxxx" => branch::decode_conditional_branch_thumb(word),
            "11100 xxx" => branch::decode_unconditional_branch_thumb(word),
            "11110 xxx" => branch::decode_bl_prefix_thumb(word),
            "11111 xxx" => branch::decode_bl_suffix_thumb(word),
            _ => Instruction::Unknown(u32::from(word)),
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
            Instruction::Unknown(word) => panic!("Tried to execute unknown instruction {:08X} at {:08X}", word, cpu.pc()),
        }
    }

    pub fn encode_arm(self, cond: Condition) -> Option<u32> {
        let mut cond = cond;
        let word = match self {
            Instruction::DataProcessing(instruction) => instruction.encode_arm()?,
            Instruction::Multiply(instruction) => instruction.encode_arm(),
            Instruction::LoadStore(instruction) => instruction.encode_arm()?,
            Instruction::Swap(instruction) => instruction.encode_arm(),
            Instruction::LoadStoreMultiple(instruction) => instruction.encode_arm(),
            Instruction::Branch(instruction) => {
                if instruction.cond != Condition::AL {
                    if cond != Condition::AL && cond != instruction.cond {
                        return None;
                    }
                    cond = instruction.cond;
                }
                instruction.encode_arm()?
            }
            Instruction::BranchExchange(instruction) => instruction.encode_arm(),
            Instruction::Mrs(instruction) => instruction.encode_arm(),
            Instruction::Msr(instruction) => instruction.encode_arm()?,
            Instruction::SoftwareInterrupt(instruction) => instruction.encode_arm()?,
            Instruction::BranchLinkPrefix(_) | Instruction::BranchLinkSuffix(_) | Instruction::Unknown(_) => return None,
        };
        Some(word | cond.bits() << 28)
    }

    pub fn encode_thumb(self) -> Option<u16> {
        match self {
            Instruction::DataProcessing(instruction) => instruction.encode_thumb(),
            Instruction::Multiply(instruction) => instruction.encode_thumb(),
            Instruction::LoadStore(instruction) => instruction.encode_thumb(),
            Instruction::LoadStoreMultiple(instruction) => instruction.encode_thumb(),
            Instruction::Branch(instruction) => instruction.encode_thumb(),
            Instruction::BranchExchange(instruction) => instruction.encode_thumb(),
            Instruction::BranchLinkPrefix(instruction) => instruction.encode_thumb(),
            Instruction::BranchLinkSuffix(instruction) => instruction.encode_thumb(),
            Instruction::SoftwareInterrupt(instruction) => instruction.encode_thumb(),
            Instruction::Swap(_) | Instruction::Mrs(_) | Instruction::Msr(_) | Instruction::Unknown(_) => None,
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
            Instruction::Unknown(word) => format!("???: {:08X}", word),
        }
    }
}

fn bit_table(word: u32, groups: &[&[u32]]) -> String {
    let row = |cell: &dyn Fn(u32) -> String| {
        groups
            .iter()
            .map(|group| group.iter().map(|index| cell(*index)).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("   ")
    };
    format!(
        "Bit Index:   {}\nValues:      {}",
        row(&|index| format!("{:02}", index)),
        row(&|index| format!("{:<2}", u32::from(word.bit(index))))
    )
}

pub fn format_instruction_arm(word: u32, address: u32) -> String {
    let text = Instruction::decode_arm(word).disassemble(Condition::decode_arm(word), address);
    format!("{} ({:08X})\n{}", text, word, bit_table(word, &[&[27, 26, 25, 24, 23, 22, 21, 20], &[7, 6, 5, 4]]))
}

pub fn format_instruction_thumb(word: u16, next_word: u16, address: u32) -> String {
    let decoded = Instruction::decode_thumb(word);
    let text = match (decoded, Instruction::decode_thumb(next_word)) {
        (Instruction::BranchLinkPrefix(prefix), Instruction::BranchLinkSuffix(suffix)) => format!("BL #{:08X}", prefix.target(suffix, address)),
        _ => decoded.disassemble(Condition::AL, address),
    };
    let indices: Vec<u32> = (0..16).rev().collect();
    format!("{} ({:04X}, next: {:04X})\n{}", text, word, next_word, bit_table(u32::from(word), &[&indices]))
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
    pub const fn from_bits(bits: u32) -> Condition {
        match bits & 0xF {
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

    pub const fn bits(self) -> u32 {
        self as u32
    }

    pub fn decode_arm(word: u32) -> Condition {
        Condition::from_bits(word.bits(28..))
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
        self.passes(cpu.cpsr().flags())
    }
}

static CONDITION_LUT: [u16; 16] = build_condition_lut();

const fn build_condition_lut() -> [u16; 16] {
    let mut lut = [0u16; 16];
    let mut cond = 0;
    while cond < 16 {
        let mut flags = 0;
        while flags < 16 {
            if Condition::from_bits(cond as u32).passes(flags) {
                lut[cond] |= 1 << flags;
            }
            flags += 1;
        }
        cond += 1;
    }
    lut
}

#[inline(always)]
pub fn condition_passed(cond: u32, psr: Psr) -> bool {
    CONDITION_LUT[cond as usize].bit(psr.flags())
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
        assert_eq!(Condition::decode_arm(0x0000_0000), Condition::EQ);
        assert_eq!(Condition::decode_arm(0x1000_0000), Condition::NE);
        assert_eq!(Condition::decode_arm(0xE000_0000), Condition::AL);
        assert_eq!(Condition::decode_arm(0x3900_0000), Condition::CC);
        for bits in 0..16 {
            assert_eq!(Condition::from_bits(bits).bits(), bits);
        }
    }

    #[test]
    fn test_condition_lut_matches_passes() {
        for cond in 0..16 {
            for flags in 0..16 {
                assert_eq!(condition_passed(cond, Psr::from(flags << 28)), Condition::from_bits(cond).passes(flags));
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

    fn xorshift(state: &mut u32) -> u32 {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        *state
    }

    #[test]
    fn test_every_arm_encoding_round_trips() {
        let mut state = 0x2545_F491;
        for index in 0..lut::LUT_ARM_SIZE {
            for _ in 0..8 {
                let word = lut::with_index_arm(xorshift(&mut state), index);
                let decoded = Instruction::decode_arm(word);
                if matches!(decoded, Instruction::Unknown(_)) {
                    continue;
                }
                let cond = Condition::decode_arm(word);
                let encoded = decoded.encode_arm(cond).unwrap_or_else(|| panic!("{:08X} {:?} has no encoding", word, decoded));
                assert_eq!(Instruction::decode_arm(encoded), decoded, "{:08X} re-encoded as {:08X}", word, encoded);
                assert_eq!(Condition::decode_arm(encoded), cond, "{:08X} re-encoded as {:08X}", word, encoded);
            }
        }
    }

    #[test]
    fn test_every_thumb_encoding_round_trips() {
        for word in 0..=u16::MAX {
            let decoded = Instruction::decode_thumb(word);
            if matches!(decoded, Instruction::Unknown(_)) {
                continue;
            }
            let encoded = decoded.encode_thumb().unwrap_or_else(|| panic!("{:04X} {:?} has no encoding", word, decoded));
            assert_eq!(Instruction::decode_thumb(encoded), decoded, "{:04X} re-encoded as {:04X}", word, encoded);
        }
    }

    #[test]
    fn test_thumb_instructions_encode_as_arm_equivalents() {
        for (thumb, arm) in [(0x2001, 0xE3B0_0001), (0x1888, 0xE091_0002), (0x4770, 0xE12F_FF1E), (0x6801, 0xE590_1000), (0xB510, 0xE92D_4010)] {
            assert_eq!(Instruction::decode_thumb(thumb).encode_arm(Condition::AL), Some(arm), "{:04X}", thumb);
        }
    }
}
