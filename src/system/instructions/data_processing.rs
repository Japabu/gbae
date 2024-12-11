use std::fmt::Display;

use crate::{
    bitutil::{self, arithmetic_shift_right, get_bit, get_bit16, get_bits16, get_bits32, rotate_right_with_extend},
    system::cpu::CPU,
};

use super::{Condition, DecodedInstruction};

pub fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
    Box::new(DataProcessing {
        opcode: Opcode::decode_arm(instruction),
        set_flags: get_bit(instruction, 20),
        d: get_bits32(instruction, 12, 4) as u8,
        n: get_bits32(instruction, 16, 4) as u8,
        shifter_operand: ShifterOperand::decode_arm(instruction),
    })
}

pub fn decode_add_sub_register_thumb(instruction: u16) -> Box<dyn DecodedInstruction> {
    Box::new(DataProcessing {
        opcode: if get_bit16(instruction, 9) { Opcode::SUB } else { Opcode::ADD },
        set_flags: true,
        d: get_bits16(instruction, 0, 3) as u8,
        n: get_bits16(instruction, 3, 3) as u8,
        shifter_operand: ShifterOperand::Register {
            m: get_bits16(instruction, 6, 3) as u8,
        },
    })
}

pub fn decode_add_sub_immediate_thumb(instruction: u16) -> Box<dyn DecodedInstruction> {
    Box::new(DataProcessing {
        opcode: if get_bit16(instruction, 9) { Opcode::SUB } else { Opcode::ADD },
        set_flags: true,
        d: get_bits16(instruction, 0, 3) as u8,
        n: get_bits16(instruction, 3, 3) as u8,
        shifter_operand: ShifterOperand::Immediate {
            immed_8: get_bits16(instruction, 6, 3) as u8,
            rotate_imm: 0,
        },
    })
}

pub fn decode_add_sub_compare_move_immediate_thumb(instruction: u16) -> Box<dyn DecodedInstruction> {
    use Opcode::*;
    Box::new({
        let d_n = get_bits16(instruction, 8, 3) as u8;
        DataProcessing {
            opcode: match get_bits16(instruction, 11, 2) {
                0b00 => MOV,
                0b01 => CMP,
                0b10 => ADD,
                0b11 => SUB,
                _ => unreachable!(),
            },
            set_flags: true,
            d: d_n,
            n: d_n,
            shifter_operand: ShifterOperand::Immediate {
                immed_8: get_bits16(instruction, 0, 8) as u8,
                rotate_imm: 0,
            },
        }
    })
}

#[derive(Debug)]
struct DataProcessing {
    opcode: Opcode,
    set_flags: bool,
    d: u8,
    n: u8,
    shifter_operand: ShifterOperand,
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

#[derive(Debug)]
enum ShifterOperand {
    Immediate { immed_8: u8, rotate_imm: u8 },
    Register { m: u8 },
    LogicalShiftLeftImmediate { m: u8, shift_imm: u8 },
    LogicalShiftLeftRegister { m: u8, s: u8 },
    LogicalShiftRightImmediate { m: u8, shift_imm: u8 },
    LogicalShiftRightRegister { m: u8, s: u8 },
    ArithmeticShiftRightImmediate { m: u8, shift_imm: u8 },
    ArithmeticShiftRightRegister { m: u8, s: u8 },
    RotateRightImmediate { m: u8, s: u8 },
    RotateRightRegister { m: u8, s: u8 },
    RotateRightWithExtend { m: u8 },
}

impl DecodedInstruction for DataProcessing {
    fn execute(&self, cpu: &mut CPU) {
        use Opcode::*;

        if self.set_flags && self.d == 15 {
            todo!("set_flags and d == 15");
        }

        let r_n = cpu.get_r(self.n);
        let (shifter_operand, mut carry) = self.shifter_operand.eval(cpu);

        let result = match self.opcode {
            AND => r_n & shifter_operand,
            EOR => r_n ^ shifter_operand,
            SUB => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(r_n, shifter_operand);
                carry = !borrow;
                cpu.set_overflow_flag(overflow);
                result
            }
            RSB => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(shifter_operand, r_n);
                carry = !borrow;
                cpu.set_overflow_flag(overflow);
                result
            }
            ADD => {
                let (result, add_carry, overflow) = bitutil::add_with_flags(r_n, shifter_operand);
                carry = add_carry;
                cpu.set_overflow_flag(overflow);
                result
            }
            TEQ => r_n ^ shifter_operand,
            CMP => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(r_n, shifter_operand);
                carry = !borrow;
                cpu.set_overflow_flag(overflow);
                result
            }
            MOV => shifter_operand,
            MVN => !shifter_operand,
            _ => todo!("opcode: {:?}", self.opcode),
        };

        cpu.set_r(self.d, result);
        if self.set_flags {
            cpu.set_negative_flag(get_bit(result, 31));
            cpu.set_zero_flag(result == 0);
            cpu.set_carry_flag(carry);
        }
    }

    fn disassemble(&self, cond: Condition) -> String {
        use Opcode::*;
        let has_d = !matches!(self.opcode, CMP | CMN | TST | TEQ);
        let has_n = !matches!(self.opcode, MOV | MVN);

        // <opcode>{<cond>}{S} <Rd>, <Rn>, <shifter_operand>
        format!(
            "{:?}{}{} {}{}{}",
            self.opcode,
            cond,
            if has_d && self.set_flags { "S" } else { "" },
            if has_d { format!("R{}, ", self.d) } else { "".into() },
            if has_n { format!("R{}, ", self.n) } else { "".into() },
            self.shifter_operand
        )
    }
}

