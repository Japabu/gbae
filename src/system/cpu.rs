use std::fmt::Display;
use std::ops::Range;

use crate::bits::Bits;

use super::{
    instructions::{
        condition_passed, format_instruction_arm, format_instruction_thumb,
        lut::{index_arm, index_thumb, ARM_LUT, THUMB_LUT},
    },
    memory::Memory,
    state::{Reader, StateError, Writer},
};

pub const CPU_FREQUENCY: u64 = 16_777_216;
pub const INSTRUCTION_LEN_ARM: u32 = 4;
pub const INSTRUCTION_LEN_THUMB: u32 = 2;

const IRQ_VECTOR: u32 = 0x18;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Register(u8);

impl Register {
    pub const R0: Register = Register(0);
    pub const R1: Register = Register(1);
    pub const R2: Register = Register(2);
    pub const R3: Register = Register(3);
    pub const R4: Register = Register(4);
    pub const R5: Register = Register(5);
    pub const R6: Register = Register(6);
    pub const R7: Register = Register(7);
    pub const R8: Register = Register(8);
    pub const R9: Register = Register(9);
    pub const R10: Register = Register(10);
    pub const R11: Register = Register(11);
    pub const R12: Register = Register(12);
    pub const SP: Register = Register(13);
    pub const LR: Register = Register(14);
    pub const PC: Register = Register(15);

    pub fn all() -> impl Iterator<Item = Register> {
        (0..16).map(Register)
    }

    pub const fn number(self) -> u32 {
        self.0 as u32
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn is_low(self) -> bool {
        self.0 < 8
    }
}

impl From<u32> for Register {
    fn from(bits: u32) -> Register {
        Register(bits.bits(0..4) as u8)
    }
}

impl Display for Register {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "R{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    User,
    Fiq,
    Irq,
    Supervisor,
    Abort,
    Undefined,
    System,
}

impl Mode {
    const ALL: [Mode; 7] = [Mode::User, Mode::Fiq, Mode::Irq, Mode::Supervisor, Mode::Abort, Mode::Undefined, Mode::System];

    pub const fn bits(self) -> u32 {
        match self {
            Mode::User => 0b10000,
            Mode::Fiq => 0b10001,
            Mode::Irq => 0b10010,
            Mode::Supervisor => 0b10011,
            Mode::Abort => 0b10111,
            Mode::Undefined => 0b11011,
            Mode::System => 0b11111,
        }
    }

    pub fn from_bits(bits: u32) -> Option<Mode> {
        Mode::ALL.into_iter().find(|mode| mode.bits() == bits)
    }

    pub fn is_privileged(self) -> bool {
        self != Mode::User
    }

