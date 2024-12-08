use std::fmt::Display;

use crate::{
    bitutil::{self, arithmetic_shift_right, get_bit, get_bits, sub_with_flags},
    system::{cpu::CPU, instructions::set_nz_flags},
};

use super::{Condition, DecodedInstruction};

struct Instruction {
    opcode: Opcode,
    cond: Condition,
    set_flags: bool,
    d: u8,
    n: u8,
    shifter_operand: ShifterOperand,
}

impl DecodedInstruction for Instruction {
    fn execute(&self, cpu: &mut CPU) {
        todo!();
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Opcode::*;
        let has_d = !matches!(self.opcode, CMP | CMN | TST | TEQ);
        let has_n = !matches!(self.opcode, MOV | MVN);

        // <opcode>{<cond>}{S} <Rd>, <Rn>, <shifter_operand>
        write!(
            f,
            "{:?}{}{} {}{}{}",
            self.opcode,
            self.cond,
            if has_d && self.set_flags { "S" } else { "" },
            if has_d { format!("R{}, ", self.d) } else { "".into() },
            if has_n { format!("R{}, ", self.n) } else { "".into() },
            self.shifter_operand
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mov() {
        let instruction = 0xe1a01000;
        let inst = decode_arm(instruction);
        assert_eq!("MOV R1, R0", format!("{}", inst));
    }

    #[test]
    fn test_cmp() {
        let instruction = 0xe1500000;
        let inst = decode_arm(instruction);
        assert_eq!("CMP R0, R0", format!("{}", inst));
    }

    #[test]
    fn test_add() {
        let instruction = 0xe0859185;
        let inst = decode_arm(instruction);
        assert_eq!("ADD R9, R5, R5, LSL #3", format!("{}", inst));
    }
}

pub fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
    Box::new(Instruction {
        opcode: Opcode::decode_arm(instruction),
        cond: Condition::decode_arm(instruction),
        set_flags: get_bit(instruction, 20),
        d: get_bits(instruction, 12, 4) as u8,
        n: get_bits(instruction, 16, 4) as u8,
        shifter_operand: ShifterOperand::decode_arm(instruction),
    })
}

impl Opcode {
    const fn decode_arm(instruction: u32) -> Opcode {
        match get_bits(instruction, 21, 4) {
            0b0000 => Opcode::AND,
            0b0001 => Opcode::EOR,
            0b0010 => Opcode::SUB,
            0b0011 => Opcode::RSB,
            0b0100 => Opcode::ADD,
            0b0101 => Opcode::ADC,
            0b0110 => Opcode::SBC,
            0b0111 => Opcode::RSC,
            0b1000 => Opcode::TST,
            0b1001 => Opcode::TEQ,
            0b1010 => Opcode::CMP,
            0b1011 => Opcode::CMN,
            0b1100 => Opcode::ORR,
            0b1101 => Opcode::MOV,
            0b1110 => Opcode::BIC,
            0b1111 => Opcode::MVN,
            _ => unreachable!(),
        }
    }
}

enum ShifterOperand {
    Immediate { immed_8: u8, rotate_imm: u8 },
    Register { m: u8 },
    LogicalShiftLeftImmediate { m: u8, shift_imm: u8 },
    LogicalShiftLeftRegister { m: u8, s: u8 },
    LogicalShiftRightImmediate { m: u8, shift_imm: u8 },
    LogicalShiftRightRegister { m: u8, shift_reg: u8 },
    ArithmeticShiftRightImmediate { m: u8, shift_imm: u8 },
    ArithmeticShiftRightRegister { m: u8, shift_reg: u8 },
    RotateRightImmediate { m: u8, shift_imm: u8 },
    RotateRightRegister { m: u8, shift_reg: u8 },
    RotateRightWithExtend { m: u8 },
}

const fn calc_immediate(immed_8: u8, rotate_imm: u8) -> u32 {
    (immed_8 as u32).rotate_right(2 * rotate_imm as u32)
}

impl Display for ShifterOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ShifterOperand::Immediate { immed_8, rotate_imm } => write!(f, "#{}", calc_immediate(immed_8, rotate_imm)),
            ShifterOperand::Register { m } => write!(f, "R{}", m),
            ShifterOperand::LogicalShiftLeftImmediate { m, shift_imm } => write!(f, "R{}, LSL #{}", m, shift_imm),
            ShifterOperand::LogicalShiftLeftRegister { m, s } => write!(f, "R{}, LSL R{}", m, s),
            ShifterOperand::LogicalShiftRightImmediate { m, shift_imm } => write!(f, "R{}, LSR #{}", m, shift_imm),
            ShifterOperand::LogicalShiftRightRegister { m, shift_reg } => write!(f, "R{}, LSR R{}", m, shift_reg),
            ShifterOperand::ArithmeticShiftRightImmediate { m, shift_imm } => write!(f, "R{}, ASR #{}", m, shift_imm),
            ShifterOperand::ArithmeticShiftRightRegister { m, shift_reg } => write!(f, "R{}, ASR R{}", m, shift_reg),
            ShifterOperand::RotateRightImmediate { m, shift_imm } => write!(f, "R{}, ROR #{}", m, shift_imm),
            ShifterOperand::RotateRightRegister { m, shift_reg } => write!(f, "R{}, ROR R{}", m, shift_reg),
            ShifterOperand::RotateRightWithExtend { m } => write!(f, "R{}, RRX", m),
        }
    }
}

