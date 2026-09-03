use std::fmt::Display;

use crate::{
    bits::{Arithmetic, Bits},
    system::{
        cpu::{Register, CPU},
        memory::Memory,
    },
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataProcessing {
    pub opcode: Opcode,
    pub set_flags: bool,
    pub d: Register,
    pub n: Register,
    pub operand: ShifterOperand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
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
    ADR,
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
    Immediate { value: u32, rotate: u32 },
    Register(Register),
    ShiftImmediate { shift: Shift, m: Register, amount: u32 },
    ShiftRegister { shift: Shift, m: Register, s: Register },
}

#[inline(always)]
pub fn decode_arm(word: u32) -> Instruction {
    Instruction::DataProcessing(DataProcessing {
        opcode: Opcode::from_bits(word.bits(21..25)),
        set_flags: word.bit(20),
        d: Register::from(word.bits(12..16)),
        n: Register::from(word.bits(16..20)),
        operand: ShifterOperand::decode_arm(word),
    })
}

#[inline(always)]
pub fn decode_shift_imm_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::DataProcessing(DataProcessing {
        opcode: Opcode::MOV,
        set_flags: true,
        d: Register::from(word.bits(0..3)),
        n: Register::R0,
        operand: ShifterOperand::ShiftImmediate {
            shift: Shift::from_bits(word.bits(11..13)),
            m: Register::from(word.bits(3..6)),
            amount: word.bits(6..11),
        },
    })
}

#[inline(always)]
pub fn decode_add_sub_register_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::DataProcessing(DataProcessing {
        opcode: if word.bit(9) { Opcode::SUB } else { Opcode::ADD },
        set_flags: true,
        d: Register::from(word.bits(0..3)),
        n: Register::from(word.bits(3..6)),
        operand: ShifterOperand::Register(Register::from(word.bits(6..9))),
    })
}

#[inline(always)]
pub fn decode_add_sub_immediate_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::DataProcessing(DataProcessing {
        opcode: if word.bit(9) { Opcode::SUB } else { Opcode::ADD },
        set_flags: true,
        d: Register::from(word.bits(0..3)),
        n: Register::from(word.bits(3..6)),
        operand: ShifterOperand::immediate(word.bits(6..9)),
    })
}

#[inline(always)]
pub fn decode_mov_cmp_add_sub_immediate_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let d = Register::from(word.bits(8..11));
    Instruction::DataProcessing(DataProcessing {
        opcode: match word.bits(11..13) {
            0b00 => Opcode::MOV,
            0b01 => Opcode::CMP,
            0b10 => Opcode::ADD,
            _ => Opcode::SUB,
        },
        set_flags: true,
        d,
        n: d,
        operand: ShifterOperand::immediate(word.bits(0..8)),
    })
}

#[inline(always)]
pub fn decode_register_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let d = Register::from(word.bits(0..3));
    let s = Register::from(word.bits(3..6));
    let register = ShifterOperand::Register(s);
    let shifted = |shift| ShifterOperand::ShiftRegister { shift, m: d, s };
    let (opcode, n, operand) = match word.bits(6..10) {
        0b0000 => (Opcode::AND, d, register),
        0b0001 => (Opcode::EOR, d, register),
        0b0010 => (Opcode::MOV, d, shifted(Shift::LSL)),
        0b0011 => (Opcode::MOV, d, shifted(Shift::LSR)),
        0b0100 => (Opcode::MOV, d, shifted(Shift::ASR)),
        0b0101 => (Opcode::ADC, d, register),
        0b0110 => (Opcode::SBC, d, register),
        0b0111 => (Opcode::MOV, d, shifted(Shift::ROR)),
        0b1000 => (Opcode::TST, d, register),
        0b1001 => (Opcode::RSB, s, ShifterOperand::immediate(0)),
        0b1010 => (Opcode::CMP, d, register),
        0b1011 => (Opcode::CMN, d, register),
        0b1100 => (Opcode::ORR, d, register),
        0b1101 => return super::multiply::decode_mul_thumb(word as u16),
        0b1110 => (Opcode::BIC, d, register),
        _ => (Opcode::MVN, d, register),
    };
    Instruction::DataProcessing(DataProcessing {
        opcode,
        set_flags: true,
        d,
        n,
        operand,
    })
}

