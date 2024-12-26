use std::fmt::Display;

use crate::{
    bitutil::{arithmetic_shift_right, get_bit, get_bit16, get_bits16, get_bits32, rotate_right_with_extend, sign_extend32},
    system::cpu::{CPU, REGISTER_SP},
};

use super::{Condition, DecodedInstruction};

pub fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
    let d = get_bits32(instruction, 12, 4) as u8;
    let b = get_bit(instruction, 22);
    Box::new(LoadStore {
        opcode: if get_bit(instruction, 20) { Opcode::LDR } else { Opcode::STR },
        length: if b { Length::Byte } else { Length::Word },
        sign_extend: false,
        d,
        adressing_mode: AddressingMode::decode_arm(instruction),
    })
}

pub fn decode_extra_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
    let d = get_bits32(instruction, 12, 4) as u8;
    let l = get_bit(instruction, 20);
    let s = get_bit(instruction, 6);
    let h = get_bit(instruction, 5);
    let (opcode, sign_extend, length) = match (l, s, h) {
        (false, false, true) => (Opcode::STR, false, Length::Halfword),
        (false, true, false) => (Opcode::LDR, false, Length::Doubleword),
        (false, true, true) => (Opcode::STR, false, Length::Doubleword),
        (true, false, true) => (Opcode::LDR, false, Length::Halfword),
        (true, true, false) => (Opcode::LDR, true, Length::Byte),
        (true, true, true) => (Opcode::LDR, true, Length::Halfword),
        _ => panic!("Invalid extra arm instruction: {:#08X}", instruction),
    };
    Box::new(LoadStore {
        opcode,
        length,
        sign_extend,
        d,
        adressing_mode: AddressingMode::decode_extra_arm(instruction),
    })
}

pub fn decode_halfword_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let is_load = get_bit16(instruction, 11);
    Box::new(LoadStore {
        opcode: if is_load { Opcode::LDR } else { Opcode::STR },
        length: Length::Halfword,
        sign_extend: false,
        d: get_bits16(instruction, 0, 3) as u8,
        adressing_mode: AddressingMode {
            u_is_add: true,
            n: get_bits16(instruction, 3, 3) as u8,
            mode: AddressingModeType::Immediate(get_bits16(instruction, 6, 5) as u16 * 2),
            indexing_mode: IndexingMode::Offset,
        },
    })
}

pub fn decode_word_byte_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let d = get_bits16(instruction, 0, 3) as u8;
    let b = get_bits16(instruction, 3, 3) as u8;
    let offset = get_bits16(instruction, 6, 5);
    let is_load = get_bit16(instruction, 11);
    let is_byte = get_bit16(instruction, 12);
    Box::new(LoadStore {
        opcode: if is_load { Opcode::LDR } else { Opcode::STR },
        length: if is_byte { Length::Byte } else { Length::Word },
        sign_extend: false,
        d,
        adressing_mode: AddressingMode {
            u_is_add: true,
            n: b,
            mode: AddressingModeType::Immediate(offset),
            indexing_mode: IndexingMode::Offset,
        },
    })
}

pub fn decode_stack_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let is_load = get_bit16(instruction, 11);
    Box::new(LoadStore {
        opcode: if is_load { Opcode::LDR } else { Opcode::STR },
        length: Length::Word,
        sign_extend: false,
        d: get_bits16(instruction, 8, 3) as u8,
        adressing_mode: AddressingMode {
            u_is_add: true,
            n: REGISTER_SP,
            mode: AddressingModeType::Immediate(get_bits16(instruction, 0, 3) as u16 * 4),
            indexing_mode: IndexingMode::Offset,
        },
    })
}

pub fn decode_load_from_literal_pool_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    Box::new(LoadStore {
        opcode: Opcode::LDR,
        length: Length::Word,
        sign_extend: false,
        d: get_bits16(instruction, 8, 3) as u8,
        adressing_mode: AddressingMode {
            u_is_add: true,
            n: 15,
            mode: AddressingModeType::Immediate(get_bits16(instruction, 0, 8) as u16 * 4),
            indexing_mode: IndexingMode::Offset,
        },
    })
}

pub fn decode_register_offset_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn DecodedInstruction> {
    let is_load = get_bit16(instruction, 11);

    Box::new(LoadStore {
        opcode: if is_load { Opcode::LDR } else { Opcode::STR },
        length: Length::Word,
        sign_extend: false,
        d: get_bits16(instruction, 0, 3) as u8,
        adressing_mode: AddressingMode {
            u_is_add: true,
            n: get_bits16(instruction, 3, 3) as u8,
            mode: AddressingModeType::Register {
                m: get_bits16(instruction, 6, 3) as u8,
            },
            indexing_mode: IndexingMode::Offset,
        },
    })
}

