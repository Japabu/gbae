use crate::system::{cpu::CPU, memory::Memory};

use super::{branch, ctrl_ext, data_processing, load_store, load_store_multiple, swi, ArmHandler, Instruction, ThumbHandler};

pub const LUT_ARM_SIZE: usize = 1 << 12;
pub const LUT_THUMB_SIZE: usize = 1 << 8;

pub static ARM_LUT: [ArmHandler; LUT_ARM_SIZE] = build_arm_lut();
pub static THUMB_LUT: [ThumbHandler; LUT_THUMB_SIZE] = build_thumb_lut();

static ARM_FORMAT_LUT: [ArmFormat; LUT_ARM_SIZE] = build_format_lut(ARM_FORMATS, ArmFormat::Unknown);
static THUMB_FORMAT_LUT: [ThumbFormat; LUT_THUMB_SIZE] = build_format_lut(THUMB_FORMATS, ThumbFormat::Unknown);

struct Pattern<F> {
    mask: u32,
    value: u32,
    format: F,
}

const fn pattern<F>(text: &str, format: F) -> Pattern<F> {
    Pattern {
        mask: pattern_mask(text),
        value: pattern_value(text),
        format,
    }
}

#[inline(always)]
pub const fn index_arm(word: u32) -> usize {
    ((word >> 16 & 0xFF0) | (word >> 4 & 0xF)) as usize
}

#[inline(always)]
pub const fn index_thumb(word: u16) -> usize {
    (word >> 8) as usize
}

#[inline(always)]
pub fn arm_format(word: u32) -> ArmFormat {
    ARM_FORMAT_LUT[index_arm(word)]
}

