use std::fmt::Display;

use crate::{
    bitutil::{arithmetic_shift_right, get_bit, get_bits, rotate_right_with_extend},
    system::cpu::CPU,
};

use super::{Condition, DecodedInstruction};

struct LoadStore {
    cond: Condition,
    opcode: Opcode,
    b: bool,
    d: u8,
    adressing_mode: AddressingMode,
}

#[derive(Debug)]
enum Opcode {
    LDR,
    STR,
}

struct AddressingMode {
    u: bool,
    n: u8,
    mode: AddressingModeType,
    indexing_mode: IndexingMode,
}

enum AddressingModeType {
    Immediate { offset: u16 },
    ScaledRegister(ScaledRegister),
}

#[derive(Clone, Copy)]
struct ScaledRegister {
    m: u8,
    mode: ScaledRegisterMode,
}

#[derive(Clone, Copy)]
enum ScaledRegisterMode {
    Register,
    LogicalShiftLeft { shift_imm: u8 },
    LogicalShiftRight { shift_imm: u8 },
    ArithmeticShiftRight { shift_imm: u8 },
    RotateRight { shift_imm: u8 },
    RotateRightWithExtend,
}

enum IndexingMode {
    Offset,
    PreIndexed,
    PostIndexed { t: bool },
}

pub fn decode_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let d = get_bits(instruction, 12, 4) as u8;
    let b = get_bit(instruction, 22);

    Box::new(LoadStore {
        cond: Condition::decode_arm(instruction),
        opcode: Opcode::decode_arm(instruction),
        b,
        d,
        adressing_mode: AddressingMode::decode_arm(instruction),
    })
}

impl Opcode {
    fn decode_arm(instruction: u32) -> Opcode {
        use Opcode::*;

        let l = get_bit(instruction, 20);
        match l {
            true => LDR,
            false => STR,
        }
    }
}

impl DecodedInstruction for LoadStore {
    fn execute(&self, cpu: &mut CPU) {
        if self.d == 15 {
            todo!("d == 15");
        }

        if !self.cond.check(cpu) {
            return;
        }

        let address = self.adressing_mode.execute(cpu);

        match self.opcode {
            Opcode::LDR => {
                let data = match self.b {
                    false => cpu.mem.read_u32(address).rotate_right(8 * get_bits(address, 0, 2)),
                    true => cpu.mem.read_u8(address) as u32,
                };
                cpu.set_r(self.d, data);
            }
            Opcode::STR => match self.b {
                false => cpu.mem.write_u32(address, cpu.get_r(self.d)),
                true => cpu.mem.write_u8(address, cpu.get_r(self.d) as u8),
            },
        }
    }
}

impl AddressingMode {
    fn decode_arm(instruction: u32) -> AddressingMode {
        let u = get_bit(instruction, 23);
        let n = get_bits(instruction, 16, 4) as u8;

        AddressingMode {
            u,
            n,
            mode: {
                let is_immediate = get_bit(instruction, 25);
                match is_immediate {
                    true => AddressingModeType::Immediate {
                        offset: get_bits(instruction, 0, 12) as u16,
                    },
                    false => AddressingModeType::ScaledRegister(ScaledRegister::decode_arm(instruction)),
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
            AddressingModeType::Immediate { offset } => offset as u32,
            AddressingModeType::ScaledRegister(scaled_register) => scaled_register.calc_address(cpu),
        };

        let r_n = cpu.get_r(self.n);
        let r_n_offset = match self.u {
            false => r_n - offset,
            true => r_n + offset,
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

impl AddressingModeType {
    fn decode_arm(instruction: u32) -> AddressingModeType {
        let is_imm = get_bit(instruction, 25);
        if is_imm {
            AddressingModeType::Immediate {
                offset: get_bits(instruction, 0, 12) as u16,
            }
        } else {
            AddressingModeType::ScaledRegister(ScaledRegister::decode_arm(instruction))
        }
    }
}

impl ScaledRegister {
    fn decode_arm(instruction: u32) -> ScaledRegister {
        ScaledRegister {
            m: get_bits(instruction, 0, 4) as u8,
            mode: {
                use ScaledRegisterMode::*;
                let shift = get_bits(instruction, 5, 2) as u8;
                let shift_imm = get_bits(instruction, 7, 5) as u8;
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

impl Display for LoadStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let t = match self.adressing_mode.indexing_mode {
            IndexingMode::PostIndexed { t } => t,
            _ => false,
        };

        write!(
            f,
            "{:?}{}{}{} R{}, {}",
            self.opcode,
            self.cond,
            if self.b { "B" } else { "" },
            if t { "T" } else { "" },
            self.d,
            self.adressing_mode
        )
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.u { "+" } else { "-" };
        let rhs = match &self.mode {
            AddressingModeType::Immediate { offset } => format!("#{}{:#X}", sign, offset),
            AddressingModeType::ScaledRegister(scaled_register) => format!("{}{}", sign, scaled_register),
        };

        let n = self.n;
        match self.indexing_mode {
            IndexingMode::Offset => write!(f, "[R{}, {}]", n, rhs),
            IndexingMode::PreIndexed => write!(f, "[R{}], {}]!", n, rhs),
            IndexingMode::PostIndexed { .. } => write!(f, "[R{}], {}", rhs, n),
        }
    }
}

impl Display for ScaledRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ScaledRegisterMode::*;

        write!(f, "R{}, ", self.m);

        match self.mode {
            Register => Ok(()),
            LogicalShiftLeft { shift_imm } => write!(f, "LSL #{}", shift_imm),
            LogicalShiftRight { shift_imm } => write!(f, "LSR #{}", shift_imm),
            ArithmeticShiftRight { shift_imm } => write!(f, "ASR #{}", shift_imm),
            RotateRight { shift_imm } => write!(f, "ROR #{}", shift_imm),
            RotateRightWithExtend => write!(f, "RRX"),
        }
    }
}
