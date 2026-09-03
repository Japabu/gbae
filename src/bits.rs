use std::ops::{Bound, RangeBounds};

pub trait Bits: Copy {
    const WIDTH: u32;

    fn bit(self, index: u32) -> bool;
    fn bits(self, range: impl RangeBounds<u32>) -> Self;
    fn with_bit(self, index: u32, value: bool) -> Self;
    fn with_bits(self, range: impl RangeBounds<u32>, value: Self) -> Self;
    fn sign_extended(self, width: u32) -> Self;
    fn arithmetic_shift_right(self, amount: u32) -> Self;
}

fn span(range: impl RangeBounds<u32>, width: u32) -> (u32, u32) {
    let start = match range.start_bound() {
        Bound::Included(start) => *start,
        Bound::Excluded(start) => start + 1,
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        Bound::Included(end) => end + 1,
        Bound::Excluded(end) => *end,
        Bound::Unbounded => width,
    };
    (start, end - start)
}

macro_rules! bits {
    ($($unsigned:ty => $signed:ty),*) => {
        $(impl Bits for $unsigned {
            const WIDTH: u32 = <$unsigned>::BITS;

            #[inline(always)]
            fn bit(self, index: u32) -> bool {
                self >> index & 1 != 0
            }

            #[inline(always)]
            fn bits(self, range: impl RangeBounds<u32>) -> Self {
                let (start, length) = span(range, Self::WIDTH);
                let shifted = self >> start;
                if length >= Self::WIDTH {
                    shifted
                } else {
                    shifted & ((1 << length) - 1)
                }
            }

            #[inline(always)]
            fn with_bit(self, index: u32, value: bool) -> Self {
                if value {
                    self | 1 << index
                } else {
                    self & !(1 << index)
                }
            }

            #[inline(always)]
            fn with_bits(self, range: impl RangeBounds<u32>, value: Self) -> Self {
                let (start, length) = span(range, Self::WIDTH);
                let mask = if length >= Self::WIDTH { !0 } else { ((1 << length) - 1) << start };
                (self & !mask) | (value << start & mask)
            }

            #[inline(always)]
            fn sign_extended(self, width: u32) -> Self {
                let shift = Self::WIDTH - width;
                ((self << shift) as $signed >> shift) as $unsigned
            }

            #[inline(always)]
            fn arithmetic_shift_right(self, amount: u32) -> Self {
                (self as $signed >> amount) as $unsigned
            }
        })*
    };
}

