use std::fmt::Display;

use crate::{
    bitutil::{self, arithmetic_shift_right, get_bit, get_bit16, get_bits16, get_bits32, rotate_right_with_extend},
    system::cpu::{CPU, REGISTER_PC, REGISTER_SP},
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataProcessing {
    pub opcode: Opcode,
    pub set_flags: bool,
    pub d: u8,
    pub n: u8,
    pub shifter_operand: ShifterOperand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    AND,
    EOR,
    SUB,
    RSB,
    ADD,
    ADR,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    LSL,
    LSR,
    ASR,
    ROR,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShifterOperand {
    Immediate { immed: u16, rotate_imm: u8 },
    Register { m: u8 },
    ShiftImmediate { shift: Shift, m: u8, shift_imm: u8 },
    ShiftRegister { shift: Shift, m: u8, s: u8 },
}

#[inline(always)]
pub fn decode_arm(instruction: u32) -> Instruction {
    Instruction::DataProcessing(DataProcessing {
        opcode: match get_bits32(instruction, 21, 4) {
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
            _ => Opcode::MVN,
        },
        set_flags: get_bit(instruction, 20),
        d: get_bits32(instruction, 12, 4) as u8,
        n: get_bits32(instruction, 16, 4) as u8,
        shifter_operand: ShifterOperand::decode_arm(instruction),
    })
}

#[inline(always)]
pub fn decode_shift_imm_thumb(instruction: u16) -> Instruction {
    Instruction::DataProcessing(DataProcessing {
        opcode: Opcode::MOV,
        set_flags: true,
        d: get_bits16(instruction, 0, 3) as u8,
        n: 0,
        shifter_operand: ShifterOperand::ShiftImmediate {
            shift: Shift::decode(get_bits16(instruction, 11, 2) as u32),
            m: get_bits16(instruction, 3, 3) as u8,
            shift_imm: get_bits16(instruction, 6, 5) as u8,
        },
    })
}

#[inline(always)]
pub fn decode_add_sub_register_thumb(instruction: u16) -> Instruction {
    Instruction::DataProcessing(DataProcessing {
        opcode: if get_bit16(instruction, 9) { Opcode::SUB } else { Opcode::ADD },
        set_flags: true,
        d: get_bits16(instruction, 0, 3) as u8,
        n: get_bits16(instruction, 3, 3) as u8,
        shifter_operand: ShifterOperand::Register {
            m: get_bits16(instruction, 6, 3) as u8,
        },
    })
}

#[inline(always)]
pub fn decode_add_sub_immediate_thumb(instruction: u16) -> Instruction {
    Instruction::DataProcessing(DataProcessing {
        opcode: if get_bit16(instruction, 9) { Opcode::SUB } else { Opcode::ADD },
        set_flags: true,
        d: get_bits16(instruction, 0, 3) as u8,
        n: get_bits16(instruction, 3, 3) as u8,
        shifter_operand: ShifterOperand::Immediate {
            immed: get_bits16(instruction, 6, 3),
            rotate_imm: 0,
        },
    })
}

#[inline(always)]
pub fn decode_mov_cmp_add_sub_immediate_thumb(instruction: u16) -> Instruction {
    let d_n = get_bits16(instruction, 8, 3) as u8;
    Instruction::DataProcessing(DataProcessing {
        opcode: match get_bits16(instruction, 11, 2) {
            0b00 => Opcode::MOV,
            0b01 => Opcode::CMP,
            0b10 => Opcode::ADD,
            _ => Opcode::SUB,
        },
        set_flags: true,
        d: d_n,
        n: d_n,
        shifter_operand: ShifterOperand::Immediate {
            immed: get_bits16(instruction, 0, 8),
            rotate_imm: 0,
        },
    })
}

#[inline(always)]
pub fn decode_register_thumb(instruction: u16) -> Instruction {
    let d = get_bits16(instruction, 0, 3) as u8;
    let s = get_bits16(instruction, 3, 3) as u8;
    let register = ShifterOperand::Register { m: s };
    let (opcode, n, shifter_operand) = match get_bits16(instruction, 6, 4) {
        0b0000 => (Opcode::AND, d, register),
        0b0001 => (Opcode::EOR, d, register),
        0b0010 => (Opcode::MOV, d, ShifterOperand::ShiftRegister { shift: Shift::LSL, m: d, s }),
        0b0011 => (Opcode::MOV, d, ShifterOperand::ShiftRegister { shift: Shift::LSR, m: d, s }),
        0b0100 => (Opcode::MOV, d, ShifterOperand::ShiftRegister { shift: Shift::ASR, m: d, s }),
        0b0101 => (Opcode::ADC, d, register),
        0b0110 => (Opcode::SBC, d, register),
        0b0111 => (Opcode::MOV, d, ShifterOperand::ShiftRegister { shift: Shift::ROR, m: d, s }),
        0b1000 => (Opcode::TST, d, register),
        0b1001 => (Opcode::RSB, s, ShifterOperand::Immediate { immed: 0, rotate_imm: 0 }),
        0b1010 => (Opcode::CMP, d, register),
        0b1011 => (Opcode::CMN, d, register),
        0b1100 => (Opcode::ORR, d, register),
        0b1101 => return super::multiply::decode_mul_thumb(instruction),
        0b1110 => (Opcode::BIC, d, register),
        _ => (Opcode::MVN, d, register),
    };
    Instruction::DataProcessing(DataProcessing {
        opcode,
        set_flags: true,
        d,
        n,
        shifter_operand,
    })
}

#[inline(always)]
pub fn decode_special_thumb(instruction: u16) -> Instruction {
    let d = get_bits16(instruction, 0, 3) as u8 | (get_bit16(instruction, 7) as u8) << 3;
    let (opcode, set_flags) = match get_bits16(instruction, 8, 2) {
        0b00 => (Opcode::ADD, false),
        0b01 => (Opcode::CMP, true),
        _ => (Opcode::MOV, false),
    };
    Instruction::DataProcessing(DataProcessing {
        opcode,
        set_flags,
        d,
        n: d,
        shifter_operand: ShifterOperand::Register {
            m: get_bits16(instruction, 3, 4) as u8,
        },
    })
}

#[inline(always)]
pub fn decode_adjust_sp_thumb(instruction: u16) -> Instruction {
    Instruction::DataProcessing(DataProcessing {
        opcode: if get_bit16(instruction, 7) { Opcode::SUB } else { Opcode::ADD },
        set_flags: false,
        d: REGISTER_SP,
        n: REGISTER_SP,
        shifter_operand: ShifterOperand::Immediate {
            immed: get_bits16(instruction, 0, 7) << 2,
            rotate_imm: 0,
        },
    })
}

#[inline(always)]
pub fn decode_add_sp_pc_thumb(instruction: u16) -> Instruction {
    let is_sp = get_bit16(instruction, 11);
    Instruction::DataProcessing(DataProcessing {
        opcode: if is_sp { Opcode::ADD } else { Opcode::ADR },
        set_flags: false,
        d: get_bits16(instruction, 8, 3) as u8,
        n: if is_sp { REGISTER_SP } else { REGISTER_PC },
        shifter_operand: ShifterOperand::Immediate {
            immed: get_bits16(instruction, 0, 8) << 2,
            rotate_imm: 0,
        },
    })
}

impl DataProcessing {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU) {
        let (shifter_operand, shifter_carry) = self.shifter_operand.eval(cpu);
        let r_n = if self.n == REGISTER_PC && self.shifter_operand.is_register_shift() {
            cpu.get_r(REGISTER_PC).wrapping_add(4)
        } else {
            cpu.get_r(self.n)
        };
        let carry_in = cpu.get_carry_flag();
        let (result, carry, overflow) = match self.opcode {
            Opcode::AND | Opcode::TST => (r_n & shifter_operand, shifter_carry, None),
            Opcode::EOR | Opcode::TEQ => (r_n ^ shifter_operand, shifter_carry, None),
            Opcode::SUB | Opcode::CMP => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(r_n, shifter_operand);
                (result, !borrow, Some(overflow))
            }
            Opcode::RSB => {
                let (result, borrow, overflow) = bitutil::sub_with_flags(shifter_operand, r_n);
                (result, !borrow, Some(overflow))
            }
            Opcode::ADD | Opcode::CMN => {
                let (result, carry, overflow) = bitutil::add_with_flags(r_n, shifter_operand);
                (result, carry, Some(overflow))
            }
            Opcode::ADR => {
                let (result, carry, overflow) = bitutil::add_with_flags(r_n & !0b11, shifter_operand);
                (result, carry, Some(overflow))
            }
            Opcode::ADC => {
                let (result, carry, overflow) = bitutil::add_with_flags_carry(r_n, shifter_operand, carry_in);
                (result, carry, Some(overflow))
            }
            Opcode::SBC => {
                let (result, borrow, overflow) = bitutil::sub_with_flags_carry(r_n, shifter_operand, !carry_in);
                (result, !borrow, Some(overflow))
            }
            Opcode::RSC => {
                let (result, borrow, overflow) = bitutil::sub_with_flags_carry(shifter_operand, r_n, !carry_in);
                (result, !borrow, Some(overflow))
            }
            Opcode::ORR => (r_n | shifter_operand, shifter_carry, None),
            Opcode::MOV => (shifter_operand, shifter_carry, None),
            Opcode::BIC => (r_n & !shifter_operand, shifter_carry, None),
            Opcode::MVN => (!shifter_operand, shifter_carry, None),
        };

        if self.d == REGISTER_PC && self.set_flags {
            if cpu.current_mode_has_spsr() {
                cpu.set_cpsr(cpu.get_spsr());
            }
            if self.opcode.writes_result() {
                cpu.set_pc(result);
            }
        } else if self.opcode.writes_result() && self.d == REGISTER_PC {
            cpu.set_pc(result);
        } else {
            if self.opcode.writes_result() {
                cpu.set_r(self.d, result);
            }
            if self.set_flags {
                match overflow {
                    Some(overflow) => cpu.set_nzcv(result, carry, overflow),
                    None => cpu.set_nzc(result, carry),
                }
            }
        }
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!(
            "{}{}{} {}{}{}",
            self.opcode,
            cond,
            if self.opcode.writes_result() && self.set_flags { "S" } else { "" },
            if self.opcode.writes_result() { format!("R{}, ", self.d) } else { String::new() },
            if self.opcode.reads_n() { format!("R{}, ", self.n) } else { String::new() },
            self.shifter_operand
        )
    }
}

