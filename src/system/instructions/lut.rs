use seq_macro::seq;

use crate::system::{cpu::CPU, memory::Memory};

use super::{ArmHandler, Instruction, ThumbHandler};

pub const LUT_ARM_SIZE: usize = 1 << 12;
pub const LUT_THUMB_SIZE: usize = 1 << 10;
const ARM_INDEX_MASK: u32 = 0x0FF0_00F0;

pub static ARM_LUT: [ArmHandler; LUT_ARM_SIZE] = build_arm_lut();
pub static THUMB_LUT: [ThumbHandler; LUT_THUMB_SIZE] = build_thumb_lut();

#[inline(always)]
pub const fn index_arm(instruction: u32) -> usize {
    (((instruction >> 16) & 0xFF0) | ((instruction >> 4) & 0xF)) as usize
}

#[inline(always)]
pub const fn index_thumb(instruction: u16) -> usize {
    (instruction >> 6) as usize
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

fn execute_arm<const INDEX: u32>(cpu: &mut CPU, mem: &mut Memory, instruction: u32) {
    let instruction = (instruction & !ARM_INDEX_MASK) | ((INDEX & 0xFF0) << 16) | ((INDEX & 0xF) << 4);
    Instruction::decode_arm(instruction).execute(cpu, mem);
}

fn execute_thumb<const INDEX: u16>(cpu: &mut CPU, mem: &mut Memory, instruction: u16) {
    let instruction = (instruction & 0x3F) | (INDEX << 6);
    Instruction::decode_thumb(instruction).execute(cpu, mem);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_reconstruction_round_trips() {
        for instruction in [0xE1A0_1000u32, 0xE5C3_3208, 0xE8BD_8000, 0xEA00_0000, 0xE12F_FF11, 0x1234_5678] {
            let index = index_arm(instruction) as u32;
            let rebuilt = (instruction & !ARM_INDEX_MASK) | ((index & 0xFF0) << 16) | ((index & 0xF) << 4);
            assert_eq!(rebuilt, instruction);
        }
        for instruction in [0x4770u16, 0xB500, 0xC9A0, 0x1234] {
            let index = index_thumb(instruction) as u16;
            assert_eq!((instruction & 0x3F) | (index << 6), instruction);
        }
    }
}
