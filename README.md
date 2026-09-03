# gbae

A Game Boy Advance emulator in Rust.

The core (`gbae` library) is a cycle-counting ARM7TDMI, the full GBA memory
map with wait states and the ROM prefetch buffer, a scanline PPU with every
background mode, sprites, windows and colour effects, the four PSG channels
plus Direct Sound, DMA and timers, flash/SRAM/EEPROM saves and the cartridge
real-time clock. It has no dependencies of its own beyond `seq-macro`.

The window build adds `winit`, `softbuffer` and `cpal` and nothing else.

## Running

```
cargo run --release              # loads rom.gba from the current directory
cargo run --release -- game.gba  # or a specific ROM
```

Without a ROM the Escape menu opens on a file browser. The emulator has its
own BIOS built in, so nothing else is needed; a `gba_bios.bin` next to the
executable or a `GBA_BIOS=path` variable selects an original BIOS image
instead, which restores the boot logo and its jingle.

Default controls (change them in the menu, saved to `gbae.cfg`):

| GBA | key |
|---|---|
| D-pad | arrow keys |
| A / B | Z / X |
| L / R | A / S |
| Start / Select | Enter / Backspace |
| menu | Escape |

The Escape menu offers resume, reset, save state, load state, a ROM browser,
volume, turbo speed, a sound smoothing switch and key mapping. Save data is written to `<rom>.sav` next
to the ROM, save states to `<rom>.state`.

## Tests

```
cargo test
```

Everything the tests need is generated in the tests themselves: the machines
boot the built-in BIOS into ROMs assembled from `Instruction` values, so a
fresh clone runs the whole suite without downloading anything. Golden-frame
tests compare rendered frames with PNGs in `tests/golden`; run with
`UPDATE_GOLDEN=1` to re-record after checking the new image.

Headless tools in `examples/`:

```
cargo run --release --example render -- - rom.gba 400 frame.png     # "-" selects the built-in BIOS
cargo run --release --example trace -- - rom.gba 1000 break=0x08000100
cargo run --release --example bench -- - rom.gba 400
```

## Layout

```
src/system/instructions   one file per instruction family: decode, execute, disassemble
src/system/cpu.rs         registers, pipeline, exceptions
src/system/memory.rs      memory map, IO registers, DMA, timers, wait states, prefetch
src/system/ppu.rs         scanline renderer
src/system/apu.rs         sound
src/system/save.rs        SRAM, flash, EEPROM
src/system/rtc.rs         GPIO and the S-3511 clock
src/system/state.rs       save state format
src/system/gba.rs         the machine and its scheduler
src/main.rs, menu.rs, audio.rs, config.rs, font.rs   window build
```

See `ARCHITECTURE.md` for how the pieces fit together.

## Acknowledgments

- [GBATEK](https://problemkaputt.de/gbatek.htm) and the ARM7TDMI technical reference manual
- [jsmolka/gba-tests](https://github.com/jsmolka/gba-tests) and [DenSinH/FuzzARM](https://github.com/DenSinH/FuzzARM) test ROMs
- the public domain [font8x8](https://github.com/dhepper/font8x8) used by the menu
