use crate::bits::Bits;

use crate::system::state::{Reader, StateError, Writer};

const COUNTER_RANGE: u32 = 0x1_0000;

#[derive(Debug, Clone, Copy, Default)]
struct Timer {
    reload: u16,
    control: u16,
    counter: u16,
    cycles: u32,
}

impl Timer {
    fn enabled(&self) -> bool {
        self.control.bit(7)
    }

    fn raises_irq(&self) -> bool {
        self.control.bit(6)
    }

    fn cascades(&self) -> bool {
        self.control.bit(2)
    }

    fn prescaler_shift(&self) -> u32 {
        match self.control.bits(0..2) {
            0 => 0,
            1 => 6,
            2 => 8,
            _ => 10,
        }
    }

    fn counts_cycles(&self, index: usize) -> bool {
        self.enabled() && (index == 0 || !self.cascades())
    }

    fn cycles_until_overflow(&self) -> u32 {
        ((COUNTER_RANGE - u32::from(self.counter)) << self.prescaler_shift()) - self.cycles
    }
}

#[derive(Debug, Default)]
pub struct Timers {
    timers: [Timer; 4],
    overflows: u8,
    pending: u32,
    budget: u32,
}

impl Timers {
    pub fn read(&self, index: usize, control: bool) -> u16 {
        let timer = &self.timers[index];
        if control {
            timer.control
        } else if timer.counts_cycles(index) {
            let elapsed = (timer.cycles + self.pending) >> timer.prescaler_shift();
            timer.counter.wrapping_add(elapsed as u16)
        } else {
            timer.counter
        }
    }

    pub fn write(&mut self, index: usize, control: bool, value: u16) -> u8 {
        let overflows = self.flush();
        let timer = &mut self.timers[index];
        if control {
            if value.bit(7) && !timer.enabled() {
                timer.counter = timer.reload;
                timer.cycles = 0;
            }
            timer.control = value;
        } else {
            timer.reload = value;
        }
        self.budget = self.cycles_until_next_overflow();
        overflows
    }

    pub fn irq_mask(&self, overflows: u8) -> u8 {
        self.timers
            .iter()
            .enumerate()
            .filter(|(index, timer)| overflows.bit(*index as u32) && timer.raises_irq())
            .fold(0, |mask, (index, _)| mask | 1 << index)
    }

    #[inline]
    pub fn tick(&mut self, cycles: u32) -> u8 {
        self.pending += cycles;
        if self.pending < self.budget {
            0
        } else {
            self.flush()
        }
    }

    #[inline]
    pub fn cycles_until_flush(&self) -> u32 {
        self.budget.saturating_sub(self.pending).max(1)
    }

    fn flush(&mut self) -> u8 {
        let cycles = std::mem::take(&mut self.pending);
        for index in 0..self.timers.len() {
            let timer = self.timers[index];
            if timer.counts_cycles(index) {
                let shift = timer.prescaler_shift();
                let accumulated = timer.cycles + cycles;
                self.timers[index].cycles = accumulated.bits(0..shift);
                self.increment(index, accumulated >> shift);
            }
        }
        self.budget = self.cycles_until_next_overflow();
        std::mem::take(&mut self.overflows)
    }

    fn cycles_until_next_overflow(&self) -> u32 {
        self.timers
            .iter()
            .enumerate()
            .filter(|(index, timer)| timer.counts_cycles(*index))
            .map(|(_, timer)| timer.cycles_until_overflow())
            .min()
            .unwrap_or(COUNTER_RANGE)
            .max(1)
    }

    fn increment(&mut self, index: usize, ticks: u32) {
        let mut ticks = ticks;
        while ticks > 0 {
            let timer = &mut self.timers[index];
            let space = COUNTER_RANGE - u32::from(timer.counter);
            if ticks < space {
                timer.counter = timer.counter.wrapping_add(ticks as u16);
                return;
            }
            ticks -= space;
            timer.counter = timer.reload;
            self.overflows |= 1 << index;
            if self.timers.get(index + 1).is_some_and(|next| next.enabled() && next.cascades()) {
                self.increment(index + 1, 1);
            }
        }
    }

    pub fn save_state(&self, writer: &mut Writer) {
        for timer in &self.timers {
            writer.u16(timer.reload);
            writer.u16(timer.control);
            writer.u16(timer.counter);
            writer.u32(timer.cycles);
        }
        writer.u8(self.overflows);
        writer.u32(self.pending);
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        for timer in &mut self.timers {
            timer.reload = reader.u16()?;
            timer.control = reader.u16()?;
            timer.counter = reader.u16()?;
            timer.cycles = reader.u32()?;
        }
        self.overflows = reader.u8()?;
        self.pending = reader.u32()?;
        self.budget = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_counts_with_prescaler_and_overflows() {
        let mut timers = Timers::default();
        timers.write(0, false, 0xFFFE);
        timers.write(0, true, 0x80 | 1);
        assert_eq!(timers.tick(64), 0);
        assert_eq!(timers.read(0, false), 0xFFFF);
        assert_eq!(timers.tick(64), 0b1);
        assert_eq!(timers.read(0, false), 0xFFFE);
    }

    #[test]
    fn test_cascade_counts_overflows_of_the_previous_timer() {
        let mut timers = Timers::default();
        timers.write(0, false, 0xFFFF);
        timers.write(0, true, 0x80);
        timers.write(1, true, 0x80 | 0x04 | 0x40);
        let overflows = timers.tick(3);
        assert_eq!(overflows, 0b01);
        assert_eq!(timers.read(1, false), 3);
        assert_eq!(timers.irq_mask(0b11), 0b10);
    }
}
