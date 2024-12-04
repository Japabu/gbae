use crate::{bitutil::{self, get_bit, get_bits}, system::{instructions::set_nz_flags, cpu::CPU}};

type Operand2Fn = fn(&mut CPU, u32) -> (u32, bool);
type DpHandlerFn = fn(&mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool);
type Operand2DecFn = fn(u32) -> String;
type DpDecFn = fn(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String;

pub fn handler(
    cpu: &mut CPU,
    instruction: u32,
    operand2_evaluator: Operand2Fn,
    dp_handler: DpHandlerFn,
) {
    // set flags bit
    let s = get_bit(instruction, 20);

    // operand 1 register
    let n = get_bits(instruction, 16, 4) as usize;

    // destination register
    let d = get_bits(instruction, 12, 4) as usize;

    if d == 15 {
        panic!("dp instructions with destination register 15 not implemented");
    }

    let (so, sco) = operand2_evaluator(cpu, instruction);

    dp_handler(cpu, s, n, d, so, sco);
}

pub fn dec(instruction: u32, operand2_decoder: Operand2DecFn, dp_decoder: DpDecFn) -> String {
        // set flags bit
        let s = get_bit(instruction, 20);

        // operand 1 register
        let n = get_bits(instruction, 16, 4) as usize;
    
        // destination register
        let d = get_bits(instruction, 12, 4) as usize;
    
        if d == 15 {
            panic!("dp instructions with destination register 15 not implemented");
        }
    
        let so = operand2_decoder(instruction);
    
        dp_decoder(instruction, s, n, d, so)
}

pub fn op2_imm(cpu: &mut CPU, instruction: u32) -> (u32, bool) {
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

pub fn op2_imm_dec(instruction: u32) -> String {
    let immed_8 = get_bits(instruction, 0, 8);
    let rotate_imm = get_bits(instruction, 8, 4);
    let shifter_operand = immed_8.rotate_right(2 * rotate_imm);
    format!("#{:08x}", shifter_operand)
}

pub fn op2_imm_shift(_cpu: &mut CPU,_instructionn: u32) -> (u32, bool) {
    todo!("op2_imm_shift");
}

pub fn op2_imm_shift_dec(_instruction: u32) -> String {
    todo!("op2_imm_shift_dec");
}


pub fn op2_reg_shift(_cpu: &mut CPU, _instruction: u32) -> (u32, bool) {
    todo!("op2_reg_shift");
}

pub fn op2_reg_shift_dec(_instruction: u32) -> String {
    todo!("op2_reg_shift_dec");
}

pub fn and(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    cpu.r[d] = so & cpu.r[n];

    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.set_carry_flag(sco);
    }
}

pub fn and_dec(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String {
    format!("AND{}{} r{}, {}", 
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}

pub fn sub(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    let (result, borrow, overflow) = bitutil::sub_with_flags(cpu.r[n], so);
    cpu.r[d] = result;

    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.set_carry_flag(!borrow);
        cpu.set_overflow_flag(overflow);
    }
}

pub fn sub_dec(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String {
    format!("SUB{}{} r{}, {}", 
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}

pub fn add(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    let (result, carry, overflow) = bitutil::add_with_flags(cpu.r[n], so);
    cpu.r[d] = result;

    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.set_carry_flag(carry);
        cpu.set_overflow_flag(overflow);
    }
}

pub fn add_dec(instruction: u32, s: bool, n: usize, d: usize, so: String) -> String {
    format!("ADD{}{} r{}, {}", 
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}

pub fn mov(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    debug_assert_eq!(n, 0);

    cpu.r[d] = so;
    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.set_carry_flag(sco);
    }
}

pub fn mov_dec(instruction: u32, s: bool, _n: usize, d: usize, so: String) -> String {
    format!("MOV{}{} r{}, {}", 
        super::get_condition_code(instruction),
        if s { "S" } else { "" },
        d,
        so
    )
}
