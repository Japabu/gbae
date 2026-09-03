use std::fmt::Display;

use crate::{
    bits::Bits,
    system::{
        cpu::{Register, CPU},
        memory::{Access, Memory},
    },
};

use super::{
    data_processing::{Shift, ShifterOperand},
    Condition, Instruction,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadStore {
    pub opcode: Opcode,
    pub length: Length,
    pub sign_extend: bool,
    pub d: Register,
    pub addressing: AddressingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swap {
    pub byte: bool,
    pub d: Register,
    pub m: Register,
    pub n: Register,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    LDR,
    STR,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    Byte,
    Halfword,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressingMode {
    pub n: Register,
    pub add: bool,
    pub offset: ShifterOperand,
    pub indexing: Indexing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indexing {
    Offset,
    PreIndexed,
    PostIndexed,
}

#[inline(always)]
pub fn decode_arm(word: u32) -> Instruction {
    Instruction::LoadStore(LoadStore {
        opcode: Opcode::from_load_bit(word.bit(20)),
        length: if word.bit(22) { Length::Byte } else { Length::Word },
        sign_extend: false,
        d: Register::from(word.bits(12..16)),
        addressing: AddressingMode::decode_arm(word),
    })
}

#[inline(always)]
pub fn decode_extra_arm(word: u32) -> Instruction {
    let (opcode, sign_extend, length) = match (word.bit(20), word.bit(6), word.bit(5)) {
        (false, false, true) => (Opcode::STR, false, Length::Halfword),
        (true, false, true) => (Opcode::LDR, false, Length::Halfword),
        (true, true, false) => (Opcode::LDR, true, Length::Byte),
        (true, true, true) => (Opcode::LDR, true, Length::Halfword),
        _ => return Instruction::Unknown(word),
    };
    Instruction::LoadStore(LoadStore {
        opcode,
        length,
        sign_extend,
        d: Register::from(word.bits(12..16)),
        addressing: AddressingMode::decode_extra_arm(word),
    })
}

#[inline(always)]
pub fn decode_swap_arm(word: u32) -> Instruction {
    Instruction::Swap(Swap {
        byte: word.bit(22),
        d: Register::from(word.bits(12..16)),
        m: Register::from(word.bits(0..4)),
        n: Register::from(word.bits(16..20)),
    })
}

#[inline(always)]
fn thumb(opcode: Opcode, length: Length, sign_extend: bool, d: Register, n: Register, offset: ShifterOperand) -> Instruction {
    Instruction::LoadStore(LoadStore {
        opcode,
        length,
        sign_extend,
        d,
        addressing: AddressingMode {
            n,
            add: true,
            offset,
            indexing: Indexing::Offset,
        },
    })
}

#[inline(always)]
pub fn decode_halfword_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    thumb(
        Opcode::from_load_bit(word.bit(11)),
        Length::Halfword,
        false,
        Register::from(word.bits(0..3)),
        Register::from(word.bits(3..6)),
        ShifterOperand::immediate(word.bits(6..11) * 2),
    )
}

#[inline(always)]
pub fn decode_word_byte_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let (length, scale) = if word.bit(12) { (Length::Byte, 1) } else { (Length::Word, 4) };
    thumb(
        Opcode::from_load_bit(word.bit(11)),
        length,
        false,
        Register::from(word.bits(0..3)),
        Register::from(word.bits(3..6)),
        ShifterOperand::immediate(word.bits(6..11) * scale),
    )
}

#[inline(always)]
pub fn decode_stack_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    thumb(
        Opcode::from_load_bit(word.bit(11)),
        Length::Word,
        false,
        Register::from(word.bits(8..11)),
        Register::SP,
        ShifterOperand::immediate(word.bits(0..8) * 4),
    )
}

#[inline(always)]
pub fn decode_load_from_literal_pool_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    thumb(
        Opcode::LDR,
        Length::Word,
        false,
        Register::from(word.bits(8..11)),
        Register::PC,
        ShifterOperand::immediate(word.bits(0..8) * 4),
    )
}

