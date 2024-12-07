use crate::{
    bitutil::{self, arithmetic_shift_right, get_bit, get_bits, sub_with_flags},
    system::{cpu::CPU, instructions::set_nz_flags},
};

use super::get_condition_code;

type DpHandlerFn = fn(&mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool);

pub fn handler(cpu: &mut CPU, instruction: u32, dp_handler: DpHandlerFn) {
    // Decode instruction components
    let s = get_bit(instruction, 20); // Set flags bit
    let n = get_bits(instruction, 16, 4) as usize; // Operand 1 register
    let d = get_bits(instruction, 12, 4) as usize; // Destination register

    if d == 15 {
        panic!("DP instructions with destination register 15 are not implemented");
    }

    // Decode shifter operand
    let (so, sco) = eval_so(cpu, instruction);

    // Call the data-processing handler
    dp_handler(cpu, s, n, d, so, sco);
}

fn eval_so(cpu: &CPU, instruction: u32) -> (u32, bool) {
    if get_bit(instruction, 25) {
        // Immediate operand
        eval_immediate_so(cpu, instruction)
    } else {
        // Register operand with potential shift
        eval_register_so(cpu, instruction)
    }
}

fn eval_immediate_so(cpu: &CPU, instruction: u32) -> (u32, bool) {
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

fn eval_register_so(cpu: &CPU, instruction: u32) -> (u32, bool) {
    let m = get_bits(instruction, 0, 4) as usize;
    let r_m = cpu.get_r(m);
    let is_reg_shift = get_bit(instruction, 4);

    let shift_amount = if is_reg_shift {
        cpu.get_r(get_bits(instruction, 8, 4) as usize) & 0xFF // Shift by least significant byte of Rs
    } else {
        get_bits(instruction, 7, 5) // Shift by immediate (0-31)
    };

    let shift_type = get_bits(instruction, 5, 2);
    eval_shift(cpu.get_carry_flag(), r_m, shift_amount, shift_type, is_reg_shift)
}

fn eval_shift(carry_flag: bool, r_m: u32, shift_amount: u32, shift_type: u32, is_reg_shift: bool) -> (u32, bool) {
    let r_m_31 = get_bit(r_m, 31);

    match shift_type {
        0b00 => eval_shift_left(r_m, shift_amount, r_m, carry_flag),
        0b01 => eval_shift_right(r_m, shift_amount, if is_reg_shift { r_m } else { 0 }, if is_reg_shift { carry_flag } else { r_m_31 }),
        0b10 => eval_arith_shift_right(
            r_m,
            r_m_31,
            shift_amount,
            if is_reg_shift {
                r_m
            } else if r_m_31 {
                0xFFFFFFFF
            } else {
                0
            },
            if is_reg_shift { carry_flag } else { r_m_31 },
        ),
        0b11 if !is_reg_shift && shift_amount == 0 => eval_rotate_right_extend(r_m, carry_flag),
        0b11 => eval_rotate_right(r_m, get_bits(shift_amount, 0, 5), r_m, if shift_amount == 0 { carry_flag } else { r_m_31 }),
        _ => unreachable!(),
    }
}

fn eval_shift_left(r_m: u32, shift_amount: u32, zero_so: u32, zero_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (zero_so, zero_sco),
        ..32 => (r_m << shift_amount, get_bit(r_m, 32 - shift_amount)),
        32 => (0, get_bit(r_m, 0)),
        _ => (0, false),
    }
}

fn eval_shift_right(r_m: u32, shift_amount: u32, zero_so: u32, zero_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (zero_so, zero_sco),
        ..32 => (r_m >> shift_amount, get_bit(r_m, shift_amount - 1)),
        32 => (0, get_bit(r_m, 31)),
        _ => (0, false),
    }
}

fn eval_arith_shift_right(r_m: u32, r_m_31: bool, shift_amount: u32, zero_so: u32, zero_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (zero_so, zero_sco),
        ..32 => (arithmetic_shift_right(r_m, shift_amount), get_bit(r_m, shift_amount - 1)),
        _ => (if r_m_31 { 0xFFFFFFFF } else { 0 }, r_m_31),
    }
}

fn eval_rotate_right(r_m: u32, shift_amount: u32, zero_so: u32, zero_sco: bool) -> (u32, bool) {
    match shift_amount {
        0 => (zero_so, zero_sco),
        _ => (r_m.rotate_right(shift_amount), get_bit(r_m, shift_amount - 1)),
    }
}

