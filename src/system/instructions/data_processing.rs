use std::fmt::Display;

use crate::{
    bitutil::{self, arithmetic_shift_right, get_bit, get_bit16, get_bits16, get_bits32, rotate_right_with_extend},
    system::cpu::{CPU, REGISTER_SP},
};

use super::{Condition, DecodedInstruction};

pub fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
    let d = get_bits32(instruction, 12, 4) as u8;
    let n = get_bits32(instruction, 16, 4) as u8;
    Box::new(DataProcessing {
        opcode: match get_bits32(instruction, 21, 4) {
            0b0000 => Opcode::AND { d, n },
            0b0001 => Opcode::EOR { d, n },
            0b0010 => Opcode::SUB { d, n },
            0b0011 => Opcode::RSB { d, n },
            0b0100 => Opcode::ADD { d, n },
            0b0101 => Opcode::ADC { d, n },
            0b0110 => Opcode::SBC { d, n },
            0b0111 => Opcode::RSC { d, n },
            0b1000 => Opcode::TST { n },
            0b1001 => Opcode::TEQ { n },
            0b1010 => Opcode::CMP { n },
            0b1011 => Opcode::CMN { n },
            0b1100 => Opcode::ORR { d, n },
            0b1101 => Opcode::MOV { d },
            0b1110 => Opcode::BIC { d, n },
            0b1111 => Opcode::MVN { d },
            _ => unreachable!(),
        },
        set_flags: get_bit(instruction, 20),

        shifter_operand: ShifterOperand::decode_arm(instruction),
    })
}

pub fn decode_shift_imm_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let m = get_bits16(instruction, 3, 3) as u8;
    let shift_imm = get_bits16(instruction, 6, 5) as u8;
    Box::new(DataProcessing {
        opcode: Opcode::MOV {
            d: get_bits16(instruction, 0, 3) as u8,
        },
        set_flags: true,
        shifter_operand: match get_bits16(instruction, 11, 2) {
            0b00 => ShifterOperand::LogicalShiftLeftImmediate { m, shift_imm },
            0b01 => ShifterOperand::LogicalShiftRightImmediate { m, shift_imm },
            0b10 => ShifterOperand::ArithmeticShiftRightImmediate { m, shift_imm },
            _ => panic!("decode_shift_imm_thumb: Unknown shift type"),
        },
    })
}

pub fn decode_register_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let d = get_bits16(instruction, 0, 3) as u8;
    let m = get_bits16(instruction, 3, 3) as u8;
    let (opcode, shifter_operand) = match get_bits16(instruction, 6, 4) {
        0b1000 => (Opcode::TST { n: d }, ShifterOperand::Register { m }),
        0b1111 => (Opcode::MVN { d }, ShifterOperand::Register { m }),
        x => todo!("Thumb opcode {:#04b}", x),
    };
    Box::new(DataProcessing {
        opcode,
        set_flags: true,
        shifter_operand,
    })
}

pub fn decode_add_sub_register_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let d = get_bits16(instruction, 0, 3) as u8;
    let n = get_bits16(instruction, 3, 3) as u8;
    Box::new(DataProcessing {
        opcode: if get_bit16(instruction, 9) { Opcode::SUB { d, n } } else { Opcode::ADD { d, n } },
        set_flags: true,
        shifter_operand: ShifterOperand::Register {
            m: get_bits16(instruction, 6, 3) as u8,
        },
    })
}

pub fn decode_add_sub_immediate_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let d = get_bits16(instruction, 0, 3) as u8;
    let n = get_bits16(instruction, 3, 3) as u8;
    Box::new(DataProcessing {
        opcode: if get_bit16(instruction, 9) { Opcode::SUB { d, n } } else { Opcode::ADD { d, n } },
        set_flags: true,
        shifter_operand: ShifterOperand::Immediate {
            immed: get_bits16(instruction, 6, 3),
            rotate_imm: 0,
        },
    })
}