#[derive(Debug)]
struct LoadStore {
    opcode: Opcode,
    length: Length,
    sign_extend: bool,
    d: u8,
    adressing_mode: AddressingMode,
}

#[derive(Debug)]
enum Opcode {
    LDR,
    STR,
}

#[derive(Debug)]
enum Length {
    Byte,
    Halfword,
    Word,
    Doubleword,
}

#[derive(Debug)]
struct AddressingMode {
    u_is_add: bool,
    n: u8,
    mode: AddressingModeType,
    indexing_mode: IndexingMode,
}

#[derive(Debug)]
enum AddressingModeType {
    Immediate(u16),
    Register { m: u8 },
    LogicalShiftLeft { m: u8, shift_imm: u8 },
    LogicalShiftRight { m: u8, shift_imm: u8 },
    ArithmeticShiftRight { m: u8, shift_imm: u8 },
    RotateRight { m: u8, shift_imm: u8 },
    RotateRightWithExtend { m: u8 },
}

#[derive(Debug)]
enum IndexingMode {
    Offset,
    PreIndexed,
    PostIndexed { t: bool },
}

impl DecodedInstruction for LoadStore {
    fn execute(&self, cpu: &mut CPU) {
        if self.d == 15 {
            todo!("d == 15");
        }

        let address = self.adressing_mode.execute(cpu);

        match self.opcode {
            Opcode::LDR => match self.length {
                Length::Byte if self.sign_extend => cpu.set_r(self.d, sign_extend32(cpu.mem.read_u8(address) as u32, 8)),
                Length::Byte => cpu.set_r(self.d, cpu.mem.read_u8(address) as u32),
                Length::Halfword if self.sign_extend => cpu.set_r(self.d, sign_extend32(cpu.mem.read_u16(address) as u32, 16)),
                Length::Halfword => cpu.set_r(self.d, cpu.mem.read_u16(address) as u32),
                Length::Word => cpu.set_r(self.d, cpu.mem.read_u32(address)),
                Length::Doubleword => {
                    cpu.set_r(self.d, cpu.mem.read_u32(address));
                    cpu.set_r(self.d + 1, cpu.mem.read_u32(address + 4));
                }
            },
            Opcode::STR => match self.length {
                Length::Byte => cpu.mem.write_u8(address, cpu.get_r(self.d) as u8),
                Length::Halfword => cpu.mem.write_u16(address, cpu.get_r(self.d) as u16),
                Length::Word => cpu.mem.write_u32(address, cpu.get_r(self.d)),
                Length::Doubleword => {
                    cpu.mem.write_u32(address, cpu.get_r(self.d));
                    cpu.mem.write_u32(address + 4, cpu.get_r(self.d + 1));
                }
            },
        }
    }

    fn disassemble(&self, cond: Condition, _base_address: u32) -> String {
        let t = match self.adressing_mode.indexing_mode {
            IndexingMode::PostIndexed { t } => t,
            _ => false,
        };

        format!(
            "{:?}{}{}{}{} R{}, {}",
            self.opcode,
            cond,
            if self.sign_extend { "S" } else { "" },
            match self.length {
                Length::Byte => "B",
                Length::Halfword => "H",
                Length::Word => "",
                Length::Doubleword => "D",
            },
            if t { "T" } else { "" },
            self.d,
            self.adressing_mode
        )
    }
}

impl AddressingMode {
    fn decode_arm(instruction: u32) -> AddressingMode {
        let u = get_bit(instruction, 23);
        let n = get_bits32(instruction, 16, 4) as u8;

        AddressingMode {
            u_is_add: u,
            n,
            mode: {
                use AddressingModeType::*;
                let is_scaled_register = get_bit(instruction, 25);
                match is_scaled_register {
                    false => Immediate(get_bits32(instruction, 0, 12) as u16),
                    true => {
                        let m = get_bits32(instruction, 0, 4) as u8;
                        let shift = get_bits32(instruction, 5, 2) as u8;
                        let shift_imm = get_bits32(instruction, 7, 5) as u8;
                        match shift {
                            0b00 if shift_imm == 0 => Register { m },
                            0b00 => LogicalShiftLeft { m, shift_imm },
                            0b01 => LogicalShiftRight { m, shift_imm },
                            0b10 => ArithmeticShiftRight { m, shift_imm },
                            0b11 if shift_imm == 0 => RotateRightWithExtend { m },
                            0b11 => RotateRight { m, shift_imm },
                            _ => unreachable!(),
                        }
                    }
                }
            },
            indexing_mode: {
                let p = get_bit(instruction, 24);
                let w = get_bit(instruction, 21);
                match (p, w) {
                    (false, t) => IndexingMode::PostIndexed { t },
                    (true, false) => IndexingMode::Offset,
                    (true, true) => IndexingMode::PreIndexed,
                }
            },
        }
    }