#[inline(always)]
pub fn decode_register_offset_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let (opcode, sign_extend, length) = match word.bits(9..12) {
        0b000 => (Opcode::STR, false, Length::Word),
        0b001 => (Opcode::STR, false, Length::Halfword),
        0b010 => (Opcode::STR, false, Length::Byte),
        0b011 => (Opcode::LDR, true, Length::Byte),
        0b100 => (Opcode::LDR, false, Length::Word),
        0b101 => (Opcode::LDR, false, Length::Halfword),
        0b110 => (Opcode::LDR, false, Length::Byte),
        _ => (Opcode::LDR, true, Length::Halfword),
    };
    thumb(
        opcode,
        length,
        sign_extend,
        Register::from(word.bits(0..3)),
        Register::from(word.bits(3..6)),
        ShifterOperand::Register(Register::from(word.bits(6..9))),
    )
}

impl Opcode {
    #[inline(always)]
    fn from_load_bit(load: bool) -> Opcode {
        if load {
            Opcode::LDR
        } else {
            Opcode::STR
        }
    }

    fn load_bit(self) -> u32 {
        u32::from(self == Opcode::LDR)
    }
}

impl LoadStore {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let value = cpu.r(self.d).wrapping_add(if self.d == Register::PC { cpu.instruction_length() } else { 0 });
        let address = self.addressing.execute(cpu);
        let access = Access::Nonsequential;
        match (self.opcode, self.length, self.sign_extend) {
            (Opcode::LDR, Length::Word, _) => {
                let value = mem.load_u32(address, access).rotate_right(address.bits(0..2) * 8);
                if self.d == Register::PC {
                    cpu.set_pc(value);
                } else {
                    cpu.set_r(self.d, value);
                }
            }
            (Opcode::LDR, Length::Halfword, false) => cpu.set_r(self.d, u32::from(mem.load_u16(address, access)).rotate_right(address.bits(0..1) * 8)),
            (Opcode::LDR, Length::Halfword, true) => {
                let value = if address.bit(0) {
                    u32::from(mem.load_u8(address, access)).sign_extended(8)
                } else {
                    u32::from(mem.load_u16(address, access)).sign_extended(16)
                };
                cpu.set_r(self.d, value);
            }
            (Opcode::LDR, Length::Byte, false) => cpu.set_r(self.d, u32::from(mem.load_u8(address, access))),
            (Opcode::LDR, Length::Byte, true) => cpu.set_r(self.d, u32::from(mem.load_u8(address, access)).sign_extended(8)),
            (Opcode::STR, Length::Word, _) => mem.store_u32(address, value, access),
            (Opcode::STR, Length::Halfword, _) => mem.store_u16(address, value as u16, access),
            (Opcode::STR, Length::Byte, _) => mem.store_u8(address, value as u8, access),
        }
        if self.opcode == Opcode::LDR {
            mem.idle(1);
        }
    }

    pub fn encode_arm(self) -> Option<u32> {
        let addressing = self.addressing;
        let common = addressing.encode_arm() | self.opcode.load_bit() << 20 | self.d.number() << 12;
        match (self.length, self.sign_extend) {
            (Length::Word | Length::Byte, false) => {
                let offset = match addressing.offset {
                    ShifterOperand::Immediate { value, .. } if value < 0x1000 => value,
                    ShifterOperand::Register(m) => 1 << 25 | m.number(),
                    ShifterOperand::ShiftImmediate { shift, m, amount } => 1 << 25 | amount << 7 | shift.bits() << 5 | m.number(),
                    _ => return None,
                };
                Some(0b01 << 26 | common | u32::from(self.length == Length::Byte) << 22 | offset)
            }
            (Length::Halfword, _) | (Length::Byte, true) => {
                if self.opcode == Opcode::STR && self.sign_extend {
                    return None;
                }
                let (immediate, offset) = match addressing.offset {
                    ShifterOperand::Immediate { value, .. } if value < 0x100 => (true, value.bits(4..8) << 8 | value.bits(0..4)),
                    ShifterOperand::Register(m) => (false, m.number()),
                    _ => return None,
                };
                Some(common | u32::from(immediate) << 22 | 1 << 7 | u32::from(self.sign_extend) << 6 | u32::from(self.length == Length::Halfword) << 5 | 1 << 4 | offset)
            }
            (Length::Word, true) => None,
        }
    }

    pub fn encode_thumb(self) -> Option<u16> {
        let AddressingMode { n, add, offset, indexing } = self.addressing;
        if indexing != Indexing::Offset || !add {
            return None;
        }
        let (d, load) = (self.d, self.opcode.load_bit());
        let low = |register: Register| register.is_low();
        let word = match (offset, self.length, self.sign_extend) {
            (ShifterOperand::Immediate { value, .. }, Length::Word, false) if n == Register::PC && self.opcode == Opcode::LDR && low(d) && value % 4 == 0 && value < 1024 => {
                0b01001 << 11 | d.number() << 8 | value / 4
            }
            (ShifterOperand::Immediate { value, .. }, Length::Word, false) if n == Register::SP && low(d) && value % 4 == 0 && value < 1024 => 0b1001 << 12 | load << 11 | d.number() << 8 | value / 4,
            (ShifterOperand::Immediate { value, .. }, Length::Word, false) if low(d) && low(n) && value % 4 == 0 && value < 128 => {
                0b011 << 13 | load << 11 | (value / 4) << 6 | n.number() << 3 | d.number()
            }
            (ShifterOperand::Immediate { value, .. }, Length::Byte, false) if low(d) && low(n) && value < 32 => 0b011 << 13 | 1 << 12 | load << 11 | value << 6 | n.number() << 3 | d.number(),
            (ShifterOperand::Immediate { value, .. }, Length::Halfword, false) if low(d) && low(n) && value % 2 == 0 && value < 64 => {
                0b1000 << 12 | load << 11 | (value / 2) << 6 | n.number() << 3 | d.number()
            }
            (ShifterOperand::Register(m), length, sign_extend) if low(d) && low(n) && low(m) => {
                let opcode = match (self.opcode, length, sign_extend) {
                    (Opcode::STR, Length::Word, false) => 0b000,
                    (Opcode::STR, Length::Halfword, false) => 0b001,
                    (Opcode::STR, Length::Byte, false) => 0b010,
                    (Opcode::LDR, Length::Byte, true) => 0b011,
                    (Opcode::LDR, Length::Word, false) => 0b100,
                    (Opcode::LDR, Length::Halfword, false) => 0b101,
                    (Opcode::LDR, Length::Byte, false) => 0b110,
                    (Opcode::LDR, Length::Halfword, true) => 0b111,
                    _ => return None,
                };
                0b0101 << 12 | opcode << 9 | m.number() << 6 | n.number() << 3 | d.number()
            }
            _ => return None,
        };
        u16::try_from(word).ok()
    }

    pub fn disassemble(self, cond: Condition) -> String {
        let length = match self.length {
            Length::Byte => "B",
            Length::Halfword => "H",
            Length::Word => "",
        };
        format!("{:?}{}{}{} {}, {}", self.opcode, cond, if self.sign_extend { "S" } else { "" }, length, self.d, self.addressing)
    }
}

