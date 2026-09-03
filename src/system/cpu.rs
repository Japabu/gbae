use std::time::Duration;

use crate::system::instructions::{format_instruction_arm, format_instruction_thumb};

use super::{
    instructions::{
        condition_passed,
        lut::{index_arm, index_thumb, ARM_LUT, THUMB_LUT},
    },
    memory::Memory,
    state::{Reader, StateError, Writer},
};

pub const MODE_USR: u8 = 0b10000;
pub const MODE_FIQ: u8 = 0b10001;
pub const MODE_IRQ: u8 = 0b10010;
pub const MODE_SVC: u8 = 0b10011;
pub const MODE_ABT: u8 = 0b10111;
pub const MODE_UND: u8 = 0b11011;
pub const MODE_SYS: u8 = 0b11111;

pub const REGISTER_SP: u8 = 13;
pub const REGISTER_LR: u8 = 14;
pub const REGISTER_PC: u8 = 15;

pub const INSTRUCTION_LEN_ARM: u32 = 4;
pub const INSTRUCTION_LEN_THUMB: u32 = 2;

pub const CPU_FREQUENCY: u64 = 16_777_216;
pub const INSTRUCTION_TIME: Duration = Duration::from_nanos(1_000_000_000 / CPU_FREQUENCY);

const BANK_USR: usize = 0;
const BANK_FIQ: usize = 1;
const BANK_IRQ: usize = 2;
const BANK_SVC: usize = 3;
const BANK_ABT: usize = 4;
const BANK_UND: usize = 5;
const BANK_COUNT: usize = 6;

const FLAG_N: u32 = 1 << 31;
const FLAG_Z: u32 = 1 << 30;
const FLAG_C: u32 = 1 << 29;
const FLAG_V: u32 = 1 << 28;
const FLAG_I: u32 = 1 << 7;
const FLAG_F: u32 = 1 << 6;
const FLAG_T: u32 = 1 << 5;
const MODE_MASK: u32 = 0x1F;

pub fn format_mode(mode: u8) -> &'static str {
    match mode {
        MODE_USR => "USR",
        MODE_FIQ => "FIQ",
        MODE_IRQ => "IRQ",
        MODE_SVC => "SVC",
        MODE_ABT => "ABT",
        MODE_UND => "UND",
        MODE_SYS => "SYS",
        _ => panic!("Invalid mode"),
    }
}

fn bank_of(mode: u8) -> usize {
    match mode {
        MODE_USR | MODE_SYS => BANK_USR,
        MODE_FIQ => BANK_FIQ,
        MODE_IRQ => BANK_IRQ,
        MODE_SVC => BANK_SVC,
        MODE_ABT => BANK_ABT,
        MODE_UND => BANK_UND,
        _ => panic!("Invalid mode {:#04X}", mode),
    }
}

pub struct CPU {
    cpsr: u32,
    r: [u32; 16],
    banked: [[u32; 7]; BANK_COUNT],
    spsr: [u32; BANK_COUNT],
    bank: usize,
    pipeline: [u32; 2],
    branch_happened: bool,
    cycles: u64,
}

impl CPU {
    pub fn new() -> Self {
        let mut cpu = CPU {
            cpsr: MODE_SVC as u32,
            r: [0; 16],
            banked: [[0; 7]; BANK_COUNT],
            spsr: [0; BANK_COUNT],
            bank: BANK_SVC,
            pipeline: [0; 2],
            branch_happened: false,
            cycles: 0,
        };
        cpu.set_cpsr(MODE_SVC as u32 | FLAG_I | FLAG_F);
        cpu
    }

    pub fn reset(&mut self, mem: &mut Memory) {
        self.set_cpsr(MODE_SVC as u32 | FLAG_I | FLAG_F);
        self.r[REGISTER_PC as usize] = 0x00000000;
        self.flush_pipeline(mem);
    }

