use std::fmt::Display;

use crate::{
    bits::Bits,
    system::{
        cpu::{Mode, Register, CPU},
        memory::{Access, Memory},
    },
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadStoreMultiple {
    pub opcode: Opcode,
    pub n: Register,
    pub writeback: bool,
    pub s: bool,
    pub registers: RegisterList,
    pub addressing: AddressingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    LDM,
    STM,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressingMode {
    DecrementAfter,
    IncrementAfter,
    DecrementBefore,
    IncrementBefore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegisterList(u16);

impl RegisterList {
    pub const LOW: RegisterList = RegisterList(0xFF);

    pub const fn from_bits(bits: u32) -> RegisterList {
        RegisterList(bits as u16)
    }

    pub const fn bits(self) -> u32 {
        self.0 as u32
    }

    pub fn with(self, register: Register) -> RegisterList {
        RegisterList(self.0.with_bit(register.number(), true))
    }

    pub fn contains(self, register: Register) -> bool {
        self.0.bit(register.number())
    }

    pub fn is_subset_of(self, other: RegisterList) -> bool {
        self.0 & !other.0 == 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub fn iter(self) -> impl Iterator<Item = Register> {
        Register::all().filter(move |register| self.contains(*register))
    }
}

impl FromIterator<Register> for RegisterList {
    fn from_iter<I: IntoIterator<Item = Register>>(registers: I) -> RegisterList {
        registers.into_iter().fold(RegisterList::default(), RegisterList::with)
    }
}

impl Display for RegisterList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{{}}}", self.iter().map(|register| register.to_string()).collect::<Vec<_>>().join(", "))
    }
}

#[inline(always)]
pub fn decode_arm(word: u32) -> Instruction {
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: Opcode::from_load_bit(word.bit(20)),
        n: Register::from(word.bits(16..20)),
        writeback: word.bit(21),
        s: word.bit(22),
        registers: RegisterList::from_bits(word.bits(0..16)),
        addressing: AddressingMode::from_bits(word.bits(23..25)),
    })
}

#[inline(always)]
pub fn decode_push_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let registers = RegisterList::from_bits(word.bits(0..8));
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: Opcode::STM,
        n: Register::SP,
        writeback: true,
        s: false,
        registers: if word.bit(8) { registers.with(Register::LR) } else { registers },
        addressing: AddressingMode::DecrementBefore,
    })
}

#[inline(always)]
pub fn decode_pop_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let registers = RegisterList::from_bits(word.bits(0..8));
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: Opcode::LDM,
        n: Register::SP,
        writeback: true,
        s: false,
        registers: if word.bit(8) { registers.with(Register::PC) } else { registers },
        addressing: AddressingMode::IncrementAfter,
    })
}

#[inline(always)]
pub fn decode_ldm_stm_thumb(word: u16) -> Instruction {
    let word = u32::from(word);
    let opcode = Opcode::from_load_bit(word.bit(11));
    let n = Register::from(word.bits(8..11));
    let registers = RegisterList::from_bits(word.bits(0..8));
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode,
        n,
        writeback: !(opcode == Opcode::LDM && registers.contains(n)),
        s: false,
        registers,
        addressing: AddressingMode::IncrementAfter,
    })
}

impl Opcode {
    #[inline(always)]
    fn from_load_bit(load: bool) -> Opcode {
        if load {
            Opcode::LDM
        } else {
            Opcode::STM
        }
    }

    fn load_bit(self) -> u32 {
        u32::from(self == Opcode::LDM)
    }
}

impl LoadStoreMultiple {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let (registers, size) = if self.registers.is_empty() {
            (RegisterList::default().with(Register::PC), 16 * 4)
        } else {
            (self.registers, self.registers.len() * 4)
        };
        let r_n = cpu.r(self.n);
        let start_address = match self.addressing {
            AddressingMode::DecrementAfter => r_n.wrapping_sub(size).wrapping_add(4),
            AddressingMode::IncrementAfter => r_n,
            AddressingMode::DecrementBefore => r_n.wrapping_sub(size),
            AddressingMode::IncrementBefore => r_n.wrapping_add(4),
        };
        let new_base = match self.addressing {
            AddressingMode::IncrementAfter | AddressingMode::IncrementBefore => r_n.wrapping_add(size),
            AddressingMode::DecrementAfter | AddressingMode::DecrementBefore => r_n.wrapping_sub(size),
        };

