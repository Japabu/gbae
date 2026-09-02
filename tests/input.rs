mod common;

use common::*;
use gbae::system::memory::Key;

const KEYINPUT: u32 = 0x0400_0130;
const KEYCNT: u32 = 0x0400_0132;
const IF: u32 = 0x0400_0202;

#[test]
fn keyinput_is_active_low() {
    let mut gba = gba_without_rom();
    assert_eq!(gba.mem.read_u16(KEYINPUT), 0x03FF);
    gba.set_key(Key::A, true);
    gba.set_key(Key::Down, true);
    assert_eq!(gba.mem.read_u16(KEYINPUT), 0x03FF & !(Key::A.bit() | Key::Down.bit()));
    gba.set_key(Key::A, false);
    assert_eq!(gba.mem.read_u16(KEYINPUT), 0x03FF & !Key::Down.bit());
}

#[test]
fn keycnt_raises_irq_when_any_selected_key_is_pressed() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(KEYCNT, 1 << 14 | Key::Start.bit() | Key::Select.bit());
    gba.set_key(Key::A, true);
    assert_eq!(gba.mem.read_u16(IF) & 1 << 12, 0);
    gba.set_key(Key::Select, true);
    assert_eq!(gba.mem.read_u16(IF) & 1 << 12, 1 << 12);
}

#[test]
fn keycnt_and_mode_needs_all_selected_keys() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(KEYCNT, 1 << 15 | 1 << 14 | Key::Start.bit() | Key::Select.bit());
    gba.set_key(Key::Select, true);
    assert_eq!(gba.mem.read_u16(IF) & 1 << 12, 0);
    gba.set_key(Key::Start, true);
    assert_eq!(gba.mem.read_u16(IF) & 1 << 12, 1 << 12);
}

#[test]
fn keycnt_write_checks_current_keys() {
    let mut gba = gba_without_rom();
    gba.set_key(Key::B, true);
    assert_eq!(gba.mem.read_u16(IF), 0);
    gba.mem.write_u16(KEYCNT, 1 << 14 | Key::B.bit());
    assert_eq!(gba.mem.read_u16(IF) & 1 << 12, 1 << 12);
}
