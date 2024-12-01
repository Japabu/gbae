use std::ops::{Index, IndexMut};

use crate::bitutil::{get_bit, get_bits, set_bit, set_bits};

pub const MODE_USR: u32 = 0b10000;
pub const MODE_FIQ: u32 = 0b10001;
pub const MODE_IRQ: u32 = 0b10010;
pub const MODE_SVC: u32 = 0b10011;
pub const MODE_ABT: u32 = 0b10111;
pub const MODE_UND: u32 = 0b11011;
pub const MODE_SYS: u32 = 0b11111;


pub struct Registers {
    r: [u32; 16],   /* r13: stack pointer, r14: link register, r15: pc */
    cpsr: u32,      /* current program status register */
    spsr: u32,      /* saved program status register */
}

impl Registers {
    pub fn new() -> Self {
        Registers {
            r: [0; 16],
            cpsr: 0,
            spsr: 0,
        }
    }

    pub fn get_negative_flag(&self) -> bool {
        get_bit(self.cpsr, 31)
    }
    pub fn set_negative_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 31, v);
    }

    pub fn get_zero_flag(&self) -> bool {
        get_bit(self.cpsr, 30)
    }
    pub fn set_zero_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 30, v);
    }
    pub fn get_carry_flag(&self) -> bool {
        get_bit(self.cpsr, 29)
    }
    pub fn set_carry_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 29, v);
    }

    pub fn get_overflow_flag(&self) -> bool {
        get_bit(self.cpsr, 28)
    }
    pub fn set_overflow_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 28, v);
    }

    pub fn get_irq_disable(&self) -> bool {
        get_bit(self.cpsr, 7)
    }
    pub fn set_irq_disable(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 7, v);
    }

    pub fn get_fiq_disable(&self) -> bool {
        get_bit(self.cpsr, 6)
    }
    pub fn set_fiq_disable(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 6, v);
    }

    pub fn get_thumb_state(&self) -> bool {
        get_bit(self.cpsr, 5)
    }
    pub fn set_thumb_state(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 5, v);
    }

    pub fn get_mode(&self) -> u32 {
        get_bits(self.cpsr, 0, 5)
    }
    pub fn set_mode(&mut self, v: u32) {
        self.cpsr = set_bits(self.cpsr, 0, 5, v);
    }
    pub fn current_mode_has_spsr(&self) -> bool {
        self.get_mode() != MODE_USR
    }
}

impl Index<u32> for Registers {
    type Output = u32;

    fn index(&self, index: u32) -> &Self::Output {
        &self.r[index as usize]
    }
}

impl IndexMut<u32> for Registers {
    fn index_mut(&mut self, index: u32) -> &mut Self::Output {
        &mut self.r[index as usize]
    }
}