impl Opcode {
    const fn writes_result(self) -> bool {
        !matches!(self, Opcode::TST | Opcode::TEQ | Opcode::CMP | Opcode::CMN)
    }

    const fn reads_n(self) -> bool {
        !matches!(self, Opcode::MOV | Opcode::MVN | Opcode::ADR)
    }
}

impl Display for Opcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Shift {
    #[inline(always)]
    pub const fn decode(bits: u32) -> Shift {
        match bits & 0b11 {
            0b00 => Shift::LSL,
            0b01 => Shift::LSR,
            0b10 => Shift::ASR,
            _ => Shift::ROR,
        }
    }

    #[inline(always)]
    pub fn by_immediate(self, value: u32, shift_imm: u32, carry: bool) -> (u32, bool) {
        match self {
            Shift::LSL => {
                if shift_imm == 0 {
                    (value, carry)
                } else {
                    (value << shift_imm, get_bit(value, (32 - shift_imm) as u8))
                }
            }
            Shift::LSR => {
                if shift_imm == 0 {
                    (0, get_bit(value, 31))
                } else {
                    (value >> shift_imm, get_bit(value, (shift_imm - 1) as u8))
                }
            }
            Shift::ASR => {
                if shift_imm == 0 {
                    (arithmetic_shift_right(value, 31), get_bit(value, 31))
                } else {
                    (arithmetic_shift_right(value, shift_imm as u8), get_bit(value, (shift_imm - 1) as u8))
                }
            }
            Shift::ROR => {
                if shift_imm == 0 {
                    (rotate_right_with_extend(carry, value), get_bit(value, 0))
                } else {
                    (value.rotate_right(shift_imm), get_bit(value, (shift_imm - 1) as u8))
                }
            }
        }
    }

