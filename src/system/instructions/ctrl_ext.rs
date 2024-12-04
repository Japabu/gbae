use core::panic;

use crate::{bitutil::{get_bit, get_bits}, system::cpu::CPU};

// Masks for processor ARM7TDMI
const UNALLOC_MASK: u32 = 0x0FFFFF00;
const USER_MASK: u32 = 0xF0000000;
const PRIV_MASK: u32 = 0x0000000F;
const STATE_MASK: u32 = 0x00000020;

pub fn msr_reg(cpu: &mut CPU, instruction: u32) {
    debug_assert_eq!(get_bits(instruction, 8, 4), 0b0000);

    let m = get_bits(instruction, 0, 4) as usize;
    let r_m = cpu.r[m];

    msr(cpu, instruction, r_m);
}


fn msr(cpu: &mut CPU, instruction: u32, operand: u32) {
    debug_assert_eq!(get_bits(instruction, 12, 4), 0b1111);

    let field_mask = get_bits(instruction, 16, 4);
    let r = get_bit(instruction, 22);

    let mut mask = 0u32;
    if get_bit(field_mask, 0) {
        mask |= 0x000000FF;
    }
    if get_bit(field_mask, 1) {
        mask |= 0x0000FF00;
    }
    if get_bit(field_mask, 2) {
        mask |= 0x00FF0000;
    }
    if get_bit(field_mask, 3) {
        mask |= 0xFF000000;
    }

    if !r {
        if cpu.in_a_privileged_mode() {
            if operand & STATE_MASK != 0 {
                panic!("Attempt to set non-ARM execution state");
            } else {
                mask &= USER_MASK | PRIV_MASK;
            }
        } else {
            mask &= USER_MASK;
        }
        cpu.cpsr = (cpu.cpsr & !mask) | (operand & mask);
    } else {
        if cpu.current_mode_has_spsr() {
            mask &= USER_MASK | PRIV_MASK | STATE_MASK;
            cpu.spsr = (cpu.spsr & !mask) | (operand & mask);
        } else {
            panic!("UNPREDICTABLE");
        }
    }
}