#[inline(always)]
pub fn decode_special_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let d = Register::from(word.bits(0..3) | word.bits(7..8) << 3);
    let (opcode, set_flags) = match word.bits(8..10) {
        0b00 => (Opcode::ADD, false),
        0b01 => (Opcode::CMP, true),
        _ => (Opcode::MOV, false),
    };
    Instruction::DataProcessing(DataProcessing {
        opcode,
        set_flags,
        d,
        n: d,
        operand: ShifterOperand::Register(Register::from(word.bits(3..7))),
    })
}

#[inline(always)]
pub fn decode_adjust_sp_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    Instruction::DataProcessing(DataProcessing {
        opcode: if word.bit(7) { Opcode::SUB } else { Opcode::ADD },
        set_flags: false,
        d: Register::SP,
        n: Register::SP,
        operand: ShifterOperand::immediate(word.bits(0..7) << 2),
    })
}

#[inline(always)]
pub fn decode_add_sp_pc_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let (opcode, n) = if word.bit(11) { (Opcode::ADD, Register::SP) } else { (Opcode::ADR, Register::PC) };
    Instruction::DataProcessing(DataProcessing {
        opcode,
        set_flags: false,
        d: Register::from(word.bits(8..11)),
        n,
        operand: ShifterOperand::immediate(word.bits(0..8) << 2),
    })
}

impl DataProcessing {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        if self.operand.is_register_shift() {
            mem.idle(1);
        }
        let (operand, shifter_carry) = self.operand.eval(cpu);
        let r_n = if self.n == Register::PC && self.operand.is_register_shift() {
            cpu.r(Register::PC).wrapping_add(4)
        } else {
            cpu.r(self.n)
        };
        let carry_in = cpu.cpsr().carry();
        let arithmetic = |arithmetic: Arithmetic| (arithmetic.result, arithmetic.carry, Some(arithmetic.overflow));
        let (result, carry, overflow) = match self.opcode {
            Opcode::AND | Opcode::TST => (r_n & operand, shifter_carry, None),
            Opcode::EOR | Opcode::TEQ => (r_n ^ operand, shifter_carry, None),
            Opcode::SUB | Opcode::CMP => arithmetic(Arithmetic::sub(r_n, operand)),
            Opcode::RSB => arithmetic(Arithmetic::sub(operand, r_n)),
            Opcode::ADD | Opcode::CMN => arithmetic(Arithmetic::add(r_n, operand)),
            Opcode::ADR => arithmetic(Arithmetic::add(r_n & !0b11, operand)),
            Opcode::ADC => arithmetic(Arithmetic::add_with_carry(r_n, operand, carry_in)),
            Opcode::SBC => arithmetic(Arithmetic::sub_with_carry(r_n, operand, carry_in)),
            Opcode::RSC => arithmetic(Arithmetic::sub_with_carry(operand, r_n, carry_in)),
            Opcode::ORR => (r_n | operand, shifter_carry, None),
            Opcode::MOV => (operand, shifter_carry, None),
            Opcode::BIC => (r_n & !operand, shifter_carry, None),
            Opcode::MVN => (!operand, shifter_carry, None),
        };