#[inline(always)]
pub fn thumb_format(word: u16) -> ThumbFormat {
    THUMB_FORMAT_LUT[index_thumb(word)]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmFormat {
    Mrs,
    Msr,
    BranchExchange,
    Multiply,
    Swap,
    ExtraLoadStore,
    DataProcessing,
    LoadStore,
    LoadStoreMultiple,
    Branch,
    BranchLink,
    SoftwareInterrupt,
    Unknown,
}

const ARM_FORMATS: &[Pattern<ArmFormat>] = &[
    pattern("00010x00 0000", ArmFormat::Mrs),
    pattern("00010x10 0000", ArmFormat::Msr),
    pattern("00010010 0001", ArmFormat::BranchExchange),
    pattern("000000xx 1001", ArmFormat::Multiply),
    pattern("00001xxx 1001", ArmFormat::Multiply),
    pattern("00010x00 1001", ArmFormat::Swap),
    pattern("000xxxxx 1xx1", ArmFormat::ExtraLoadStore),
    pattern("00010xx0 xxxx", ArmFormat::Unknown),
    pattern("000xxxxx xxxx", ArmFormat::DataProcessing),
    pattern("00110x00 xxxx", ArmFormat::Unknown),
    pattern("00110x10 xxxx", ArmFormat::Msr),
    pattern("001xxxxx xxxx", ArmFormat::DataProcessing),
    pattern("010xxxxx xxxx", ArmFormat::LoadStore),
    pattern("011xxxxx xxx0", ArmFormat::LoadStore),
    pattern("100xxxxx xxxx", ArmFormat::LoadStoreMultiple),
    pattern("1010xxxx xxxx", ArmFormat::Branch),
    pattern("1011xxxx xxxx", ArmFormat::BranchLink),
    pattern("1111xxxx xxxx", ArmFormat::SoftwareInterrupt),
];

const ARM_HANDLERS: &[ArmHandler] = &[
    run_arm::<0>,
    run_arm::<1>,
    run_arm::<2>,
    run_arm::<3>,
    run_arm::<4>,
    run_arm::<5>,
    run_arm::<6>,
    run_arm::<7>,
    run_arm::<8>,
    run_arm::<9>,
    run_arm::<10>,
    run_arm::<11>,
    run_arm::<12>,
];

impl ArmFormat {
    const ALL: [ArmFormat; 13] = [
        ArmFormat::Mrs,
        ArmFormat::Msr,
        ArmFormat::BranchExchange,
        ArmFormat::Multiply,
        ArmFormat::Swap,
        ArmFormat::ExtraLoadStore,
        ArmFormat::DataProcessing,
        ArmFormat::LoadStore,
        ArmFormat::LoadStoreMultiple,
        ArmFormat::Branch,
        ArmFormat::BranchLink,
        ArmFormat::SoftwareInterrupt,
        ArmFormat::Unknown,
    ];

    #[inline(always)]
    pub fn decode(self, word: u32) -> Instruction {
        match self {
            ArmFormat::Mrs => ctrl_ext::decode_mrs_arm(word),
            ArmFormat::Msr => ctrl_ext::decode_msr_arm(word),
            ArmFormat::BranchExchange => branch::decode_bx_arm(word),
            ArmFormat::Multiply => super::multiply::decode_arm(word),
            ArmFormat::Swap => load_store::decode_swap_arm(word),
            ArmFormat::ExtraLoadStore => load_store::decode_extra_arm(word),
            ArmFormat::DataProcessing => data_processing::decode_arm(word),
            ArmFormat::LoadStore => load_store::decode_arm(word),
            ArmFormat::LoadStoreMultiple => load_store_multiple::decode_arm(word),
            ArmFormat::Branch => branch::decode_b_arm(word),
            ArmFormat::BranchLink => branch::decode_bl_arm(word),
            ArmFormat::SoftwareInterrupt => swi::decode_arm(word),
            ArmFormat::Unknown => Instruction::Unknown(word),
        }
    }
}

fn run_arm<const FORMAT: usize>(cpu: &mut CPU, mem: &mut Memory, word: u32) {
    ArmFormat::ALL[FORMAT].decode(word).execute(cpu, mem);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbFormat {
    AddSubRegister,
    AddSubImmediate,
    ShiftImmediate,
    MovCmpAddSubImmediate,
    Register,
    BranchExchange,
    Special,
    LoadFromLiteralPool,
    RegisterOffset,
    WordByte,
    Halfword,
    Stack,
    AddSpPc,
    AdjustSp,
    Push,
    Pop,
    LdmStm,
    SoftwareInterrupt,
    ConditionalBranch,
    UnconditionalBranch,
    BlPrefix,
    BlSuffix,
    Unknown,
}

const THUMB_FORMATS: &[Pattern<ThumbFormat>] = &[
    pattern("000 11 0 xx", ThumbFormat::AddSubRegister),
    pattern("000 11 1 xx", ThumbFormat::AddSubImmediate),
    pattern("000 xx x xx", ThumbFormat::ShiftImmediate),
    pattern("001 xxxxx", ThumbFormat::MovCmpAddSubImmediate),
    pattern("010000 xx", ThumbFormat::Register),
    pattern("010001 11", ThumbFormat::BranchExchange),
    pattern("010001 xx", ThumbFormat::Special),
    pattern("01001 xxx", ThumbFormat::LoadFromLiteralPool),
    pattern("0101 xxxx", ThumbFormat::RegisterOffset),
    pattern("011x xxxx", ThumbFormat::WordByte),
    pattern("1000 xxxx", ThumbFormat::Halfword),
    pattern("1001 xxxx", ThumbFormat::Stack),
    pattern("1010 xxxx", ThumbFormat::AddSpPc),
    pattern("1011 0000", ThumbFormat::AdjustSp),
    pattern("1011 010x", ThumbFormat::Push),
    pattern("1011 110x", ThumbFormat::Pop),
    pattern("1100 xxxx", ThumbFormat::LdmStm),
    pattern("1101 1110", ThumbFormat::Unknown),
    pattern("1101 1111", ThumbFormat::SoftwareInterrupt),
    pattern("1101 xxxx", ThumbFormat::ConditionalBranch),
    pattern("11100 xxx", ThumbFormat::UnconditionalBranch),
    pattern("11110 xxx", ThumbFormat::BlPrefix),
    pattern("11111 xxx", ThumbFormat::BlSuffix),
];

const THUMB_HANDLERS: &[ThumbHandler] = &[
    run_thumb::<0>,
    run_thumb::<1>,
    run_thumb::<2>,
    run_thumb::<3>,
    run_thumb::<4>,
    run_thumb::<5>,
    run_thumb::<6>,
    run_thumb::<7>,
    run_thumb::<8>,
    run_thumb::<9>,
    run_thumb::<10>,
    run_thumb::<11>,
    run_thumb::<12>,
    run_thumb::<13>,
    run_thumb::<14>,
    run_thumb::<15>,
    run_thumb::<16>,
    run_thumb::<17>,
    run_thumb::<18>,
    run_thumb::<19>,
    run_thumb::<20>,
    run_thumb::<21>,
    run_thumb::<22>,
];

impl ThumbFormat {
    const ALL: [ThumbFormat; 23] = [
        ThumbFormat::AddSubRegister,
        ThumbFormat::AddSubImmediate,
        ThumbFormat::ShiftImmediate,
        ThumbFormat::MovCmpAddSubImmediate,
        ThumbFormat::Register,
        ThumbFormat::BranchExchange,
        ThumbFormat::Special,
        ThumbFormat::LoadFromLiteralPool,
        ThumbFormat::RegisterOffset,
        ThumbFormat::WordByte,
        ThumbFormat::Halfword,
        ThumbFormat::Stack,
        ThumbFormat::AddSpPc,
        ThumbFormat::AdjustSp,
        ThumbFormat::Push,
        ThumbFormat::Pop,
        ThumbFormat::LdmStm,
        ThumbFormat::SoftwareInterrupt,
        ThumbFormat::ConditionalBranch,
        ThumbFormat::UnconditionalBranch,
        ThumbFormat::BlPrefix,
        ThumbFormat::BlSuffix,
        ThumbFormat::Unknown,
    ];

    #[inline(always)]
    pub fn decode(self, word: u16) -> Instruction {
        match self {
            ThumbFormat::AddSubRegister => data_processing::decode_add_sub_register_thumb(word),
            ThumbFormat::AddSubImmediate => data_processing::decode_add_sub_immediate_thumb(word),
            ThumbFormat::ShiftImmediate => data_processing::decode_shift_imm_thumb(word),
            ThumbFormat::MovCmpAddSubImmediate => data_processing::decode_mov_cmp_add_sub_immediate_thumb(word),
            ThumbFormat::Register => data_processing::decode_register_thumb(word),
            ThumbFormat::BranchExchange => branch::decode_branch_exchange_thumb(word),
            ThumbFormat::Special => data_processing::decode_special_thumb(word),
            ThumbFormat::LoadFromLiteralPool => load_store::decode_load_from_literal_pool_thumb(word),
            ThumbFormat::RegisterOffset => load_store::decode_register_offset_thumb(word),
            ThumbFormat::WordByte => load_store::decode_word_byte_thumb(word),
            ThumbFormat::Halfword => load_store::decode_halfword_thumb(word),
            ThumbFormat::Stack => load_store::decode_stack_thumb(word),
            ThumbFormat::AddSpPc => data_processing::decode_add_sp_pc_thumb(word),
            ThumbFormat::AdjustSp => data_processing::decode_adjust_sp_thumb(word),
            ThumbFormat::Push => load_store_multiple::decode_push_thumb(word),
            ThumbFormat::Pop => load_store_multiple::decode_pop_thumb(word),
            ThumbFormat::LdmStm => load_store_multiple::decode_ldm_stm_thumb(word),
            ThumbFormat::SoftwareInterrupt => swi::decode_thumb(word),
            ThumbFormat::ConditionalBranch => branch::decode_conditional_branch_thumb(word),
            ThumbFormat::UnconditionalBranch => branch::decode_unconditional_branch_thumb(word),
            ThumbFormat::BlPrefix => branch::decode_bl_prefix_thumb(word),
            ThumbFormat::BlSuffix => branch::decode_bl_suffix_thumb(word),
            ThumbFormat::Unknown => Instruction::Unknown(u32::from(word)),
        }
    }
}

fn run_thumb<const FORMAT: usize>(cpu: &mut CPU, mem: &mut Memory, word: u16) {
    ThumbFormat::ALL[FORMAT].decode(word).execute(cpu, mem);
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

const fn format_of<F: Copy>(formats: &[Pattern<F>], index: usize, unknown: F) -> F {
    let mut i = 0;
    while i < formats.len() {
        if index as u32 & formats[i].mask == formats[i].value {
            return formats[i].format;
        }
        i += 1;
    }
    unknown
}

const fn build_format_lut<F: Copy, const SIZE: usize>(formats: &[Pattern<F>], unknown: F) -> [F; SIZE] {
    let mut lut = [unknown; SIZE];
    let mut index = 0;
    while index < SIZE {
        lut[index] = format_of(formats, index, unknown);
        index += 1;
    }
    lut
}

const fn build_arm_lut() -> [ArmHandler; LUT_ARM_SIZE] {
    let mut lut = [ARM_HANDLERS[0]; LUT_ARM_SIZE];
    let mut index = 0;
    while index < LUT_ARM_SIZE {
        lut[index] = ARM_HANDLERS[ARM_FORMAT_LUT[index] as usize];
        index += 1;
    }
    lut
}

const fn build_thumb_lut() -> [ThumbHandler; LUT_THUMB_SIZE] {
    let mut lut = [THUMB_HANDLERS[0]; LUT_THUMB_SIZE];
    let mut index = 0;
    while index < LUT_THUMB_SIZE {
        lut[index] = THUMB_HANDLERS[THUMB_FORMAT_LUT[index] as usize];
        index += 1;
    }
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_first_matching_pattern_wins() {
        assert_eq!(arm_format(0xE000_0091), ArmFormat::Multiply);
        assert_eq!(arm_format(0xE000_0000), ArmFormat::DataProcessing);
        assert_eq!(arm_format(0xE1B0_0090), ArmFormat::ExtraLoadStore);
        assert_eq!(thumb_format(0xDE00), ThumbFormat::Unknown);
        assert_eq!(thumb_format(0xD000), ThumbFormat::ConditionalBranch);
    }

    #[test]
    fn test_handler_tables_follow_the_format_tables() {
        for format in ArmFormat::ALL {
            assert_eq!(ARM_HANDLERS[format as usize] as usize, ARM_LUT[ARM_FORMAT_LUT.iter().position(|f| *f == format).unwrap()] as usize);
        }
        for format in ThumbFormat::ALL {
            assert_eq!(
                THUMB_HANDLERS[format as usize] as usize,
                THUMB_LUT[THUMB_FORMAT_LUT.iter().position(|f| *f == format).unwrap()] as usize
            );
        }
    }
}