pub fn decode_mov_cmp_add_sub_immediate_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let d_n = get_bits16(instruction, 8, 3) as u8;
    Box::new(DataProcessing {
        opcode: match get_bits16(instruction, 11, 2) {
            0b00 => Opcode::MOV { d: d_n },
            0b01 => Opcode::CMP { n: d_n },
            0b10 => Opcode::ADD { d: d_n, n: d_n },
            0b11 => Opcode::SUB { d: d_n, n: d_n },
            _ => unreachable!(),
        },
        set_flags: true,
        shifter_operand: ShifterOperand::Immediate {
            immed: get_bits16(instruction, 0, 8),
            rotate_imm: 0,
        },
    })
}

pub fn decode_adjust_sp_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let d = REGISTER_SP;
    let n = REGISTER_SP;
    Box::new(DataProcessing {
        opcode: if get_bit16(instruction, 7) { Opcode::SUB { d, n } } else { Opcode::ADD { d, n } },
        set_flags: false,
        shifter_operand: ShifterOperand::Immediate {
            immed: get_bits16(instruction, 0, 7) << 2,
            rotate_imm: 0,
        },
    })
}

#[derive(Debug)]
struct DataProcessing {
    opcode: Opcode,
    set_flags: bool,
    shifter_operand: ShifterOperand,
}

#[derive(Debug)]
enum Opcode {
    AND { d: u8, n: u8 },
    EOR { d: u8, n: u8 },
    SUB { d: u8, n: u8 },
    RSB { d: u8, n: u8 },
    ADD { d: u8, n: u8 },
    ADC { d: u8, n: u8 },
    SBC { d: u8, n: u8 },
    RSC { d: u8, n: u8 },
    TST { n: u8 },
    TEQ { n: u8 },
    CMP { n: u8 },
    CMN { n: u8 },
    ORR { d: u8, n: u8 },
    MOV { d: u8 },
    BIC { d: u8, n: u8 },
    MVN { d: u8 },
}

#[derive(Debug)]
enum ShifterOperand {
    Immediate { immed: u16, rotate_imm: u8 },
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

        let process_result = |cpu: &mut CPU, d: Option<u8>, result: u32, carry: bool, overflow: Option<bool>| {
            if let Some(d) = d {
                if self.set_flags && d == 15 {
                    todo!("d == 15");
                }
                cpu.set_r(d, result);
            }
            if self.set_flags {
                cpu.set_negative_flag(get_bit(result, 31));
                cpu.set_zero_flag(result == 0);
                cpu.set_carry_flag(carry);
                if let Some(overflow) = overflow {
                    cpu.set_overflow_flag(overflow);
                }
            }
        };