        if self.d == Register::PC && self.set_flags {
            if cpu.has_spsr() {
                cpu.set_cpsr(cpu.spsr());
            }
            if self.opcode.writes_result() {
                cpu.set_pc(result);
            }
        } else if self.opcode.writes_result() && self.d == Register::PC {
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

    pub fn encode_arm(self) -> Option<u32> {
        let (opcode, n) = match self.opcode {
            Opcode::ADR => (Opcode::ADD, Register::PC),
            opcode => (opcode, self.n),
        };
        let set_flags = self.set_flags || !opcode.writes_result();
        Some(self.operand.encode_arm()? | opcode.bits() << 21 | u32::from(set_flags) << 20 | n.number() << 16 | self.d.number() << 12)
    }

    pub fn encode_thumb(self) -> Option<u16> {
        let (d, n) = (self.d, self.n);
        let low = |register: Register| register.is_low();
        let word = match (self.opcode, self.operand, self.set_flags) {
            (Opcode::MOV, ShifterOperand::ShiftImmediate { shift, m, amount }, true) if low(d) && low(m) && shift != Shift::ROR => shift.bits() << 11 | amount << 6 | m.number() << 3 | d.number(),
            (Opcode::ADD | Opcode::SUB, ShifterOperand::Register(m), true) if low(d) && low(n) && low(m) => {
                0b11 << 11 | u32::from(self.opcode == Opcode::SUB) << 9 | m.number() << 6 | n.number() << 3 | d.number()
            }
            (Opcode::ADD | Opcode::SUB, ShifterOperand::Immediate { value, .. }, true) if low(d) && low(n) && value < 8 => {
                0b11 << 11 | 1 << 10 | u32::from(self.opcode == Opcode::SUB) << 9 | value << 6 | n.number() << 3 | d.number()
            }
            (Opcode::MOV | Opcode::CMP | Opcode::ADD | Opcode::SUB, ShifterOperand::Immediate { value, .. }, true) if low(d) && n == d && value < 256 => {
                let opcode = match self.opcode {
                    Opcode::MOV => 0b00,
                    Opcode::CMP => 0b01,
                    Opcode::ADD => 0b10,
                    _ => 0b11,
                };
                0b001 << 13 | opcode << 11 | d.number() << 8 | value
            }
            (opcode, ShifterOperand::Register(m), true) if low(d) && n == d && low(m) && opcode.thumb_alu_bits().is_some() => {
                0b010000 << 10 | opcode.thumb_alu_bits()? << 6 | m.number() << 3 | d.number()
            }
            (Opcode::MOV, ShifterOperand::ShiftRegister { shift, m, s }, true) if low(d) && n == d && m == d && low(s) => {
                let opcode = match shift {
                    Shift::LSL => 0b0010,
                    Shift::LSR => 0b0011,
                    Shift::ASR => 0b0100,
                    Shift::ROR => 0b0111,
                };
                0b010000 << 10 | opcode << 6 | s.number() << 3 | d.number()
            }
            (Opcode::RSB, ShifterOperand::Immediate { value: 0, .. }, true) if low(d) && low(n) => 0b010000 << 10 | 0b1001 << 6 | n.number() << 3 | d.number(),
            (Opcode::ADD | Opcode::CMP | Opcode::MOV, ShifterOperand::Register(m), set_flags) if n == d && set_flags == (self.opcode == Opcode::CMP) => {
                let opcode = match self.opcode {
                    Opcode::ADD => 0b00,
                    Opcode::CMP => 0b01,
                    _ => 0b10,
                };
                0b010001 << 10 | opcode << 8 | d.number().bits(3..4) << 7 | m.number() << 3 | d.number().bits(0..3)
            }
            (Opcode::ADD, ShifterOperand::Immediate { value, .. }, false) if low(d) && n == Register::SP && value % 4 == 0 && value < 1024 => 0b1_0101 << 11 | d.number() << 8 | (value / 4),
            (Opcode::ADR, ShifterOperand::Immediate { value, .. }, false) if low(d) && n == Register::PC && value % 4 == 0 && value < 1024 => 0b1_0100 << 11 | d.number() << 8 | (value / 4),
            (Opcode::ADD | Opcode::SUB, ShifterOperand::Immediate { value, .. }, false) if d == Register::SP && n == Register::SP && value % 4 == 0 && value < 512 => {
                0b1011_0000 << 8 | u32::from(self.opcode == Opcode::SUB) << 7 | (value / 4)
            }
            _ => return None,
        };
        u16::try_from(word).ok()
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!(
            "{}{}{} {}{}{}",
            self.opcode,
            cond,
            if self.opcode.writes_result() && self.set_flags { "S" } else { "" },
            if self.opcode.writes_result() { format!("{}, ", self.d) } else { String::new() },
            if self.opcode.reads_n() { format!("{}, ", self.n) } else { String::new() },
            self.operand
        )
    }
}

impl Opcode {
    const ARM: [Opcode; 16] = [
        Opcode::AND,
        Opcode::EOR,
        Opcode::SUB,
        Opcode::RSB,
        Opcode::ADD,
        Opcode::ADC,
        Opcode::SBC,
        Opcode::RSC,
        Opcode::TST,
        Opcode::TEQ,
        Opcode::CMP,
        Opcode::CMN,
        Opcode::ORR,
        Opcode::MOV,
        Opcode::BIC,
        Opcode::MVN,
    ];

