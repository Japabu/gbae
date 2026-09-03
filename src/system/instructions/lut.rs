use seq_macro::seq;

use crate::{
    bits::Bits,
    system::{cpu::CPU, memory::Memory},
};

use super::{ArmHandler, Instruction, ThumbHandler};

pub const LUT_ARM_SIZE: usize = 1 << 12;
pub const LUT_THUMB_SIZE: usize = 1 << 10;

pub static ARM_LUT: [ArmHandler; LUT_ARM_SIZE] = build_arm_lut();
pub static THUMB_LUT: [ThumbHandler; LUT_THUMB_SIZE] = build_thumb_lut();

#[inline(always)]
pub const fn index_arm(word: u32) -> usize {
    ((word >> 16 & 0xFF0) | (word >> 4 & 0xF)) as usize
}

#[inline(always)]
pub const fn index_thumb(word: u16) -> usize {
    (word >> 6) as usize
}

#[inline(always)]
pub fn with_index_arm(word: u32, index: usize) -> u32 {
    let index = index as u32;
    word.with_bits(20..28, index.bits(4..12)).with_bits(4..8, index.bits(0..4))
}

#[inline(always)]
pub fn with_index_thumb(word: u16, index: usize) -> u16 {
    word.with_bits(6..16, index as u16)
}

const fn build_arm_lut() -> [ArmHandler; LUT_ARM_SIZE] {
    let mut lut = [execute_arm::<0> as ArmHandler; LUT_ARM_SIZE];
    seq!(INDEX in 0..4096 {
        lut[INDEX] = execute_arm::<INDEX>;
    });
    lut
}

const fn build_thumb_lut() -> [ThumbHandler; LUT_THUMB_SIZE] {
    let mut lut = [execute_thumb::<0> as ThumbHandler; LUT_THUMB_SIZE];
    seq!(INDEX in 0..1024 {
        lut[INDEX] = execute_thumb::<INDEX>;
    });
    lut
}

fn execute_arm<const INDEX: usize>(cpu: &mut CPU, mem: &mut Memory, word: u32) {
    Instruction::decode_arm(with_index_arm(word, INDEX)).execute(cpu, mem);
}

fn execute_thumb<const INDEX: usize>(cpu: &mut CPU, mem: &mut Memory, word: u16) {
    Instruction::decode_thumb(with_index_thumb(word, INDEX)).execute(cpu, mem);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_reconstruction_round_trips() {
        for word in [0xE1A0_1000u32, 0xE5C3_3208, 0xE8BD_8000, 0xEA00_0000, 0xE12F_FF11, 0x1234_5678] {
            assert_eq!(with_index_arm(word, index_arm(word)), word);
        }
        for word in [0x4770u16, 0xB500, 0xC9A0, 0x1234] {
            assert_eq!(with_index_thumb(word, index_thumb(word)), word);
        }
    }
}
