use crate::{
    bitutil::{self, arithmetic_shift_right, get_bit, get_bits},
    system::{cpu::CPU, instructions::set_nz_flags},
};

type DpHandlerFn = fn(&mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool);
type DpDecFn = fn(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String;

pub fn handler(cpu: &mut CPU, instruction: u32, dp_handler: DpHandlerFn) {
    // set flags bit
    let s = get_bit(instruction, 20);

    // operand 1 register
    let n = get_bits(instruction, 16, 4) as usize;

    // destination register
    let d = get_bits(instruction, 12, 4) as usize;

    if d == 15 {
        panic!("dp instructions with destination register 15 not implemented");
    }

    let is_imm = get_bit(instruction, 25);
    let (so, sco) = if is_imm {
        op2_imm(cpu, instruction)
    } else {
        let m = get_bits(instruction, 0, 4) as usize;
        let r_m = cpu.get_r(m);
        let r_m_31 = get_bit(r_m, 31);
        let is_reg = get_bit(instruction, 4);
        let shift_amount = if is_reg {
            cpu.get_r(get_bits(instruction, 8, 4) as usize) & 0xFF // r_s
        } else {
            get_bits(instruction, 7, 5)
        };

        let shift_type = get_bits(instruction, 5, 2);
        match shift_type {
            0b00 => op2_shift_left(r_m, shift_amount, r_m, cpu.get_carry_flag()),
            0b01 => op2_shift_right(
                r_m,
                shift_amount,
                if is_reg { r_m } else { 0 },
                if is_reg { cpu.get_carry_flag() } else { r_m_31 },
            ),
            0b10 => op2_arith_shift_right(
                r_m,
                shift_amount,
                if is_reg {
                    r_m
                } else {
                    if r_m_31 {
                        0xFFFFFFFF
                    } else {
                        0
                    }
                },
                if is_reg { cpu.get_carry_flag() } else { r_m_31 },
            ),
            0b11 if !is_reg && shift_amount == 0 => {
                op2_rotate_right_extend(r_m, cpu.get_carry_flag())
            }
            0b11 => op2_rotate_right(
                r_m,
                get_bits(shift_amount, 0, 5),
                r_m,
                if shift_amount == 0 {
                    cpu.get_carry_flag()
                } else {
                    r_m_31
                },
            ),
            _ => unreachable!(),
        }
    };

    dp_handler(cpu, s, n, d, so, sco);
}

pub fn dec(instruction: u32, dp_decoder: DpDecFn) -> String {
    // set flags bit
    let s = get_bit(instruction, 20);

    // operand 1 register
    let n = get_bits(instruction, 16, 4) as usize;

    // destination register
    let d = get_bits(instruction, 12, 4) as usize;

    let is_imm = get_bit(instruction, 25);
    let so = if is_imm {
        op2_imm_dec(instruction)
    } else {
        op2_shift_dec(instruction)
    };

    dp_decoder(instruction, s, n, d, so)
}

fn op2_imm(cpu: &mut CPU, instruction: u32) -> (u32, bool) {
    let immed_8 = get_bits(instruction, 0, 8);
    let rotate_imm = get_bits(instruction, 8, 4);
    let shifter_operand = immed_8.rotate_right(2 * rotate_imm);
    let carry: bool;
    if rotate_imm == 0 {
        carry = cpu.get_carry_flag()
    } else {
        carry = get_bit(shifter_operand, 31)
    };
    (shifter_operand, carry)
}

fn op2_imm_dec(instruction: u32) -> String {
    let immed_8 = get_bits(instruction, 0, 8);
    let rotate_imm = get_bits(instruction, 8, 4);
    let shifter_operand = immed_8.rotate_right(2 * rotate_imm);
    format!("#{:08x}", shifter_operand)
}

fn op2_shift_left(r_m: u32, shift_amount: u32, z_so: u32, z_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (z_so, z_sco),
        ..32 => (r_m << shift_amount, get_bit(r_m, 32 - shift_amount)),
        32 => (0, get_bit(r_m, 0)),
        _ => (0, false),
    }
}

fn op2_shift_right(r_m: u32, shift_amount: u32, z_so: u32, z_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (z_so, z_sco),
        ..32 => (r_m >> shift_amount, get_bit(r_m, shift_amount - 1)),
        32 => (0, get_bit(r_m, 31)),
        _ => (0, false),
    }
}

fn op2_arith_shift_right(r_m: u32, shift_amount: u32, z_so: u32, z_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (z_so, z_sco),
        ..32 => (
            arithmetic_shift_right(r_m, shift_amount),
            get_bit(r_m, shift_amount - 1),
        ),
        _ => (
            if get_bit(r_m, 31) { 0xFFFFFFFF } else { 0 },
            get_bit(r_m, 31),
        ),
    }
}

fn op2_rotate_right(r_m: u32, shift_amount: u32, z_so: u32, z_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (z_so, z_sco),
        _ => (
            r_m.rotate_right(shift_amount),
            get_bit(r_m, shift_amount - 1),
        ),
    }
}

fn op2_rotate_right_extend(r_m: u32, carry_flag: bool) -> (u32, bool) {
    (((carry_flag as u32) << 31) | (r_m >> 1), get_bit(r_m, 0))
}

fn op2_shift_dec(instruction: u32) -> String {
    let m = get_bits(instruction, 0, 4);
    let is_reg = get_bit(instruction, 25);

    let shift_type = match get_bits(instruction, 5, 2) {
        0b00 => "LSL",
        0b01 => "LSR",
        0b10 => "ASR",
        0b11 if !is_reg && get_bits(instruction, 7, 5) == 0 => "RRX",
        0b11 => "ROR",
        _ => unreachable!(),
    };

    let shift_amount = if is_reg {
        format!("r{}", get_bits(instruction, 8, 4))
    } else {
        format!("#{}", get_bits(instruction, 7, 5))
    };

    format!("r{}, {} {}", m, shift_type, shift_amount)
}

pub fn and(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    cpu.set_r(d, so & cpu.get_r(n));

    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(sco);
    }
}

pub fn and_dec(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String {
    format!(
        "AND{}{} r{}, {}",
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}

pub fn sub(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    let (result, borrow, overflow) = bitutil::sub_with_flags(cpu.get_r(n), so);
    cpu.set_r(d, result);

    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(!borrow);
        cpu.set_overflow_flag(overflow);
    }
}

pub fn sub_dec(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String {
    format!(
        "SUB{}{} r{}, {}",
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}

pub fn add(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    let (result, carry, overflow) = bitutil::add_with_flags(cpu.get_r(n), so);
    cpu.set_r(d, result);

    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(carry);
        cpu.set_overflow_flag(overflow);
    }
}

pub fn add_dec(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String {
    format!(
        "ADD{}{} r{}, {}",
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}

pub fn mov(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    debug_assert_eq!(n, 0);

    cpu.set_r(d, so);
    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(sco);
    }
}

pub fn mov_dec(instruction: u32, s: bool, _n: usize, d: usize, so: String) -> String {
    format!(
        "MOV{}{} r{}, {}",
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}
