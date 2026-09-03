mod common;

use common::*;
use gbae::system::state::StateError;

const IWRAM: u32 = 0x0300_0000;

fn machine_with_activity() -> gbae::system::gba::Gba {
    let mut gba = gba_with_save_marker("SRAM_V");
    gba.mem.write_u16(PALETTE + 2, rgb555(31, 0, 0));
    gba.mem.write_u16(DISPCNT, 4 | 1 << 10);
    for i in 0..240u32 {
        gba.mem.write_u8(VRAM + i, 1);
    }
    gba.mem.write_u8(0x0E00_0010, 0x42);
    gba.mem.write_u32(IWRAM + 0x100, 0xCAFE_BABE);
    for _ in 0..3 {
        gba.run_frame();
    }
    gba
}

#[test]
fn state_round_trip_restores_the_machine() {
    let mut original = machine_with_activity();
    let state = original.save_state();
    let cycles = original.cpu.get_cycles();

    let mut restored = gba_with_save_marker("SRAM_V");
    restored.load_state(&state).unwrap();
    assert_eq!(restored.cpu.get_cycles(), cycles);
    assert_eq!(restored.cpu.pc(), original.cpu.pc());
    assert_eq!(restored.frame_count(), original.frame_count());
    assert_eq!(restored.mem.read_u32(IWRAM + 0x100), 0xCAFE_BABE);
    assert_eq!(restored.mem.read_u8(0x0E00_0010), 0x42);
    assert_eq!(restored.framebuffer()[0][0], rgb(31, 0, 0));
    assert_eq!(frame_hash(restored.framebuffer()), frame_hash(original.framebuffer()));

    original.take_audio_samples();
    restored.take_audio_samples();
    for _ in 0..2 {
        original.run_frame();
        restored.run_frame();
    }
    assert_eq!(restored.cpu.get_cycles(), original.cpu.get_cycles());
    assert_eq!(frame_hash(restored.framebuffer()), frame_hash(original.framebuffer()));
    assert_eq!(restored.take_audio_samples().len(), original.take_audio_samples().len());
}

#[test]
fn state_from_another_rom_is_rejected() {
    let state = machine_with_activity().save_state();
    let mut other = gba_without_rom();
    assert_eq!(other.load_state(&state), Err(StateError::DifferentRom));
}

#[test]
fn damaged_states_are_rejected() {
    let state = machine_with_activity().save_state();
    let mut gba = gba_with_save_marker("SRAM_V");
    assert_eq!(gba.load_state(&state[..state.len() / 2]), Err(StateError::Truncated));
    assert_eq!(gba.load_state(b"nonsense"), Err(StateError::BadMagic));
}