    #[inline(always)]
    pub fn by_register(self, value: u32, amount: u32, carry: bool) -> (u32, bool) {
        if amount == 0 {
            (value, carry)
        } else {
            match self {
                Shift::LSL => {
                    if amount < 32 {
                        (value << amount, get_bit(value, (32 - amount) as u8))
                    } else if amount == 32 {
                        (0, get_bit(value, 0))
                    } else {
                        (0, false)
                    }
                }
                Shift::LSR => {
                    if amount < 32 {
                        (value >> amount, get_bit(value, (amount - 1) as u8))
                    } else if amount == 32 {
                        (0, get_bit(value, 31))
                    } else {
                        (0, false)
                    }
                }
                Shift::ASR => {
                    if amount < 32 {
                        (arithmetic_shift_right(value, amount as u8), get_bit(value, (amount - 1) as u8))
                    } else {
                        (arithmetic_shift_right(value, 31), get_bit(value, 31))
                    }
                }
                Shift::ROR => {
                    let amount = amount & 0x1F;
                    if amount == 0 {
                        (value, get_bit(value, 31))
                    } else {
                        (value.rotate_right(amount), get_bit(value, (amount - 1) as u8))
                    }
                }
            }
        }
    }
}

impl Display for Shift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ShifterOperand {
    const fn calc_immediate(immed: u16, rotate_imm: u8) -> u32 {
        (immed as u32).rotate_right(rotate_imm as u32 * 2)
    }

