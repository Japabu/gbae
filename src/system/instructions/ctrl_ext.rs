use core::panic;

use crate::{
    bitutil::{get_bit, get_bits},
    system::{cpu::CPU, instructions::get_condition_code},
};

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

pub fn msr_reg_dec(instruction: u32) -> String {
    let r = get_bit(instruction, 22);
    let c = get_bit(instruction, 16);
    let x = get_bit(instruction, 17);
    let s = get_bit(instruction, 18);
    let f = get_bit(instruction, 19);
    let m = get_bits(instruction, 0, 4) as usize;

    format!(
        "MSR{} {}_{}{}{}{}, r{}",
        get_condition_code(instruction),
        if r { "SPSR" } else { "CPSR" },
        if c { "c" } else { "-" },
        if x { "x" } else { "-" },
        if s { "s" } else { "-" },
        if f { "f" } else { "-" },
        m
    )
}

fn msr(cpu: &mut CPU, instruction: u32, operand: u32) {
    debug_assert_eq!(get_bits(instruction, 12, 4), 0b1111);

    let r = get_bit(instruction, 22);
    let c = get_bit(instruction, 16);
    let x = get_bit(instruction, 17);
    let s = get_bit(instruction, 18);
    let f = get_bit(instruction, 19);

    let mut mask = 0u32;
    if c {
        mask |= 0x000000FF;
    }
    if x {
        mask |= 0x0000FF00;
    }
    if s {
        mask |= 0x00FF0000;
    }
    if f {
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
            panic!("Tried to set SPSR in user or system mode");
        }
    }
}