        let (shifter_operand, shifter_carry) = self.shifter_operand.eval(cpu);
        match self.opcode {
            AND { d, n } => process_result(cpu, Some(d), cpu.get_r(n) & shifter_operand, shifter_carry, None),
            EOR { d, n } => process_result(cpu, Some(d), cpu.get_r(n) ^ shifter_operand, shifter_carry, None),
            SUB { d, n } => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(cpu.get_r(n), shifter_operand);
                process_result(cpu, Some(d), result, !borrow, Some(overflow));
            }
            RSB { d, n } => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(shifter_operand, cpu.get_r(n));
                process_result(cpu, Some(d), result, !borrow, Some(overflow))
            }
            ADD { d, n } => {
                let (result, carry, overflow) = bitutil::add_with_flags(cpu.get_r(n), shifter_operand);
                process_result(cpu, Some(d), result, carry, Some(overflow))
            }
            ADC { d, n } => {
                let (result, carry, overflow) = bitutil::add_with_flags_carry(cpu.get_r(n), shifter_operand, cpu.get_carry_flag());
                process_result(cpu, Some(d), result, carry, Some(overflow))
            }
            SBC { d, n } => {
                let (result, borrow, overflow) = bitutil::sub_with_flags_carry(cpu.get_r(n), shifter_operand, !cpu.get_carry_flag());
                process_result(cpu, Some(d), result, !borrow, Some(overflow))
            }
            RSC { d, n } => {
                let (result, borrow, overflow) = bitutil::sub_with_flags_carry(shifter_operand, cpu.get_r(n), !cpu.get_carry_flag());
                process_result(cpu, Some(d), result, !borrow, Some(overflow))
            }
            TST { n } => process_result(cpu, None, cpu.get_r(n) & shifter_operand, shifter_carry, None),
            TEQ { n } => process_result(cpu, None, cpu.get_r(n) ^ shifter_operand, shifter_carry, None),
            CMP { n } => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(cpu.get_r(n), shifter_operand);
                process_result(cpu, None, result, !borrow, Some(overflow));
            }
            CMN { n } => {
                let (result, add_carry, overflow) = bitutil::add_with_flags(cpu.get_r(n), shifter_operand);
                process_result(cpu, None, result, add_carry, Some(overflow));
            }
            ORR { d, n } => process_result(cpu, Some(d), cpu.get_r(n) | shifter_operand, shifter_carry, None),
            MOV { d } => process_result(cpu, Some(d), shifter_operand, shifter_carry, None),
            BIC { d, n } => process_result(cpu, Some(d), cpu.get_r(n) & !shifter_operand, shifter_carry, None),
            MVN { d } => process_result(cpu, Some(d), !shifter_operand, shifter_carry, None),
        }
    }

    fn disassemble(&self, cond: Condition) -> String {
        use Opcode::*;
        let (d, n) = match self.opcode {
            AND { d, n } | EOR { d, n } | SUB { d, n } | RSB { d, n } | ADD { d, n } | ADC { d, n } | SBC { d, n } | RSC { d, n } | ORR { d, n } | BIC { d, n } => (Some(d), Some(n)),
            TST { n } | TEQ { n } | CMP { n } | CMN { n } => (None, Some(n)),
            MOV { d } | MVN { d } => (Some(d), None),
        };

        format!(
            "{}{}{} {}{}{}",
            self.opcode,
            cond,
            if d.is_some() && self.set_flags { "S" } else { "" },
            d.map_or(String::new(), |d| format!("R{}, ", d)),
            n.map_or(String::new(), |n| format!("R{}, ", n)),
            self.shifter_operand
        )
    }
}

impl Display for ShifterOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ShifterOperand::Immediate { immed, rotate_imm } => write!(f, "#{:#X}", ShifterOperand::calc_immediate(immed, rotate_imm)),
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

impl Display for Opcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Opcode::AND { .. } => write!(f, "AND"),
            Opcode::EOR { .. } => write!(f, "EOR"),
            Opcode::SUB { .. } => write!(f, "SUB"),
            Opcode::RSB { .. } => write!(f, "RSB"),
            Opcode::ADD { .. } => write!(f, "ADD"),
            Opcode::ADC { .. } => write!(f, "ADC"),
            Opcode::SBC { .. } => write!(f, "SBC"),
            Opcode::RSC { .. } => write!(f, "RSC"),
            Opcode::TST { .. } => write!(f, "TST"),
            Opcode::TEQ { .. } => write!(f, "TEQ"),
            Opcode::CMP { .. } => write!(f, "CMP"),
            Opcode::CMN { .. } => write!(f, "CMN"),
            Opcode::ORR { .. } => write!(f, "ORR"),
            Opcode::MOV { .. } => write!(f, "MOV"),
            Opcode::BIC { .. } => write!(f, "BIC"),
            Opcode::MVN { .. } => write!(f, "MVN"),
        }
    }
}

impl ShifterOperand {
    const fn calc_immediate(immed: u16, rotate_imm: u8) -> u32 {
        (immed as u32).rotate_right(rotate_imm as u32 * 2)
    }

    const fn decode_arm(instruction: u32) -> ShifterOperand {
        let is_immediate = get_bit(instruction, 25);

        if is_immediate {
            ShifterOperand::Immediate {
                immed: get_bits32(instruction, 0, 8) as u16,
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
            ShifterOperand::Immediate { immed, rotate_imm } => {
                let shifter_operand = ShifterOperand::calc_immediate(immed, rotate_imm);
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