impl Swap {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let address = cpu.r(self.n);
        let r_m = cpu.r(self.m);
        let access = Access::Nonsequential;
        if self.byte {
            let old = mem.load_u8(address, access);
            mem.store_u8(address, r_m as u8, access);
            cpu.set_r(self.d, u32::from(old));
        } else {
            let old = mem.load_u32(address, access).rotate_right(address.bits(0..2) * 8);
            mem.store_u32(address, r_m, access);
            cpu.set_r(self.d, old);
        }
        mem.idle(1);
    }

    pub fn encode_arm(self) -> u32 {
        0b00010 << 23 | u32::from(self.byte) << 22 | self.n.number() << 16 | self.d.number() << 12 | 0b1001 << 4 | self.m.number()
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("SWP{}{} {}, {}, [{}]", cond, if self.byte { "B" } else { "" }, self.d, self.m, self.n)
    }
}

impl AddressingMode {
    #[inline(always)]
    fn decode_arm(word: u32) -> AddressingMode {
        AddressingMode {
            n: Register::from(word.bits(16..20)),
            add: word.bit(23),
            offset: if word.bit(25) {
                ShifterOperand::ShiftImmediate {
                    shift: Shift::from_bits(word.bits(5..7)),
                    m: Register::from(word.bits(0..4)),
                    amount: word.bits(7..12),
                }
            } else {
                ShifterOperand::immediate(word.bits(0..12))
            },
            indexing: Indexing::decode_arm(word),
        }
    }