impl Opcode {
    const fn decode_arm(instruction: u32) -> Opcode {
        match get_bits32(instruction, 21, 4) {
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

impl Display for ShifterOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ShifterOperand::Immediate { immed_8, rotate_imm } => write!(f, "#{:#X}", ShifterOperand::calc_immediate(immed_8, rotate_imm)),
            ShifterOperand::Register { m } => write!(f, "R{}", m),
            ShifterOperand::LogicalShiftLeftImmediate { m, shift_imm } => write!(f, "R{}, LSL #{:#X}", m, shift_imm),
            ShifterOperand::LogicalShiftLeftRegister { m, s } => write!(f, "R{}, LSL R{}", m, s),
            ShifterOperand::LogicalShiftRightImmediate { m, shift_imm } => write!(f, "R{}, LSR #{:#X}", m, shift_imm),
            ShifterOperand::LogicalShiftRightRegister { m, s } => write!(f, "R{}, LSR R{}", m, s),
            ShifterOperand::ArithmeticShiftRightImmediate { m, shift_imm } => write!(f, "R{}, ASR #{:#X}", m, shift_imm),
            ShifterOperand::ArithmeticShiftRightRegister { m, s } => write!(f, "R{}, ASR R{}", m, s),
            ShifterOperand::RotateRightImmediate { m, s } => write!(f, "R{}, ROR #{:#X}", m, s),
            ShifterOperand::RotateRightRegister { m, s } => write!(f, "R{}, ROR R{}", m, s),
            ShifterOperand::RotateRightWithExtend { m } => write!(f, "R{}, RRX", m),
        }
    }
}

impl ShifterOperand {
    const fn calc_immediate(immed_8: u8, rotate_imm: u8) -> u32 {
        (immed_8 as u32).rotate_right(2 * rotate_imm as u32)
    }

    const fn decode_arm(instruction: u32) -> ShifterOperand {
        let is_immediate = get_bit(instruction, 25);

        if is_immediate {
            ShifterOperand::Immediate {
                immed_8: get_bits32(instruction, 0, 8) as u8,
                rotate_imm: get_bits32(instruction, 8, 4) as u8,
            }
        } else {
            let m = get_bits32(instruction, 0, 4) as u8;
            let is_reg_shift = get_bit(instruction, 4);

            if is_reg_shift {
                let s = get_bits32(instruction, 8, 4) as u8;
                let shift_type = get_bits32(instruction, 5, 2);
                match shift_type {
                    0b00 => ShifterOperand::LogicalShiftLeftRegister { m, s },
                    0b01 => ShifterOperand::LogicalShiftRightRegister { m, s },
                    0b10 => ShifterOperand::ArithmeticShiftRightRegister { m, s },
                    0b11 => ShifterOperand::RotateRightRegister { m, s },
                    _ => unreachable!(),
                }
            } else {
                let shift_imm = get_bits32(instruction, 7, 5) as u8;
                let shift_type = get_bits32(instruction, 5, 2);
                match shift_type {
                    0b00 if shift_imm == 0 => ShifterOperand::Register { m },
                    0b00 => ShifterOperand::LogicalShiftLeftImmediate { m, shift_imm },
                    0b01 => ShifterOperand::LogicalShiftRightImmediate { m, shift_imm },
                    0b10 => ShifterOperand::ArithmeticShiftRightImmediate { m, shift_imm },
                    0b11 if shift_imm == 0 => ShifterOperand::RotateRightWithExtend { m },
                    0b11 => ShifterOperand::RotateRightImmediate { m, s: shift_imm },
                    _ => unreachable!(),
                }
            }
        }
    }