    pub fn flush_pipeline(&mut self, mem: &mut Memory) {
        let pc = self.r[REGISTER_PC as usize];
        mem.invalidate_fetch_sequence();
        if self.get_thumb_state() {
            self.pipeline = [mem.fetch_u16(pc) as u32, mem.fetch_u16(pc.wrapping_add(INSTRUCTION_LEN_THUMB)) as u32];
            self.r[REGISTER_PC as usize] = pc.wrapping_add(INSTRUCTION_LEN_THUMB * 2);
        } else {
            self.pipeline = [mem.fetch_u32(pc), mem.fetch_u32(pc.wrapping_add(INSTRUCTION_LEN_ARM))];
            self.r[REGISTER_PC as usize] = pc.wrapping_add(INSTRUCTION_LEN_ARM * 2);
        }
        self.branch_happened = false;
    }

    #[inline(always)]
    pub fn pc(&self) -> u32 {
        self.curr_instruction_address_from_execution_stage()
    }

    #[inline(always)]
    pub fn get_r(&self, r: u8) -> u32 {
        self.r[r as usize]
    }

    #[inline(always)]
    pub fn set_r(&mut self, r: u8, value: u32) {
        self.r[r as usize] = value;
        if r == REGISTER_PC {
            self.branch_happened = true;
        }
    }

    #[inline(always)]
    pub fn set_pc(&mut self, value: u32) {
        let mask = if self.get_thumb_state() { !0b1 } else { !0b11 };
        self.r[REGISTER_PC as usize] = value & mask;
        self.branch_happened = true;
    }

    fn bank_slot(&self, r: u8, bank: usize) -> Option<(usize, usize)> {
        if bank == self.bank || r < 8 || r == REGISTER_PC {
            None
        } else if r >= REGISTER_SP {
            Some((bank, r as usize - 8))
        } else if bank == BANK_FIQ {
            Some((BANK_FIQ, r as usize - 8))
        } else if self.bank == BANK_FIQ {
            Some((BANK_USR, r as usize - 8))
        } else {
            None
        }
    }

    pub fn get_r_in_mode(&self, r: u8, mode: u8) -> u32 {
        match self.bank_slot(r, bank_of(mode)) {
            Some((bank, slot)) => self.banked[bank][slot],
            None => self.r[r as usize],
        }
    }

    pub fn set_r_in_mode(&mut self, r: u8, mode: u8, value: u32) {
        match self.bank_slot(r, bank_of(mode)) {
            Some((bank, slot)) => self.banked[bank][slot] = value,
            None => self.set_r(r, value),
        }
    }

    fn switch_bank(&mut self, new_bank: usize) {
        let old_bank = self.bank;
        if old_bank != new_bank {
            let old_low_bank = if old_bank == BANK_FIQ { BANK_FIQ } else { BANK_USR };
            let new_low_bank = if new_bank == BANK_FIQ { BANK_FIQ } else { BANK_USR };
            self.banked[old_low_bank][0..5].copy_from_slice(&self.r[8..13]);
            self.banked[old_bank][5..7].copy_from_slice(&self.r[13..15]);
            self.r[8..13].copy_from_slice(&self.banked[new_low_bank][0..5]);
            self.r[13..15].copy_from_slice(&self.banked[new_bank][5..7]);
            self.bank = new_bank;
        }
    }

    #[inline(always)]
    pub fn get_cpsr(&self) -> u32 {
        self.cpsr
    }

    pub fn set_cpsr(&mut self, value: u32) {
        self.switch_bank(bank_of((value & MODE_MASK) as u8));
        self.cpsr = value;
    }

    #[inline(always)]
    pub fn get_spsr(&self) -> u32 {
        self.spsr[self.bank]
    }

    #[inline(always)]
    pub fn set_spsr(&mut self, value: u32) {
        self.spsr[self.bank] = value;
    }

    #[inline(always)]
    pub fn cycle(&mut self, mem: &mut Memory) {
        let pc = self.r[REGISTER_PC as usize];
        let instruction = self.pipeline[0];
        self.pipeline[0] = self.pipeline[1];
        self.branch_happened = false;

        if self.get_thumb_state() {
            self.pipeline[1] = mem.fetch_u16(pc) as u32;
            THUMB_LUT[index_thumb(instruction as u16)](self, mem, instruction as u16);
        } else {
            self.pipeline[1] = mem.fetch_u32(pc);
            if condition_passed(instruction >> 28, self.cpsr) {
                ARM_LUT[index_arm(instruction)](self, mem, instruction);
            }
        }

        if self.branch_happened {
            self.flush_pipeline(mem);
        } else {
            self.r[REGISTER_PC as usize] = pc.wrapping_add(self.instruction_len_in_bytes());
        }

        self.cycles += mem.take_cycles() as u64;
    }

