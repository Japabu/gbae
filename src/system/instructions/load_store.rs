use std::fmt::Display;

use crate::{
    bitutil::{arithmetic_shift_right, get_bit, get_bits16, get_bits32, rotate_right_with_extend, sign_extend32},
    system::cpu::CPU,
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

pub fn decode_thumb_load_from_literal_pool(instruction: u16) -> Box<dyn DecodedInstruction> {
    Box::new(LoadStore {
        opcode: Opcode::LDR,
        length: Length::Word,
        sign_extend: false,
        d: get_bits16(instruction, 8, 3) as u8,
        adressing_mode: AddressingMode {
            u: true,
            n: 15,
            mode: AddressingModeType::Immediate(get_bits16(instruction, 0, 8) as u16 * 4),
            indexing_mode: IndexingMode::Offset,
        },
    })
}

pub fn decode_thumb_load_store_register_offset(instruction: u16) -> Box<dyn DecodedInstruction> {
    todo!()
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
    u: bool,
    n: u8,
    mode: AddressingModeType,
    indexing_mode: IndexingMode,
}

#[derive(Debug)]
enum AddressingModeType {
    Immediate(u16),
    ScaledRegister(ScaledRegister),
}

#[derive(Debug, Clone, Copy)]
struct ScaledRegister {
    m: u8,
    mode: ScaledRegisterMode,
}

#[derive(Debug, Clone, Copy)]
enum ScaledRegisterMode {
    Register,
    LogicalShiftLeft { shift_imm: u8 },
    LogicalShiftRight { shift_imm: u8 },
    ArithmeticShiftRight { shift_imm: u8 },
    RotateRight { shift_imm: u8 },
    RotateRightWithExtend,
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

    fn disassemble(&self, cond: Condition) -> String {
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
            u,
            n,
            mode: {
                let is_scaled_register = get_bit(instruction, 25);
                match is_scaled_register {
                    false => AddressingModeType::Immediate(get_bits32(instruction, 0, 12) as u16),
                    true => AddressingModeType::ScaledRegister(ScaledRegister::decode_arm(instruction)),
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
            u,
            n,
            mode: {
                let is_immediate = get_bit(instruction, 22);
                match is_immediate {
                    true => AddressingModeType::Immediate((get_bits32(instruction, 8, 4) as u16) << 4 | get_bits32(instruction, 0, 4) as u16),
                    false => AddressingModeType::ScaledRegister(ScaledRegister {
                        m: get_bits32(instruction, 0, 4) as u8,
                        mode: ScaledRegisterMode::Register,
                    }),
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
        let offset = match self.mode {
            AddressingModeType::Immediate(imm) => imm as u32,
            AddressingModeType::ScaledRegister(scaled_register) => scaled_register.calc_address(cpu),
        };

        // If n == 15, we need to mask the bottom two bits of the PC for Thumb mode
        let r_n = if self.n == 15 { cpu.get_r(self.n) & !0b11u32 } else { cpu.get_r(self.n) };
        let r_n_offset = match self.u {
            false => r_n.wrapping_sub(offset),
            true => r_n.wrapping_add(offset),
        };

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

impl ScaledRegister {
    fn decode_arm(instruction: u32) -> ScaledRegister {
        ScaledRegister {
            m: get_bits32(instruction, 0, 4) as u8,
            mode: {
                use ScaledRegisterMode::*;
                let shift = get_bits32(instruction, 5, 2) as u8;
                let shift_imm = get_bits32(instruction, 7, 5) as u8;
                match shift {
                    0b00 if shift_imm == 0 => Register,
                    0b00 => LogicalShiftLeft { shift_imm },
                    0b01 => LogicalShiftRight { shift_imm },
                    0b10 => ArithmeticShiftRight { shift_imm },
                    0b11 if shift_imm == 0 => RotateRightWithExtend,
                    0b11 => RotateRight { shift_imm },
                    _ => unreachable!(),
                }
            },
        }
    }

    fn calc_address(&self, cpu: &CPU) -> u32 {
        use ScaledRegisterMode::*;
        let r_m = cpu.get_r(self.m);
        match self.mode {
            Register => r_m,
            LogicalShiftLeft { shift_imm } => r_m << shift_imm,
            LogicalShiftRight { shift_imm } => {
                if shift_imm == 0 {
                    0
                } else {
                    r_m >> shift_imm
                }
            }
            ArithmeticShiftRight { shift_imm } => {
                if shift_imm == 0 {
                    if get_bit(r_m, 31) {
                        0xFFFFFFFF
                    } else {
                        0
                    }
                } else {
                    arithmetic_shift_right(r_m, shift_imm)
                }
            }
            RotateRight { shift_imm } => r_m.rotate_right(shift_imm.into()),
            RotateRightWithExtend => rotate_right_with_extend(cpu.get_carry_flag(), r_m),
        }
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.u { "+" } else { "-" };
        let rhs = match &self.mode {
            AddressingModeType::Immediate(imm) => format!("#{}{:#X}", sign, imm),
            AddressingModeType::ScaledRegister(scaled_register) => format!("{}{}", sign, scaled_register),
        };

        let n = self.n;
        match self.indexing_mode {
            IndexingMode::Offset => write!(f, "[R{}, {}]", n, rhs),
            IndexingMode::PreIndexed => write!(f, "[R{}, {}]!", n, rhs),
            IndexingMode::PostIndexed { .. } => write!(f, "[R{}], {}", rhs, n),
        }
    }
}

impl Display for ScaledRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ScaledRegisterMode::*;

        write!(f, "R{}", self.m)?;

        match self.mode {
            Register => Ok(()),
            LogicalShiftLeft { shift_imm } => write!(f, ", LSL #{:#X}", shift_imm),
            LogicalShiftRight { shift_imm } => write!(f, ", LSR #{:#X}", shift_imm),
            ArithmeticShiftRight { shift_imm } => write!(f, ", ASR #{:#X}", shift_imm),
            RotateRight { shift_imm } => write!(f, ", ROR #{:#X}", shift_imm),
            RotateRightWithExtend => write!(f, ", RRX"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strb() {
        let strb = decode_arm(0xe5c33208);
        assert_eq!(format!("{}", strb.disassemble(Condition::EQ)), "STREQB R3, [R3, #+0x208]");
    }

    #[test]
    fn test_ldrsd() {
        let instruction = decode_extra_arm(0xe17670f1);
        assert_eq!(format!("{}", instruction.disassemble(Condition::EQ)), "LDREQSH R7, [R6, #-0x1]!");
    }
}