    fn decode_extra_arm(instruction: u32) -> AddressingMode {
        let u = get_bit(instruction, 23);
        let n = get_bits32(instruction, 16, 4) as u8;

        AddressingMode {
            u_is_add: u,
            n,
            mode: {
                let is_immediate = get_bit(instruction, 22);
                match is_immediate {
                    true => AddressingModeType::Immediate((get_bits32(instruction, 8, 4) as u16) << 4 | get_bits32(instruction, 0, 4) as u16),
                    false => AddressingModeType::Register {
                        m: get_bits32(instruction, 0, 4) as u8,
                    },
                }
            },
            indexing_mode: {
                let p = get_bit(instruction, 24);
                let w = get_bit(instruction, 21);
                match (p, w) {
                    (false, t) => IndexingMode::PostIndexed { t },
                    (true, false) => IndexingMode::Offset,
                    (true, true) => IndexingMode::PreIndexed,
                }
            },
        }
    }

    fn execute(&self, cpu: &mut CPU) -> u32 {
        use AddressingModeType::*;
        let offset = match self.mode {
            Immediate(imm) => imm as u32,
            Register { m } => cpu.get_r(m),
            LogicalShiftLeft { m, shift_imm } => cpu.get_r(m) << shift_imm,
            LogicalShiftRight { m, shift_imm } => {
                if shift_imm == 0 {
                    0
                } else {
                    cpu.get_r(m) >> shift_imm
                }
            }
            ArithmeticShiftRight { m, shift_imm } => {
                if shift_imm == 0 {
                    if get_bit(cpu.get_r(m), 31) {
                        0xFFFFFFFF
                    } else {
                        0
                    }
                } else {
                    arithmetic_shift_right(cpu.get_r(m), shift_imm)
                }
            }
            RotateRight { m, shift_imm } => cpu.get_r(m).rotate_right(shift_imm.into()),
            RotateRightWithExtend { m } => rotate_right_with_extend(cpu.get_carry_flag(), cpu.get_r(m)),
        };

        // If n == 15, we need to mask the bottom two bits of the PC for Thumb mode
        let r_n = if self.n == 15 { cpu.get_r(self.n) & !0b11u32 } else { cpu.get_r(self.n) };
        let r_n_offset = if self.u_is_add { r_n.wrapping_add(offset) } else { r_n.wrapping_sub(offset) };

        match self.indexing_mode {
            IndexingMode::Offset => r_n_offset,
            IndexingMode::PreIndexed => {
                cpu.set_r(self.n, r_n_offset);
                r_n_offset
            }
            IndexingMode::PostIndexed { .. } => {
                cpu.set_r(self.n, r_n_offset);
                r_n
            }
        }
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use AddressingModeType::*;
        let rhs = match self.mode {
            Immediate(imm) => format!("#{}{:X}", if self.u_is_add { "+" } else { "-" }, imm),
            Register { m } => format!("R{}", m),
            LogicalShiftLeft { m, shift_imm } => format!("R{}, LSL #{:X}", m, shift_imm),
            LogicalShiftRight { m, shift_imm } => format!("R{}, LSR #{:X}", m, shift_imm),
            ArithmeticShiftRight { m, shift_imm } => format!("R{}, ASR #{:X}", m, shift_imm),
            RotateRight { m, shift_imm } => format!("R{}, ROR #{:X}", m, shift_imm),
            RotateRightWithExtend { m } => format!("R{}, RRX", m),
        };

        let n = self.n;
        match self.indexing_mode {
            IndexingMode::Offset => write!(f, "[R{}, {}]", n, rhs),
            IndexingMode::PreIndexed => write!(f, "[R{}, {}]!", n, rhs),
            IndexingMode::PostIndexed { .. } => write!(f, "[R{}], {}", rhs, n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strb() {
        let strb = decode_arm(0xe5c33208);
        assert_eq!(format!("{}", strb.disassemble(Condition::EQ, 0)), "STREQB R3, [R3, #+0x208]");
    }

    #[test]
    fn test_ldrsd() {
        let instruction = decode_extra_arm(0xe17670f1);
        assert_eq!(format!("{}", instruction.disassemble(Condition::EQ, 0)), "LDREQSH R7, [R6, #-0x1]!");
    }

    #[test]
    fn test_strh_thumb() {
        let instruction = decode_halfword_thumb(0x8021, 0);
        assert_eq!(format!("{}", instruction.disassemble(Condition::AL, 0)), "STRH R1, [R4, #+0x0]");
    }
}
