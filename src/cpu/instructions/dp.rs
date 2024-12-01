use crate::{bitutil::{self, get_bit, get_bits}, cpu::{instructions::set_nz_flags, CPU}};

type Operand2Fn = fn(&mut CPU, u32) -> (u32, bool);
type DpHandlerFn = fn(&mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool);

pub fn handler(
    cpu: &mut CPU,
    instruction: u32,
    operand2_decoder: Operand2Fn,
    handler: DpHandlerFn,
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

    let (so, sco) = operand2_decoder(cpu, instruction);

    handler(cpu, s, n, d, so, sco);
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

pub fn op2_imm_shift(_cpu: &mut CPU,_instructionn: u32) -> (u32, bool) {
    panic!("op2_imm_shift");
}


pub fn op2_reg_shift(_cpu: &mut CPU, _instruction: u32) -> (u32, bool) {
    panic!("op2_reg_shift");
}

pub fn and(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    cpu.r[d] = so & cpu.r[n];

    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.set_carry_flag(sco);
    }
}

pub fn add(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, _sco: bool) {
    let (result, carry, overflow) = bitutil::add(cpu.r[n], so);
    cpu.r[d] = result;

    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.set_carry_flag(carry);
        cpu.set_overflow_flag(overflow);
    }
}

pub fn mov(cpu: &mut CPU, s: bool, n: usize, d: usize, so: u32, sco: bool) {
    debug_assert_eq!(n, 0);

    cpu.r[d] = so;
    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.set_carry_flag(sco);
    }
}