    fn eval(&self, cpu: &CPU) -> (u32, bool) {
        match *self {
            ShifterOperand::Immediate { immed_8, rotate_imm } => {
                let shifter_operand = ShifterOperand::calc_immediate(immed_8, rotate_imm);
                let carry = if rotate_imm == 0 { cpu.get_carry_flag() } else { get_bit(shifter_operand, 31) };
                (shifter_operand, carry)
            }
            ShifterOperand::Register { m } => (cpu.get_r(m), cpu.get_carry_flag()),
            ShifterOperand::LogicalShiftLeftImmediate { m, shift_imm } => {
                if shift_imm == 0 {
                    panic!("Should be ShifterOperand::Register");
                }
                let r_m = cpu.get_r(m);
                (r_m << shift_imm, get_bit(r_m, 32 - shift_imm))
            }
            ShifterOperand::LogicalShiftLeftRegister { m, s } => {
                let r_m = cpu.get_r(m);
                let r_s_lsb = cpu.get_r(s) as u8;
                if r_s_lsb == 0 {
                    (r_m, cpu.get_carry_flag())
                } else if r_s_lsb < 32 {
                    (r_m << r_s_lsb, get_bit(r_m, 32 - r_s_lsb))
                } else if r_s_lsb == 32 {
                    (0, get_bit(r_m, 0))
                } else {
                    (0, false)
                }
            }
            ShifterOperand::LogicalShiftRightImmediate { m, shift_imm } => {
                let r_m = cpu.get_r(m);
                if shift_imm == 0 {
                    (0, get_bit(r_m, 31))
                } else {
                    (r_m >> shift_imm, get_bit(r_m, shift_imm - 1))
                }
            }
            ShifterOperand::LogicalShiftRightRegister { m, s } => {
                let r_m = cpu.get_r(m);
                let r_s_lsb = cpu.get_r(s) as u8;
                if r_s_lsb == 0 {
                    (r_m, cpu.get_carry_flag())
                } else if r_s_lsb < 32 {
                    (r_m >> r_s_lsb, get_bit(r_m, r_s_lsb - 1))
                } else if r_s_lsb == 32 {
                    (0, get_bit(r_m, 31))
                } else {
                    (0, false)
                }
            }
            ShifterOperand::ArithmeticShiftRightImmediate { m, shift_imm } => {
                let r_m = cpu.get_r(m);
                let r_m_31 = get_bit(r_m, 31);
                if shift_imm == 0 {
                    if !r_m_31 {
                        (0, r_m_31)
                    } else {
                        (0xFFFFFFFF, r_m_31)
                    }
                } else {
                    (arithmetic_shift_right(r_m, shift_imm), get_bit(r_m, shift_imm - 1))
                }
            }
            ShifterOperand::ArithmeticShiftRightRegister { m, s } => {
                let r_m = cpu.get_r(m);
                let r_s_lsb = cpu.get_r(s) as u8;
                if r_s_lsb == 0 {
                    (r_m, cpu.get_carry_flag())
                } else if r_s_lsb < 32 {
                    (arithmetic_shift_right(r_m, r_s_lsb), get_bit(r_m, r_s_lsb - 1))
                } else {
                    let r_m_31 = get_bit(r_m, 31);
                    if !r_m_31 {
                        (0, r_m_31)
                    } else {
                        (0xFFFFFFFF, r_m_31)
                    }
                }
            }
            ShifterOperand::RotateRightImmediate { m, s: shift_imm } => {
                if shift_imm == 0 {
                    panic!("Should be ShifterOperand::RotateRightWithExtend");
                }
                let r_m = cpu.get_r(m);
                (r_m.rotate_right(shift_imm as u32), get_bit(r_m, shift_imm - 1))
            }
            ShifterOperand::RotateRightRegister { m, s } => {
                let r_m = cpu.get_r(m);
                let r_s_lsb = cpu.get_r(s) & 0xFF;
                let r_s_4_0 = r_s_lsb as u8 & 0b11111;
                if r_s_lsb == 0 {
                    (r_m, cpu.get_carry_flag())
                } else if r_s_4_0 == 0 {
                    (r_m, get_bit(r_m, 31))
                } else {
                    (r_m.rotate_right(r_s_4_0 as u32), get_bit(r_m, r_s_4_0 - 1))
                }
            }
            ShifterOperand::RotateRightWithExtend { m } => {
                let r_m = cpu.get_r(m);
                (rotate_right_with_extend(cpu.get_carry_flag(), r_m), get_bit(r_m, 0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mov() {
        let instruction = 0xe1a01000;
        let inst = decode_arm(instruction);
        assert_eq!("MOV R1, R0", format!("{}", inst.disassemble(Condition::AL)));
    }

    #[test]
    fn test_cmp() {
        let instruction = 0xe1500000;
        let inst = decode_arm(instruction);
        assert_eq!("CMPEQ R0, R0", format!("{}", inst.disassemble(Condition::EQ)));
    }

    #[test]
    fn test_add() {
        let instruction = 0xe0859185;
        let inst = decode_arm(instruction);
        assert_eq!("ADD R9, R5, R5, LSL #0x3", format!("{}", inst.disassemble(Condition::AL)));

        let instruction = 0xe2821f82;
        let inst = decode_arm(instruction);
        assert_eq!("ADD R1, R2, #0x208", format!("{}", inst.disassemble(Condition::AL)));
    }
}