fn eval_rotate_right_extend(r_m: u32, carry_flag: bool) -> (u32, bool) {
    (((carry_flag as u32) << 31) | (r_m >> 1), get_bit(r_m, 0))
}

pub fn dec(instruction: u32) -> String {
    // set flags bit
    let s = get_bit(instruction, 20);

    // operand 1 register
    let n = get_bits(instruction, 16, 4);

    // destination register
    let d = get_bits(instruction, 12, 4);

    // decode shifter operand
    let so = dec_so(instruction);

    let opcode = get_bits(instruction, 21, 4);
    let opcode_str = get_opcode_mnemonic(opcode);
    let condition_str = get_condition_code(instruction);
    match opcode {
        0b1000..=0b1011 => format_instruction(opcode_str, condition_str, false, Some(n), None, &so),
        0b1101 | 0b1111 => format_instruction(opcode_str, condition_str, s, None, Some(d), &so),
        _ => format_instruction(opcode_str, condition_str, s, Some(n), Some(d), &so),
    }
}

fn dec_so(instruction: u32) -> String {
    let m = get_bits(instruction, 0, 4);
    let is_imm = get_bit(instruction, 25);
    if is_imm {
        let immed_8 = get_bits(instruction, 0, 8);
        let rotate_imm = get_bits(instruction, 8, 4);
        let shifter_operand = immed_8.rotate_right(2 * rotate_imm);
        return format!("#{:#08x}", shifter_operand);
    }

    let is_reg_shift = get_bit(instruction, 4);
    let shift_type = match get_bits(instruction, 5, 2) {
        0b00 => "LSL",
        0b01 => "LSR",
        0b10 => "ASR",
        0b11 if !is_reg_shift && get_bits(instruction, 7, 5) == 0 => "RRX",
        0b11 => "ROR",
        _ => unreachable!(),
    };

    if is_reg_shift {
        format!("r{}, {} r{}", m, shift_type, get_bits(instruction, 8, 4))
    } else {
        let imm_5 = get_bits(instruction, 7, 5);
        if imm_5 == 0 {
            format!("r{}", m)
        } else {
            format!("r{}, {} #{}", m, shift_type, imm_5)
        }
    }
}

fn format_instruction(opcode: &str, condition: &str, s: bool, n: Option<u32>, d: Option<u32>, so: &str) -> String {
    let s_flag = if s { "S" } else { "" };
    let d_reg = if let Some(d) = d { format!(", r{}", d) } else { "".to_string() };
    let n_reg = if let Some(n) = n { format!(", r{}", n) } else { "".to_string() };

    format!("{}{}{}{}{}, {}", opcode, condition, s_flag, d_reg, n_reg, so)
}

fn get_opcode_mnemonic(opcode: u32) -> &'static str {
    ["AND", "EOR", "SUB", "RSB", "ADD", "ADC", "SBC", "RSC", "TST", "TEQ", "CMP", "CMN", "ORR", "MOV", "BIC", "MVN"][opcode as usize]
}

pub fn and(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    cpu.set_r(d, so & cpu.get_r(n));

    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(sco);
    }
}

pub fn eor(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    todo!();
}

pub fn sub(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    let (result, borrow, overflow) = bitutil::sub_with_flags(cpu.get_r(n), so);
    cpu.set_r(d, result);

    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(!borrow);
        cpu.set_overflow_flag(overflow);
    }
}

pub fn rsb(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    todo!();
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

pub fn adc(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    todo!();
}

pub fn sbc(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    todo!();
}

pub fn rsc(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    todo!();
}

pub fn tst(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    todo!();
}

pub fn teq(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    todo!();
}

pub fn cmp(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    let (alu_out, borrow, overflow) = sub_with_flags(cpu.get_r(n), so);
    set_nz_flags(cpu, alu_out);
    cpu.set_carry_flag(!borrow);
    cpu.set_overflow_flag(overflow);
}

pub fn cmn(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    todo!();
}

pub fn orr(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    todo!();
}

pub fn mov(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    debug_assert_eq!(n, 0);

    cpu.set_r(d, so);
    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(sco);
    }
}

pub fn bic(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    todo!();
}

pub fn mvn(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    debug_assert_eq!(n, 0);

    cpu.set_r(d, !so);
    if s {
        set_nz_flags(cpu, cpu.get_r(d));
        cpu.set_carry_flag(sco);
    }
}