bits!(u8 => i8, u16 => i16, u32 => i32, u64 => i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arithmetic {
    pub result: u32,
    pub carry: bool,
    pub overflow: bool,
}

impl Arithmetic {
    #[inline(always)]
    pub fn add(a: u32, b: u32) -> Arithmetic {
        Arithmetic::add_with_carry(a, b, false)
    }

    #[inline(always)]
    pub fn add_with_carry(a: u32, b: u32, carry: bool) -> Arithmetic {
        let (partial, first_carry) = a.overflowing_add(b);
        let (result, second_carry) = partial.overflowing_add(u32::from(carry));
        Arithmetic {
            result,
            carry: first_carry | second_carry,
            overflow: ((a ^ result) & (b ^ result)).bit(31),
        }
    }

    #[inline(always)]
    pub fn sub(a: u32, b: u32) -> Arithmetic {
        Arithmetic::sub_with_carry(a, b, true)
    }

    #[inline(always)]
    pub fn sub_with_carry(a: u32, b: u32, carry: bool) -> Arithmetic {
        let (partial, first_borrow) = a.overflowing_sub(b);
        let (result, second_borrow) = partial.overflowing_sub(u32::from(!carry));
        Arithmetic {
            result,
            carry: !(first_borrow | second_borrow),
            overflow: ((a ^ b) & (a ^ result)).bit(31),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_ranges() {
        assert_eq!(0b1011_0000u8.bits(6..8), 0b10);
        assert_eq!(0b1011_0000u8.bits(4..=7), 0b1011);
        assert_eq!(0x1234_5678u32.bits(..), 0x1234_5678);
        assert_eq!(0x1234_5678u32.bits(28..), 0x1);
        assert_eq!(0xFFu16.bits(0..1), 1);
        assert!(0b100u32.bit(2));
        assert!(!0b100u32.bit(1));
    }

    #[test]
    fn test_bit_replacement() {
        assert_eq!(0b1111_1111u32.with_bits(4..8, 0b0110), 0b0110_1111);
        assert_eq!(0u32.with_bits(0..8, 0xFFF), 0xFF);
        assert_eq!(0u32.with_bits(.., 0xFFFF_FFFF), 0xFFFF_FFFF);
        assert_eq!(0u32.with_bit(0, true), 1);
        assert_eq!(0b11u32.with_bit(1, false), 0b01);
    }

    #[test]
    fn test_sign_extension() {
        assert_eq!(0x7FFu32.sign_extended(12), 0x7FF);
        assert_eq!(0x800u32.sign_extended(12), 0xFFFF_F800);
        assert_eq!(0x80u32.sign_extended(8), 0xFFFF_FF80);
        assert_eq!(1u32.sign_extended(1), 0xFFFF_FFFF);
        assert_eq!(0xFFFF_FFFFu32.sign_extended(32), 0xFFFF_FFFF);
        assert_eq!(0x80u16.sign_extended(8), 0xFF80);
    }

    #[test]
    fn test_arithmetic_shift_right() {
        assert_eq!(0x8000_0001u32.arithmetic_shift_right(1), 0xC000_0000);
        assert_eq!(0x7FFF_FFFFu32.arithmetic_shift_right(2), 0x1FFF_FFFF);
        assert_eq!(0xFFFF_FFFFu32.arithmetic_shift_right(0), 0xFFFF_FFFF);
    }

    #[test]
    fn test_addition_flags() {
        assert_eq!(
            Arithmetic::add(1, 1),
            Arithmetic {
                result: 2,
                carry: false,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::add(u32::MAX, 1),
            Arithmetic {
                result: 0,
                carry: true,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::add(0x7FFF_FFFF, 1),
            Arithmetic {
                result: 0x8000_0000,
                carry: false,
                overflow: true
            }
        );
        assert_eq!(
            Arithmetic::add(0x8000_0000, 0x8000_0000),
            Arithmetic {
                result: 0,
                carry: true,
                overflow: true
            }
        );
        assert_eq!(
            Arithmetic::add_with_carry(1, 1, true),
            Arithmetic {
                result: 3,
                carry: false,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::add_with_carry(u32::MAX, 0, true),
            Arithmetic {
                result: 0,
                carry: true,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::add_with_carry(0x7FFF_FFFF, 1, true),
            Arithmetic {
                result: 0x8000_0001,
                carry: false,
                overflow: true
            }
        );
    }

    #[test]
    fn test_subtraction_flags() {
        assert_eq!(
            Arithmetic::sub(2, 1),
            Arithmetic {
                result: 1,
                carry: true,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::sub(0, 1),
            Arithmetic {
                result: 0xFFFF_FFFF,
                carry: false,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::sub(0x7FFF_FFFF, 0x8000_0000),
            Arithmetic {
                result: 0xFFFF_FFFF,
                carry: false,
                overflow: true
            }
        );
        assert_eq!(
            Arithmetic::sub(0x8000_0000, 1),
            Arithmetic {
                result: 0x7FFF_FFFF,
                carry: true,
                overflow: true
            }
        );
        assert_eq!(
            Arithmetic::sub_with_carry(2, 1, false),
            Arithmetic {
                result: 0,
                carry: true,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::sub_with_carry(0, 0, false),
            Arithmetic {
                result: 0xFFFF_FFFF,
                carry: false,
                overflow: false
            }
        );
        assert_eq!(
            Arithmetic::sub_with_carry(0x8000_0000, 1, false),
            Arithmetic {
                result: 0x7FFF_FFFE,
                carry: true,
                overflow: true
            }
        );
    }
}
