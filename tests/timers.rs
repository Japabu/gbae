mod common;

use common::*;

const TM0CNT_L: u32 = 0x0400_0100;
const TM0CNT_H: u32 = 0x0400_0102;
const TM1CNT_L: u32 = 0x0400_0104;
const TM1CNT_H: u32 = 0x0400_0106;
const IF: u32 = 0x0400_0202;
const ENABLE: u16 = 1 << 7;
const IRQ: u16 = 1 << 6;
const COUNT_UP: u16 = 1 << 2;
const PRESCALER_64: u16 = 1;
const TIMER0_IRQ: u16 = 1 << 3;

fn steps(gba: &mut gbae::system::gba::Gba, count: u32) {
    for _ in 0..count {
        gba.step();
    }
}

#[test]
fn timer_counts_every_cycle_without_prescaler() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(TM0CNT_L, 0);
    gba.mem.write_u16(TM0CNT_H, ENABLE);
    steps(&mut gba, 100);
    assert_eq!(gba.mem.read_u16(TM0CNT_L), 200);
}

#[test]
fn timer_counts_with_prescaler_64() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(TM0CNT_L, 0);
    gba.mem.write_u16(TM0CNT_H, ENABLE | PRESCALER_64);
    steps(&mut gba, 64);
    assert_eq!(gba.mem.read_u16(TM0CNT_L), 2);
}

#[test]
fn timer_reloads_and_raises_irq_on_overflow() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(TM0CNT_L, 0xFFF0);
    gba.mem.write_u16(TM0CNT_H, ENABLE | IRQ);
    steps(&mut gba, 7);
    assert_eq!(gba.mem.read_u16(TM0CNT_L), 0xFFFE);
    assert_eq!(gba.mem.read_u16(IF) & TIMER0_IRQ, 0);
    steps(&mut gba, 1);
    assert_eq!(gba.mem.read_u16(TM0CNT_L), 0xFFF0);
    assert_eq!(gba.mem.read_u16(IF) & TIMER0_IRQ, TIMER0_IRQ);
}

#[test]
fn writing_if_acknowledges_flags() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(TM0CNT_L, 0xFFFE);
    gba.mem.write_u16(TM0CNT_H, ENABLE | IRQ);
    steps(&mut gba, 1);
    assert_eq!(gba.mem.read_u16(IF) & TIMER0_IRQ, TIMER0_IRQ);
    gba.mem.write_u16(IF, TIMER0_IRQ);
    assert_eq!(gba.mem.read_u16(IF) & TIMER0_IRQ, 0);
}

#[test]
fn count_up_timer_counts_overflows_of_previous_timer() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(TM1CNT_L, 0);
    gba.mem.write_u16(TM1CNT_H, ENABLE | COUNT_UP);
    gba.mem.write_u16(TM0CNT_L, 0xFFFF);
    gba.mem.write_u16(TM0CNT_H, ENABLE);
    steps(&mut gba, 4);
    assert_eq!(gba.mem.read_u16(TM1CNT_L), 8);
}

#[test]
fn disabled_timer_does_not_count() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(TM0CNT_L, 0x1234);
    steps(&mut gba, 10);
    assert_eq!(gba.mem.read_u16(TM0CNT_L), 0);
}