#[derive(Debug)]
enum Opcode {
    AND,
    EOR,
    SUB,
    RSB,
    ADD,
    ADC,
    SBC,
    RSC,
    TST,
    TEQ,
    CMP,
    CMN,
    ORR,
    MOV,
    BIC,
    MVN,
}

impl ShifterOperand {
    const fn decode_arm(instruction: u32) -> ShifterOperand {
        let is_immediate = get_bit(instruction, 25);

        if is_immediate {
            ShifterOperand::Immediate {
                immed_8: get_bits(instruction, 0, 8) as u8,
                rotate_imm: get_bits(instruction, 8, 4) as u8,
            }
        } else {
            let m = get_bits(instruction, 0, 4) as u8;
            let is_reg_shift = get_bit(instruction, 4);

            if is_reg_shift {
                let s = get_bits(instruction, 8, 4) as u8;
                let shift_type = get_bits(instruction, 5, 2);
                match shift_type {
                    0b00 => ShifterOperand::LogicalShiftLeftRegister { m, s },
                    0b01 => ShifterOperand::LogicalShiftRightRegister { m, shift_reg: s },
                    0b10 => ShifterOperand::ArithmeticShiftRightRegister { m, shift_reg: s },
                    0b11 => ShifterOperand::RotateRightRegister { m, shift_reg: s },
                    _ => unreachable!(),
                }
            } else {
                let shift_imm = get_bits(instruction, 7, 5) as u8;
                let shift_type = get_bits(instruction, 5, 2);
                match shift_type {
                    0b00 if shift_imm == 0 => ShifterOperand::Register { m },
                    0b00 => ShifterOperand::LogicalShiftLeftImmediate { m, shift_imm },
                    0b01 => ShifterOperand::LogicalShiftRightImmediate { m, shift_imm },
                    0b10 => ShifterOperand::ArithmeticShiftRightImmediate { m, shift_imm },
                    0b11 if shift_imm == 0 => ShifterOperand::RotateRightWithExtend { m },
                    0b11 => ShifterOperand::RotateRightImmediate { m, shift_imm },
                    _ => unreachable!(),
                }
            }
        }
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

fn format_instruction(opcode: &str, condition: &str, s: bool, n: Option<u32>, d: Option<u32>, so: &str) -> String {
    let s_flag = if s { "S" } else { "" };
    let d_reg = if let Some(d) = d { format!(", R{}", d) } else { "".to_string() };
    let n_reg = if let Some(n) = n { format!(", R{}", n) } else { "".to_string() };

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
