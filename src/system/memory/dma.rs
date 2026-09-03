use crate::bits::Bits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaTiming {
    Immediate,
    VBlank,
    HBlank,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjust {
    Increment,
    Decrement,
    Fixed,
    Reload,
}

impl Adjust {
    const ALL: [Adjust; 4] = [Adjust::Increment, Adjust::Decrement, Adjust::Fixed, Adjust::Reload];

    fn from_bits(bits: u16) -> Adjust {
        Adjust::ALL[bits.bits(0..2) as usize]
    }

    pub fn apply(self, address: u32, step: u32) -> u32 {
        match self {
            Adjust::Increment | Adjust::Reload => address.wrapping_add(step),
            Adjust::Decrement => address.wrapping_sub(step),
            Adjust::Fixed => address,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DmaControl(pub u16);

impl DmaControl {
    pub fn enabled(self) -> bool {
        self.0.bit(15)
    }

    pub fn raises_irq(self) -> bool {
        self.0.bit(14)
    }

    pub fn timing(self) -> DmaTiming {
        match self.0.bits(12..14) {
            0 => DmaTiming::Immediate,
            1 => DmaTiming::VBlank,
            2 => DmaTiming::HBlank,
            _ => DmaTiming::Special,
        }
    }

    pub fn transfers_words(self) -> bool {
        self.0.bit(10)
    }

    pub fn repeats(self) -> bool {
        self.0.bit(9)
    }

    pub fn source_adjust(self) -> Adjust {
        Adjust::from_bits(self.0.bits(7..9))
    }

    pub fn destination_adjust(self) -> Adjust {
        Adjust::from_bits(self.0.bits(5..7))
    }

    pub fn disabled(self) -> DmaControl {
        DmaControl(self.0.with_bit(15, false))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DmaRegisters {
    pub source: u32,
    pub destination: u32,
    pub count: u16,
    pub control: DmaControl,
}

impl DmaRegisters {
    pub fn length(&self, index: usize) -> u32 {
        match (index, self.count) {
            (3, 0) => 0x1_0000,
            (_, 0) => 0x4000,
            (3, count) => u32::from(count),
            (_, count) => u32::from(count.bits(0..14)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DmaChannel {
    pub armed: bool,
    pub source: u32,
    pub destination: u32,
    pub count: u32,
}