    #[inline(always)]
    fn decode_extra_arm(word: u32) -> AddressingMode {
        AddressingMode {
            n: Register::from(word.bits(16..20)),
            add: word.bit(23),
            offset: if word.bit(22) {
                ShifterOperand::immediate(word.bits(8..12) << 4 | word.bits(0..4))
            } else {
                ShifterOperand::Register(Register::from(word.bits(0..4)))
            },
            indexing: Indexing::decode_arm(word),
        }
    }

    fn encode_arm(self) -> u32 {
        let (pre, writeback) = match self.indexing {
            Indexing::Offset => (true, false),
            Indexing::PreIndexed => (true, true),
            Indexing::PostIndexed => (false, false),
        };
        u32::from(pre) << 24 | u32::from(self.add) << 23 | u32::from(writeback) << 21 | self.n.number() << 16
    }

    #[inline(always)]
    fn execute(self, cpu: &mut CPU) -> u32 {
        let offset = self.offset.eval(cpu).0;
        let r_n = if self.n == Register::PC { cpu.r(Register::PC) & !0b11 } else { cpu.r(self.n) };
        let offset_address = if self.add { r_n.wrapping_add(offset) } else { r_n.wrapping_sub(offset) };
        match self.indexing {
            Indexing::Offset => offset_address,
            Indexing::PreIndexed => {
                cpu.set_r(self.n, offset_address);
                offset_address
            }
            Indexing::PostIndexed => {
                cpu.set_r(self.n, offset_address);
                r_n
            }
        }
    }
}

impl Indexing {
    #[inline(always)]
    fn decode_arm(word: u32) -> Indexing {
        match (word.bit(24), word.bit(21)) {
            (false, _) => Indexing::PostIndexed,
            (true, false) => Indexing::Offset,
            (true, true) => Indexing::PreIndexed,
        }
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.add { "+" } else { "-" };
        let offset = match self.offset {
            ShifterOperand::Immediate { value, .. } => format!("#{}{:#X}", sign, value),
            offset => format!("{}{}", sign, offset),
        };
        match self.indexing {
            Indexing::Offset => write!(f, "[{}, {}]", self.n, offset),
            Indexing::PreIndexed => write!(f, "[{}, {}]!", self.n, offset),
            Indexing::PostIndexed => write!(f, "[{}], {}", self.n, offset),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strb() {
        assert_eq!(Instruction::decode_arm(0xE5C3_3208).disassemble(Condition::EQ, 0), "STREQB R3, [R3, #+0x208]");
    }

    #[test]
    fn test_ldrsh() {
        assert_eq!(Instruction::decode_arm(0xE176_70F1).disassemble(Condition::EQ, 0), "LDREQSH R7, [R6, #-0x1]!");
    }

    #[test]
    fn test_strh_thumb() {
        assert_eq!(Instruction::decode_thumb(0x8021).disassemble(Condition::AL, 0), "STRH R1, [R4, #+0x0]");
    }

    #[test]
    fn test_ldr_scaled_register_post_indexed() {
        assert_eq!(Instruction::decode_arm(0xE692_1103).disassemble(Condition::AL, 0), "LDR R1, [R2], +R3, LSL #0x2");
    }

    #[test]
    fn test_encoding_matches_known_words() {
        for word in [0xE5C3_3208, 0xE176_70F1, 0xE692_1103, 0xE100_0090] {
            assert_eq!(Instruction::decode_arm(word).encode_arm(Condition::AL), Some(word), "{:08X}", word);
        }
        for word in [0x8021, 0x4801, 0x5800, 0x9801, 0x6801, 0x7801] {
            assert_eq!(Instruction::decode_thumb(word).encode_thumb(), Some(word), "{:04X}", word);
        }
    }
}