    #[inline(always)]
    pub fn instruction_len_in_bytes(&self) -> u32 {
        if self.get_thumb_state() {
            INSTRUCTION_LEN_THUMB
        } else {
            INSTRUCTION_LEN_ARM
        }
    }

    #[inline(always)]
    pub fn next_instruction_address_from_execution_stage(&self) -> u32 {
        self.r[REGISTER_PC as usize].wrapping_sub(self.instruction_len_in_bytes())
    }

    #[inline(always)]
    pub fn curr_instruction_address_from_execution_stage(&self) -> u32 {
        self.r[REGISTER_PC as usize].wrapping_sub(self.instruction_len_in_bytes() * 2)
    }

    #[inline(always)]
    pub fn get_negative_flag(&self) -> bool {
        self.cpsr & FLAG_N != 0
    }
    #[inline(always)]
    pub fn get_zero_flag(&self) -> bool {
        self.cpsr & FLAG_Z != 0
    }
    #[inline(always)]
    pub fn get_carry_flag(&self) -> bool {
        self.cpsr & FLAG_C != 0
    }
    #[inline(always)]
    pub fn get_overflow_flag(&self) -> bool {
        self.cpsr & FLAG_V != 0
    }

    #[inline(always)]
    pub fn set_nz(&mut self, result: u32) {
        self.set_nz_flags(result & FLAG_N != 0, result == 0);
    }