    #[inline(always)]
    fn from_bits(bits: u32) -> Opcode {
        Opcode::ARM[bits as usize]
    }

    fn bits(self) -> u32 {
        Opcode::ARM.iter().position(|opcode| *opcode == self).expect("ADR is encoded as ADD") as u32
    }

    fn thumb_alu_bits(self) -> Option<u32> {
        match self {
            Opcode::AND => Some(0b0000),
            Opcode::EOR => Some(0b0001),
            Opcode::ADC => Some(0b0101),
            Opcode::SBC => Some(0b0110),
            Opcode::TST => Some(0b1000),
            Opcode::CMP => Some(0b1010),
            Opcode::CMN => Some(0b1011),
            Opcode::ORR => Some(0b1100),
            Opcode::BIC => Some(0b1110),
            Opcode::MVN => Some(0b1111),
            _ => None,
        }
    }

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
    const ALL: [Shift; 4] = [Shift::LSL, Shift::LSR, Shift::ASR, Shift::ROR];

    #[inline(always)]
    pub fn from_bits(bits: u32) -> Shift {
        Shift::ALL[bits.bits(0..2) as usize]
    }

    pub fn bits(self) -> u32 {
        self as u32
    }

    #[inline(always)]
    pub fn by_immediate(self, value: u32, amount: u32, carry: bool) -> (u32, bool) {
        match (self, amount) {
            (Shift::LSL, 0) => (value, carry),
            (Shift::LSL, _) => (value << amount, value.bit(32 - amount)),
            (Shift::LSR, 0) => (0, value.bit(31)),
            (Shift::LSR, _) => (value >> amount, value.bit(amount - 1)),
            (Shift::ASR, 0) => (value.arithmetic_shift_right(31), value.bit(31)),
            (Shift::ASR, _) => (value.arithmetic_shift_right(amount), value.bit(amount - 1)),
            (Shift::ROR, 0) => (rotate_right_extended(value, carry), value.bit(0)),
            (Shift::ROR, _) => (value.rotate_right(amount), value.bit(amount - 1)),
        }
    }

    #[inline(always)]
    pub fn by_register(self, value: u32, amount: u32, carry: bool) -> (u32, bool) {
        match (self, amount) {
            (_, 0) => (value, carry),
            (Shift::LSL, 1..=31) => (value << amount, value.bit(32 - amount)),
            (Shift::LSL, 32) => (0, value.bit(0)),
            (Shift::LSL, _) => (0, false),
            (Shift::LSR, 1..=31) => (value >> amount, value.bit(amount - 1)),
            (Shift::LSR, 32) => (0, value.bit(31)),
            (Shift::LSR, _) => (0, false),
            (Shift::ASR, 1..=31) => (value.arithmetic_shift_right(amount), value.bit(amount - 1)),
            (Shift::ASR, _) => (value.arithmetic_shift_right(31), value.bit(31)),
            (Shift::ROR, _) => match amount.bits(0..5) {
                0 => (value, value.bit(31)),
                amount => (value.rotate_right(amount), value.bit(amount - 1)),
            },
        }
    }
}

fn rotate_right_extended(value: u32, carry: bool) -> u32 {
    u32::from(carry) << 31 | value >> 1
}

impl Display for Shift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ShifterOperand {
    pub const fn immediate(value: u32) -> ShifterOperand {
        ShifterOperand::Immediate { value, rotate: 0 }
    }

    #[inline(always)]
    fn decode_arm(word: u32) -> ShifterOperand {
        let m = Register::from(word.bits(0..4));
        let shift = Shift::from_bits(word.bits(5..7));
        if word.bit(25) {
            let rotate = word.bits(8..12);
            ShifterOperand::Immediate {
                value: word.bits(0..8).rotate_right(rotate * 2),
                rotate,
            }
        } else if word.bit(4) {
            ShifterOperand::ShiftRegister {
                shift,
                m,
                s: Register::from(word.bits(8..12)),
            }
        } else {
            ShifterOperand::ShiftImmediate { shift, m, amount: word.bits(7..12) }
        }
    }

    pub fn arm_immediate(value: u32, preferred_rotate: u32) -> Option<(u32, u32)> {
        std::iter::once(preferred_rotate)
            .chain(0..16)
            .map(|rotate| (value.rotate_left(rotate * 2), rotate))
            .find(|(byte, _)| *byte <= 0xFF)
    }