        let pc_in_list = registers.contains(Register::PC);
        let user_bank = self.s && !(self.opcode == Opcode::LDM && pc_in_list);
        let mut address = start_address;
        let mut access = Access::Nonsequential;
        match self.opcode {
            Opcode::LDM => {
                if self.writeback {
                    cpu.set_r(self.n, new_base);
                }
                for register in registers.iter().filter(|register| *register != Register::PC) {
                    let value = mem.load_u32(address, access);
                    if user_bank {
                        cpu.set_r_in_mode(register, Mode::User, value);
                    } else {
                        cpu.set_r(register, value);
                    }
                    address = address.wrapping_add(4);
                    access = Access::Sequential;
                }
                if pc_in_list {
                    let value = mem.load_u32(address, access);
                    if self.s && cpu.has_spsr() {
                        cpu.set_cpsr(cpu.spsr());
                    }
                    cpu.set_pc(value);
                }
                mem.idle(1);
            }
            Opcode::STM => {
                let first = registers.iter().next();
                for register in registers.iter() {
                    let value = if register == self.n && Some(register) != first && self.writeback {
                        new_base
                    } else if user_bank {
                        cpu.r_in_mode(register, Mode::User)
                    } else if register == Register::PC {
                        cpu.r(Register::PC).wrapping_add(cpu.instruction_length())
                    } else {
                        cpu.r(register)
                    };
                    mem.store_u32(address, value, access);
                    address = address.wrapping_add(4);
                    access = Access::Sequential;
                }
                if self.writeback {
                    cpu.set_r(self.n, new_base);
                }
            }
        }
    }

    pub fn encode_arm(self) -> u32 {
        0b100 << 25 | self.addressing.bits() << 23 | u32::from(self.s) << 22 | u32::from(self.writeback) << 21 | self.opcode.load_bit() << 20 | self.n.number() << 16 | self.registers.bits()
    }

    pub fn encode_thumb(self) -> Option<u16> {
        if self.s {
            return None;
        }
        let low = self.registers.is_subset_of(RegisterList::LOW);
        let word = match (self.opcode, self.n, self.addressing) {
            (Opcode::STM, Register::SP, AddressingMode::DecrementBefore) if self.writeback && self.registers.is_subset_of(RegisterList::LOW.with(Register::LR)) => {
                0b101_1010 << 9 | u32::from(self.registers.contains(Register::LR)) << 8 | self.registers.bits().bits(0..8)
            }
            (Opcode::LDM, Register::SP, AddressingMode::IncrementAfter) if self.writeback && self.registers.is_subset_of(RegisterList::LOW.with(Register::PC)) => {
                0b101_1110 << 9 | u32::from(self.registers.contains(Register::PC)) << 8 | self.registers.bits().bits(0..8)
            }
            (opcode, n, AddressingMode::IncrementAfter) if n.is_low() && low && self.writeback == !(opcode == Opcode::LDM && self.registers.contains(n)) => {
                0b1100 << 12 | opcode.load_bit() << 11 | n.number() << 8 | self.registers.bits()
            }
            _ => return None,
        };
        u16::try_from(word).ok()
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!(
            "{:?}{}{} {}{}, {}{}",
            self.opcode,
            cond,
            self.addressing,
            self.n,
            if self.writeback { "!" } else { "" },
            self.registers,
            if self.s { "^" } else { "" }
        )
    }
}

impl AddressingMode {
    const ALL: [AddressingMode; 4] = [
        AddressingMode::DecrementAfter,
        AddressingMode::IncrementAfter,
        AddressingMode::DecrementBefore,
        AddressingMode::IncrementBefore,
    ];

    #[inline(always)]
    fn from_bits(bits: u32) -> AddressingMode {
        AddressingMode::ALL[bits as usize]
    }

    fn bits(self) -> u32 {
        self as u32
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AddressingMode::DecrementAfter => "DA",
            AddressingMode::IncrementAfter => "IA",
            AddressingMode::DecrementBefore => "DB",
            AddressingMode::IncrementBefore => "IB",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pop_pc() {
        assert_eq!(Instruction::decode_thumb(0xBD10).disassemble(Condition::AL, 0), "LDMIA R13!, {R4, R15}");
    }

    #[test]
    fn test_stmfd_with_user_bank() {
        assert_eq!(Instruction::decode_arm(0xE96D_4003).disassemble(Condition::AL, 0), "STMDB R13!, {R0, R1, R14}^");
    }

    #[test]
    fn test_register_lists() {
        let list: RegisterList = [Register::R1, Register::LR].into_iter().collect();
        assert_eq!(list.bits(), 0x4002);
        assert_eq!(list.len(), 2);
        assert!(list.contains(Register::LR) && !list.contains(Register::R0));
        assert!(!list.is_subset_of(RegisterList::LOW));
        assert_eq!(list.to_string(), "{R1, R14}");
    }

    #[test]
    fn test_encoding_matches_known_words() {
        assert_eq!(Instruction::decode_arm(0xE96D_4003).encode_arm(Condition::AL), Some(0xE96D_4003));
        for word in [0xBD10, 0xB510, 0xC9A0, 0xC1A0] {
            assert_eq!(Instruction::decode_thumb(word).encode_thumb(), Some(word), "{:04X}", word);
        }
    }
}