    #[inline(always)]
    pub fn set_nz_flags(&mut self, negative: bool, zero: bool) {
        self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z)) | (negative as u32) << 31 | (zero as u32) << 30;
    }

    #[inline(always)]
    pub fn set_nzc(&mut self, result: u32, carry: bool) {
        self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C)) | (result & FLAG_N) | ((result == 0) as u32) << 30 | (carry as u32) << 29;
    }

    #[inline(always)]
    pub fn set_nzcv(&mut self, result: u32, carry: bool, overflow: bool) {
        self.cpsr = (self.cpsr & !(FLAG_N | FLAG_Z | FLAG_C | FLAG_V)) | (result & FLAG_N) | ((result == 0) as u32) << 30 | (carry as u32) << 29 | (overflow as u32) << 28;
    }

    #[inline(always)]
    pub fn get_irq_disable(&self) -> bool {
        self.cpsr & FLAG_I != 0
    }
    pub fn set_irq_disable(&mut self, v: bool) {
        self.cpsr = (self.cpsr & !FLAG_I) | (v as u32) << 7;
    }

    pub fn get_fiq_disable(&self) -> bool {
        self.cpsr & FLAG_F != 0
    }
    pub fn set_fiq_disable(&mut self, v: bool) {
        self.cpsr = (self.cpsr & !FLAG_F) | (v as u32) << 6;
    }

    #[inline(always)]
    pub fn get_thumb_state(&self) -> bool {
        self.cpsr & FLAG_T != 0
    }
    #[inline(always)]
    pub fn set_thumb_state(&mut self, v: bool) {
        self.cpsr = (self.cpsr & !FLAG_T) | (v as u32) << 5;
    }

    #[inline(always)]
    pub fn get_mode(&self) -> u8 {
        (self.cpsr & MODE_MASK) as u8
    }
    pub fn set_mode(&mut self, v: u8) {
        self.set_cpsr((self.cpsr & !MODE_MASK) | v as u32);
    }
    #[inline(always)]
    pub fn current_mode_has_spsr(&self) -> bool {
        self.bank != BANK_USR
    }
    #[inline(always)]
    pub fn in_a_privileged_mode(&self) -> bool {
        self.get_mode() != MODE_USR
    }
    #[inline(always)]
    pub fn get_cycles(&self) -> u64 {
        self.cycles
    }
    #[inline(always)]
    pub fn add_cycles(&mut self, cycles: u64) {
        self.cycles += cycles;
    }

    #[inline(always)]
    pub fn handle_interrupts(&mut self, mem: &mut Memory) {
        let io = mem.get_io_registers();
        if io.ime && !self.get_irq_disable() && io.ie & io.irf != 0 {
            let old_cpsr = self.cpsr;
            self.set_mode(MODE_IRQ);
            self.set_spsr(old_cpsr);
            self.set_irq_disable(true);
            let return_addr = self.pc().wrapping_add(4);
            self.set_r(REGISTER_LR, return_addr);
            self.set_thumb_state(false);
            self.r[REGISTER_PC as usize] = 0x18;
            self.flush_pipeline(mem);
        }
    }

    pub fn save_state(&self, writer: &mut Writer) {
        writer.u32(self.cpsr);
        writer.u32s(&self.r);
        for bank in &self.banked {
            writer.u32s(bank);
        }
        writer.u32s(&self.spsr);
        writer.u8(self.bank as u8);
        writer.u32s(&self.pipeline);
        writer.bool(self.branch_happened);
        writer.u64(self.cycles);
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.cpsr = reader.u32()?;
        reader.u32s(&mut self.r)?;
        for bank in &mut self.banked {
            reader.u32s(bank)?;
        }
        reader.u32s(&mut self.spsr)?;
        self.bank = reader.u8()? as usize;
        if self.bank >= BANK_COUNT {
            return Err(StateError::Corrupt);
        }
        reader.u32s(&mut self.pipeline)?;
        self.branch_happened = reader.bool()?;
        self.cycles = reader.u64()?;
        Ok(())
    }

    pub fn print_registers(&self) {
        for i in (0..16u8).step_by(4) {
            println!(
                "r{:2}: {:08X}   r{:2}: {:08X}   r{:2}: {:08X}   r{:2}: {:08X}",
                i,
                self.get_r(i),
                i + 1,
                self.get_r(i + 1),
                i + 2,
                self.get_r(i + 2),
                i + 3,
                self.get_r(i + 3),
            );
        }
    }

    pub fn print_status(&self) {
        println!(
            "CPSR: {:08X} [{}{}{}{}{}{}{}] MODE: {}",
            self.cpsr,
            if self.get_negative_flag() { 'N' } else { '-' },
            if self.get_zero_flag() { 'Z' } else { '-' },
            if self.get_carry_flag() { 'C' } else { '-' },
            if self.get_overflow_flag() { 'V' } else { '-' },
            if self.get_irq_disable() { 'I' } else { '-' },
            if self.get_fiq_disable() { 'F' } else { '-' },
            if self.get_thumb_state() { 'T' } else { '-' },
            format_mode(self.get_mode()),
        );
    }

    pub fn print_next_instruction(&self) {
        let pc = self.pc();
        if self.get_thumb_state() {
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
        cpu.set_r(13, 0x1000);
        cpu.set_r(8, 0x88);
        cpu.set_mode(MODE_IRQ);
        assert_eq!(cpu.get_r(8), 0x88);
        cpu.set_r(13, 0x2000);
        cpu.set_mode(MODE_FIQ);
        cpu.set_r(8, 0xF8);
        cpu.set_r(13, 0x3000);
        assert_eq!(cpu.get_r_in_mode(8, MODE_USR), 0x88);
        assert_eq!(cpu.get_r_in_mode(13, MODE_IRQ), 0x2000);
        assert_eq!(cpu.get_r_in_mode(13, MODE_SVC), 0x1000);
        cpu.set_mode(MODE_SVC);
        assert_eq!(cpu.get_r(8), 0x88);
        assert_eq!(cpu.get_r(13), 0x1000);
        assert_eq!(cpu.get_r_in_mode(8, MODE_FIQ), 0xF8);
        assert_eq!(cpu.get_r_in_mode(13, MODE_FIQ), 0x3000);
    }

    #[test]
    fn test_set_r_in_user_mode_from_privileged_mode() {
        let mut cpu = CPU::new();
        cpu.set_mode(MODE_USR);
        cpu.set_r(13, 0x1234);
        cpu.set_mode(MODE_IRQ);
        cpu.set_r_in_mode(13, MODE_USR, 0x5678);
        cpu.set_r_in_mode(8, MODE_USR, 0x9);
        assert_eq!(cpu.get_r(8), 0x9);
        cpu.set_mode(MODE_SYS);
        assert_eq!(cpu.get_r(13), 0x5678);
    }
}