    #[inline(always)]
    const fn decode_arm(instruction: u32) -> ShifterOperand {
        let m = get_bits32(instruction, 0, 4) as u8;
        let shift = Shift::decode(get_bits32(instruction, 5, 2));
        if get_bit(instruction, 25) {
            ShifterOperand::Immediate {
                immed: get_bits32(instruction, 0, 8) as u16,
                rotate_imm: get_bits32(instruction, 8, 4) as u8,
            }
        } else if get_bit(instruction, 4) {
            ShifterOperand::ShiftRegister {
                shift,
                m,
                s: get_bits32(instruction, 8, 4) as u8,
            }
        } else {
            ShifterOperand::ShiftImmediate {
                shift,
                m,
                shift_imm: get_bits32(instruction, 7, 5) as u8,
            }
        }
    }

    const fn is_register_shift(self) -> bool {
        matches!(self, ShifterOperand::ShiftRegister { .. })
    }

    #[inline(always)]
    pub fn eval(self, cpu: &CPU) -> (u32, bool) {
        let carry = cpu.get_carry_flag();
        match self {
            ShifterOperand::Immediate { immed, rotate_imm } => {
                let value = ShifterOperand::calc_immediate(immed, rotate_imm);
                (value, if rotate_imm == 0 { carry } else { get_bit(value, 31) })
            }
            ShifterOperand::Register { m } => (cpu.get_r(m), carry),
            ShifterOperand::ShiftImmediate { shift, m, shift_imm } => shift.by_immediate(cpu.get_r(m), shift_imm as u32, carry),
            ShifterOperand::ShiftRegister { shift, m, s } => {
                let r_m = cpu.get_r(m).wrapping_add(if m == REGISTER_PC { 4 } else { 0 });
                shift.by_register(r_m, cpu.get_r(s) & 0xFF, carry)
            }
        }
    }
}

impl Display for ShifterOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ShifterOperand::Immediate { immed, rotate_imm } => write!(f, "#{:#X}", ShifterOperand::calc_immediate(immed, rotate_imm)),
            ShifterOperand::Register { m } => write!(f, "R{}", m),
            ShifterOperand::ShiftImmediate { shift: Shift::LSL, m, shift_imm: 0 } => write!(f, "R{}", m),
            ShifterOperand::ShiftImmediate { shift: Shift::ROR, m, shift_imm: 0 } => write!(f, "R{}, RRX", m),
            ShifterOperand::ShiftImmediate { shift, m, shift_imm } => write!(f, "R{}, {} #{:#X}", m, shift, shift_imm),
            ShifterOperand::ShiftRegister { shift, m, s } => write!(f, "R{}, {} R{}", m, shift, s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mov() {
        let instruction = 0xe1a01000;
        let inst = Instruction::decode_arm(instruction);
        assert_eq!("MOV R1, R0", format!("{}", inst.disassemble(Condition::AL, 0)));
    }

    #[test]
    fn test_cmp() {
        let instruction = 0xe1500000;
        let inst = Instruction::decode_arm(instruction);
        assert_eq!("CMPEQ R0, R0", format!("{}", inst.disassemble(Condition::EQ, 0)));
    }

    #[test]
    fn test_add() {
        let instruction = 0xe0859185;
        let inst = Instruction::decode_arm(instruction);
        assert_eq!("ADD R9, R5, R5, LSL #0x3", format!("{}", inst.disassemble(Condition::AL, 0)));

        let instruction = 0xe2821f82;
        let inst = Instruction::decode_arm(instruction);
        assert_eq!("ADD R1, R2, #0x208", format!("{}", inst.disassemble(Condition::AL, 0)));
    }

    #[test]
    fn test_thumb_neg_is_rsb_from_zero() {
        assert_eq!("RSBS R0, R1, #0x0", Instruction::decode_thumb(0x4248).disassemble(Condition::AL, 0));
    }

    #[test]
    fn test_shift_by_register_edge_cases() {
        assert_eq!(Shift::LSL.by_register(1, 32, false), (0, true));
        assert_eq!(Shift::LSL.by_register(1, 33, true), (0, false));
        assert_eq!(Shift::LSR.by_register(0x8000_0000, 32, false), (0, true));
        assert_eq!(Shift::ASR.by_register(0x8000_0000, 40, false), (0xFFFF_FFFF, true));
        assert_eq!(Shift::ROR.by_register(0x8000_0001, 32, false), (0x8000_0001, true));
        assert_eq!(Shift::ROR.by_register(3, 1, false), (0x8000_0001, true));
        assert_eq!(Shift::ROR.by_register(3, 0, true), (3, true));
    }

    #[test]
    fn test_shift_by_immediate_zero_amounts() {
        assert_eq!(Shift::LSL.by_immediate(5, 0, true), (5, true));
        assert_eq!(Shift::LSR.by_immediate(0x8000_0000, 0, false), (0, true));
        assert_eq!(Shift::ASR.by_immediate(0x8000_0000, 0, false), (0xFFFF_FFFF, true));
        assert_eq!(Shift::ROR.by_immediate(1, 0, true), (0x8000_0000, true));
    }
}