    fn bank(self) -> Bank {
        match self {
            Mode::User | Mode::System => Bank::User,
            Mode::Fiq => Bank::Fiq,
            Mode::Irq => Bank::Irq,
            Mode::Supervisor => Bank::Supervisor,
            Mode::Abort => Bank::Abort,
            Mode::Undefined => Bank::Undefined,
        }
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Mode::User => "USR",
            Mode::Fiq => "FIQ",
            Mode::Irq => "IRQ",
            Mode::Supervisor => "SVC",
            Mode::Abort => "ABT",
            Mode::Undefined => "UND",
            Mode::System => "SYS",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bank {
    User,
    Fiq,
    Irq,
    Supervisor,
    Abort,
    Undefined,
}

impl Bank {
    const ALL: [Bank; 6] = [Bank::User, Bank::Fiq, Bank::Irq, Bank::Supervisor, Bank::Abort, Bank::Undefined];

    fn low_bank(self) -> Bank {
        if self == Bank::Fiq {
            Bank::Fiq
        } else {
            Bank::User
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Psr(u32);

impl Psr {
    const NEGATIVE: u32 = 31;
    const ZERO: u32 = 30;
    const CARRY: u32 = 29;
    const OVERFLOW: u32 = 28;
    const FLAGS: Range<u32> = 28..32;
    const IRQ_DISABLE: u32 = 7;
    const FIQ_DISABLE: u32 = 6;
    const THUMB: u32 = 5;
    const MODE: Range<u32> = 0..5;

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub fn negative(self) -> bool {
        self.0.bit(Self::NEGATIVE)
    }

    pub fn zero(self) -> bool {
        self.0.bit(Self::ZERO)
    }

    pub fn carry(self) -> bool {
        self.0.bit(Self::CARRY)
    }

    pub fn overflow(self) -> bool {
        self.0.bit(Self::OVERFLOW)
    }

    pub fn flags(self) -> u32 {
        self.0.bits(Self::FLAGS)
    }

    pub fn irq_disabled(self) -> bool {
        self.0.bit(Self::IRQ_DISABLE)
    }

    pub fn fiq_disabled(self) -> bool {
        self.0.bit(Self::FIQ_DISABLE)
    }

    pub fn thumb(self) -> bool {
        self.0.bit(Self::THUMB)
    }

    pub fn mode(self) -> Mode {
        let bits = self.0.bits(Self::MODE);
        Mode::from_bits(bits).unwrap_or_else(|| panic!("Invalid processor mode {:#07b}", bits))
    }

    pub fn with_flags(self, negative: bool, zero: bool, carry: bool, overflow: bool) -> Psr {
        let flags = u32::from(negative) << 3 | u32::from(zero) << 2 | u32::from(carry) << 1 | u32::from(overflow);
        Psr(self.0.with_bits(Self::FLAGS, flags))
    }

    pub fn with_nz(self, result: u32) -> Psr {
        self.with_flags(result.bit(31), result == 0, self.carry(), self.overflow())
    }

    pub fn with_nzc(self, result: u32, carry: bool) -> Psr {
        self.with_flags(result.bit(31), result == 0, carry, self.overflow())
    }

    pub fn with_nzcv(self, result: u32, carry: bool, overflow: bool) -> Psr {
        self.with_flags(result.bit(31), result == 0, carry, overflow)
    }

    pub fn with_irq_disabled(self, disabled: bool) -> Psr {
        Psr(self.0.with_bit(Self::IRQ_DISABLE, disabled))
    }

    pub fn with_fiq_disabled(self, disabled: bool) -> Psr {
        Psr(self.0.with_bit(Self::FIQ_DISABLE, disabled))
    }

    pub fn with_thumb(self, thumb: bool) -> Psr {
        Psr(self.0.with_bit(Self::THUMB, thumb))
    }

    pub fn with_mode(self, mode: Mode) -> Psr {
        Psr(self.0.with_bits(Self::MODE, mode.bits()))
    }
}

impl From<u32> for Psr {
    fn from(bits: u32) -> Psr {
        Psr(bits)
    }
}

impl Display for Psr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let flag = |set: bool, letter: char| if set { letter } else { '-' };
        write!(
            f,
            "{:08X} [{}{}{}{}{}{}{}] {}",
            self.0,
            flag(self.negative(), 'N'),
            flag(self.zero(), 'Z'),
            flag(self.carry(), 'C'),
            flag(self.overflow(), 'V'),
            flag(self.irq_disabled(), 'I'),
            flag(self.fiq_disabled(), 'F'),
            flag(self.thumb(), 'T'),
            self.mode()
        )
    }
}

pub struct CPU {
    cpsr: Psr,
    r: [u32; 16],
    banked: [[u32; 7]; Bank::ALL.len()],
    spsr: [Psr; Bank::ALL.len()],
    bank: Bank,
    pipeline: [u32; 2],
    branch_happened: bool,
    cycles: u64,
}

impl CPU {
    pub fn new() -> CPU {
        CPU {
            cpsr: Psr(0).with_mode(Mode::Supervisor).with_irq_disabled(true).with_fiq_disabled(true),
            r: [0; 16],
            banked: [[0; 7]; Bank::ALL.len()],
            spsr: [Psr(0); Bank::ALL.len()],
            bank: Bank::Supervisor,
            pipeline: [0; 2],
            branch_happened: false,
            cycles: 0,
        }
    }

    pub fn reset(&mut self, mem: &mut Memory) {
        self.set_cpsr(Psr(0).with_mode(Mode::Supervisor).with_irq_disabled(true).with_fiq_disabled(true));
        self.r[Register::PC.index()] = 0;
        self.flush_pipeline(mem);
    }

    pub fn flush_pipeline(&mut self, mem: &mut Memory) {
        let pc = self.r[Register::PC.index()];
        mem.invalidate_fetch_sequence();
        if self.thumb() {
            self.pipeline = [u32::from(mem.fetch_u16(pc)), u32::from(mem.fetch_u16(pc.wrapping_add(INSTRUCTION_LEN_THUMB)))];
            self.r[Register::PC.index()] = pc.wrapping_add(INSTRUCTION_LEN_THUMB * 2);
        } else {
            self.pipeline = [mem.fetch_u32(pc), mem.fetch_u32(pc.wrapping_add(INSTRUCTION_LEN_ARM))];
            self.r[Register::PC.index()] = pc.wrapping_add(INSTRUCTION_LEN_ARM * 2);
        }
        self.branch_happened = false;
    }

    #[inline(always)]
    pub fn pc(&self) -> u32 {
        self.r[Register::PC.index()].wrapping_sub(self.instruction_length() * 2)
    }

    #[inline(always)]
    pub fn next_pc(&self) -> u32 {
        self.r[Register::PC.index()].wrapping_sub(self.instruction_length())
    }

    #[inline(always)]
    pub fn r(&self, register: Register) -> u32 {
        self.r[register.index()]
    }

    #[inline(always)]
    pub fn set_r(&mut self, register: Register, value: u32) {
        self.r[register.index()] = value;
        if register == Register::PC {
            self.branch_happened = true;
        }
    }

    #[inline(always)]
    pub fn set_pc(&mut self, value: u32) {
        let alignment = if self.thumb() { 0b1 } else { 0b11 };
        self.r[Register::PC.index()] = value & !alignment;
        self.branch_happened = true;
    }

    fn banked_slot(&self, register: Register, bank: Bank) -> Option<(Bank, usize)> {
        let slot = register.index().checked_sub(8)?;
        if bank == self.bank || register == Register::PC {
            None
        } else if register >= Register::SP {
            Some((bank, slot))
        } else if bank == Bank::Fiq {
            Some((Bank::Fiq, slot))
        } else if self.bank == Bank::Fiq {
            Some((Bank::User, slot))
        } else {
            None
        }
    }

    pub fn r_in_mode(&self, register: Register, mode: Mode) -> u32 {
        match self.banked_slot(register, mode.bank()) {
            Some((bank, slot)) => self.banked[bank as usize][slot],
            None => self.r(register),
        }
    }

    pub fn set_r_in_mode(&mut self, register: Register, mode: Mode, value: u32) {
        match self.banked_slot(register, mode.bank()) {
            Some((bank, slot)) => self.banked[bank as usize][slot] = value,
            None => self.set_r(register, value),
        }
    }

    fn switch_bank(&mut self, new_bank: Bank) {
        let old_bank = self.bank;
        if old_bank != new_bank {
            self.banked[old_bank.low_bank() as usize][0..5].copy_from_slice(&self.r[8..13]);
            self.banked[old_bank as usize][5..7].copy_from_slice(&self.r[13..15]);
            self.r[8..13].copy_from_slice(&self.banked[new_bank.low_bank() as usize][0..5]);
            self.r[13..15].copy_from_slice(&self.banked[new_bank as usize][5..7]);
            self.bank = new_bank;
        }
    }

    #[inline(always)]
    pub fn cpsr(&self) -> Psr {
        self.cpsr
    }

    pub fn set_cpsr(&mut self, value: Psr) {
        self.switch_bank(value.mode().bank());
        self.cpsr = value;
    }

    #[inline(always)]
    pub fn spsr(&self) -> Psr {
        self.spsr[self.bank as usize]
    }

    #[inline(always)]
    pub fn set_spsr(&mut self, value: Psr) {
        self.spsr[self.bank as usize] = value;
    }

    #[inline(always)]
    pub fn has_spsr(&self) -> bool {
        self.bank != Bank::User
    }

    #[inline(always)]
    pub fn mode(&self) -> Mode {
        self.cpsr.mode()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.set_cpsr(self.cpsr.with_mode(mode));
    }

    #[inline(always)]
    pub fn thumb(&self) -> bool {
        self.cpsr.thumb()
    }

    #[inline(always)]
    pub fn set_thumb(&mut self, thumb: bool) {
        self.cpsr = self.cpsr.with_thumb(thumb);
    }

    #[inline(always)]
    pub fn set_irq_disabled(&mut self, disabled: bool) {
        self.cpsr = self.cpsr.with_irq_disabled(disabled);
    }

    #[inline(always)]
    pub fn set_nz(&mut self, result: u32) {
        self.cpsr = self.cpsr.with_nz(result);
    }

    #[inline(always)]
    pub fn set_negative_zero(&mut self, negative: bool, zero: bool) {
        self.cpsr = self.cpsr.with_flags(negative, zero, self.cpsr.carry(), self.cpsr.overflow());
    }

    #[inline(always)]
    pub fn set_nzc(&mut self, result: u32, carry: bool) {
        self.cpsr = self.cpsr.with_nzc(result, carry);
    }

    #[inline(always)]
    pub fn set_nzcv(&mut self, result: u32, carry: bool, overflow: bool) {
        self.cpsr = self.cpsr.with_nzcv(result, carry, overflow);
    }

    #[inline(always)]
    pub fn cycle(&mut self, mem: &mut Memory) {
        let pc = self.r[Register::PC.index()];
        let instruction = self.pipeline[0];
        self.pipeline[0] = self.pipeline[1];
        self.branch_happened = false;

        if self.thumb() {
            self.pipeline[1] = u32::from(mem.fetch_u16(pc));
            let instruction = instruction as u16;
            THUMB_LUT[index_thumb(instruction)](self, mem, instruction);
        } else {
            self.pipeline[1] = mem.fetch_u32(pc);
            if condition_passed(instruction.bits(28..), self.cpsr) {
                ARM_LUT[index_arm(instruction)](self, mem, instruction);
            }
        }

        if self.branch_happened {
            self.flush_pipeline(mem);
        } else {
            self.r[Register::PC.index()] = pc.wrapping_add(self.instruction_length());
        }

        self.cycles += u64::from(mem.take_cycles());
    }

    #[inline(always)]
    pub fn instruction_length(&self) -> u32 {
        if self.thumb() {
            INSTRUCTION_LEN_THUMB
        } else {
            INSTRUCTION_LEN_ARM
        }
    }

    #[inline(always)]
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    #[inline(always)]
    pub fn add_cycles(&mut self, cycles: u64) {
        self.cycles += cycles;
    }

    #[inline(always)]
    pub fn handle_interrupts(&mut self, mem: &mut Memory) {
        let io = mem.get_io_registers();
        if io.ime && !self.cpsr.irq_disabled() && io.ie & io.irf != 0 {
            self.take_exception(Mode::Irq, IRQ_VECTOR, self.pc().wrapping_add(4));
            self.flush_pipeline(mem);
        }
    }

    pub fn take_exception(&mut self, mode: Mode, vector: u32, return_address: u32) {
        let old_cpsr = self.cpsr;
        self.set_mode(mode);
        self.set_spsr(old_cpsr);
        self.set_r(Register::LR, return_address);
        self.cpsr = self.cpsr.with_irq_disabled(true).with_thumb(false);
        self.set_r(Register::PC, vector);
    }

    pub fn save_state(&self, writer: &mut Writer) {
        writer.u32(self.cpsr.bits());
        writer.u32s(&self.r);
        for bank in &self.banked {
            writer.u32s(bank);
        }
        for spsr in &self.spsr {
            writer.u32(spsr.bits());
        }
        writer.u8(self.bank as u8);
        writer.u32s(&self.pipeline);
        writer.bool(self.branch_happened);
        writer.u64(self.cycles);
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.cpsr = Psr(reader.u32()?);
        reader.u32s(&mut self.r)?;
        for bank in &mut self.banked {
            reader.u32s(bank)?;
        }
        for spsr in &mut self.spsr {
            *spsr = Psr(reader.u32()?);
        }
        self.bank = *Bank::ALL.get(usize::from(reader.u8()?)).ok_or(StateError::Corrupt)?;
        reader.u32s(&mut self.pipeline)?;
        self.branch_happened = reader.bool()?;
        self.cycles = reader.u64()?;
        Ok(())
    }

    pub fn print_registers(&self) {
        for row in Register::all().collect::<Vec<_>>().chunks(4) {
            println!(
                "{}",
                row.iter()
                    .map(|register| format!("{:>3}: {:08X}", register.to_string(), self.r(*register)))
                    .collect::<Vec<_>>()
                    .join("   ")
            );
        }
    }

    pub fn print_status(&self) {
        println!("CPSR: {}", self.cpsr);
    }

    pub fn print_next_instruction(&self) {
        let pc = self.pc();
        if self.thumb() {
            println!(
                "Next thumb instruction at {:08X}: {}",
                pc,
                format_instruction_thumb(self.pipeline[0] as u16, self.pipeline[1] as u16, pc)
            );
        } else {
            println!("Next arm instruction at {:08X}: {}", pc, format_instruction_arm(self.pipeline[0], pc));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bank_switch_keeps_registers_per_mode() {
        let mut cpu = CPU::new();
        cpu.set_r(Register::SP, 0x1000);
        cpu.set_r(Register::R8, 0x88);
        cpu.set_mode(Mode::Irq);
        assert_eq!(cpu.r(Register::R8), 0x88);
        cpu.set_r(Register::SP, 0x2000);
        cpu.set_mode(Mode::Fiq);
        cpu.set_r(Register::R8, 0xF8);
        cpu.set_r(Register::SP, 0x3000);
        assert_eq!(cpu.r_in_mode(Register::R8, Mode::User), 0x88);
        assert_eq!(cpu.r_in_mode(Register::SP, Mode::Irq), 0x2000);
        assert_eq!(cpu.r_in_mode(Register::SP, Mode::Supervisor), 0x1000);
        cpu.set_mode(Mode::Supervisor);
        assert_eq!(cpu.r(Register::R8), 0x88);
        assert_eq!(cpu.r(Register::SP), 0x1000);
        assert_eq!(cpu.r_in_mode(Register::R8, Mode::Fiq), 0xF8);
        assert_eq!(cpu.r_in_mode(Register::SP, Mode::Fiq), 0x3000);
    }

    #[test]
    fn test_set_r_in_user_mode_from_privileged_mode() {
        let mut cpu = CPU::new();
        cpu.set_mode(Mode::User);
        cpu.set_r(Register::SP, 0x1234);
        cpu.set_mode(Mode::Irq);
        cpu.set_r_in_mode(Register::SP, Mode::User, 0x5678);
        cpu.set_r_in_mode(Register::R8, Mode::User, 0x9);
        assert_eq!(cpu.r(Register::R8), 0x9);
        cpu.set_mode(Mode::System);
        assert_eq!(cpu.r(Register::SP), 0x5678);
    }

    #[test]
    fn test_status_register_fields() {
        let psr = Psr(0).with_mode(Mode::Irq).with_thumb(true).with_nzcv(0x8000_0000, true, false);
        assert_eq!(psr.mode(), Mode::Irq);
        assert!(psr.thumb() && psr.negative() && psr.carry() && !psr.zero() && !psr.overflow());
        assert_eq!(psr.flags(), 0b1010);
        assert_eq!(psr.with_nz(0).flags(), 0b0110);
        assert_eq!(psr.to_string(), "A0000032 [N-C---T] IRQ");
    }
}