    fn encode_arm(self) -> Option<u32> {
        Some(match self {
            ShifterOperand::Immediate { value, rotate } => {
                let (byte, rotate) = ShifterOperand::arm_immediate(value, rotate)?;
                1 << 25 | rotate << 8 | byte
            }
            ShifterOperand::Register(m) => m.number(),
            ShifterOperand::ShiftImmediate { shift, m, amount } => amount << 7 | shift.bits() << 5 | m.number(),
            ShifterOperand::ShiftRegister { shift, m, s } => s.number() << 8 | shift.bits() << 5 | 1 << 4 | m.number(),
        })
    }

    const fn is_register_shift(self) -> bool {
        matches!(self, ShifterOperand::ShiftRegister { .. })
    }

    #[inline(always)]
    pub fn eval(self, cpu: &CPU) -> (u32, bool) {
        let carry = cpu.cpsr().carry();
        match self {
            ShifterOperand::Immediate { value, rotate } => (value, if rotate == 0 { carry } else { value.bit(31) }),
            ShifterOperand::Register(m) => (cpu.r(m), carry),
            ShifterOperand::ShiftImmediate { shift, m, amount } => shift.by_immediate(cpu.r(m), amount, carry),
            ShifterOperand::ShiftRegister { shift, m, s } => {
                let r_m = cpu.r(m).wrapping_add(if m == Register::PC { 4 } else { 0 });
                shift.by_register(r_m, cpu.r(s).bits(0..8), carry)
            }
        }
    }
}

impl Display for ShifterOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ShifterOperand::Immediate { value, .. } => write!(f, "#{:#X}", value),
            ShifterOperand::Register(m) => write!(f, "{}", m),
            ShifterOperand::ShiftImmediate { shift: Shift::LSL, m, amount: 0 } => write!(f, "{}", m),
            ShifterOperand::ShiftImmediate { shift: Shift::ROR, m, amount: 0 } => write!(f, "{}, RRX", m),
            ShifterOperand::ShiftImmediate { shift, m, amount } => write!(f, "{}, {} #{:#X}", m, shift, amount),
            ShifterOperand::ShiftRegister { shift, m, s } => write!(f, "{}, {} {}", m, shift, s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mov() {
        assert_eq!(Instruction::decode_arm(0xE1A0_1000).disassemble(Condition::AL, 0), "MOV R1, R0");
    }

    #[test]
    fn test_cmp() {
        assert_eq!(Instruction::decode_arm(0xE150_0000).disassemble(Condition::EQ, 0), "CMPEQ R0, R0");
    }

    #[test]
    fn test_add() {
        assert_eq!(Instruction::decode_arm(0xE085_9185).disassemble(Condition::AL, 0), "ADD R9, R5, R5, LSL #0x3");
        assert_eq!(Instruction::decode_arm(0xE282_1F82).disassemble(Condition::AL, 0), "ADD R1, R2, #0x208");
    }

    #[test]
    fn test_thumb_neg_is_rsb_from_zero() {
        assert_eq!(Instruction::decode_thumb(0x4248).disassemble(Condition::AL, 0), "RSBS R0, R1, #0x0");
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

    #[test]
    fn test_arm_immediates_are_rotated_bytes() {
        assert_eq!(ShifterOperand::arm_immediate(0x208, 0), Some((0x82, 15)));
        assert_eq!(ShifterOperand::arm_immediate(0xFF00_0000, 0), Some((0xFF, 4)));
        assert_eq!(ShifterOperand::arm_immediate(0x101, 0), None);
    }

    #[test]
    fn test_encoding_matches_known_words() {
        let mov = DataProcessing {
            opcode: Opcode::MOV,
            set_flags: false,
            d: Register::R1,
            n: Register::R0,
            operand: ShifterOperand::Register(Register::R0),
        };
        assert_eq!(Instruction::DataProcessing(mov).encode_arm(Condition::AL), Some(0xE1A0_1000));
        assert_eq!(Instruction::decode_arm(0xE282_1F82).encode_arm(Condition::AL), Some(0xE282_1F82));
        assert_eq!(Instruction::decode_thumb(0x4248).encode_thumb(), Some(0x4248));
        assert_eq!(Instruction::decode_thumb(0xB081).encode_thumb(), Some(0xB081));
    }
}
