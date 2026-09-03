# gbae

A Game Boy Advance emulator in Rust, MIT licensed, with its own BIOS.

The core (`gbae` library) is a cycle-counting ARM7TDMI with the full memory
map, PPU, sound, DMA, timers, saves and cartridge clock; its only dependency
is `seq-macro`. The window build adds `winit`, `softbuffer` and `cpal`.

## Running

```
cargo run --release -- game.gba
```

`gbae [ROM]` runs a ROM; without one the menu opens on its file browser.
`--help` and `--version` are the only options. Games boot with the
built-in BIOS.

| GBA | key |
|---|---|
| D-pad | arrow keys |
| A / B | Z / X |
| L / R | A / S |
| Start / Select | Enter / Backspace |
| turbo | Tab |
| menu | Escape |

The menu has resume, reset, save state, load state, a ROM browser, volume,
the turbo speed and key mapping. At 1.5x to 4x sound plays pitched up; at
Max the emulator is unthrottled and silent.

Settings are stored in `$XDG_CONFIG_HOME/gbae/config`, which is
`~/.config/gbae/config` by default and `%APPDATA%\gbae\config` on Windows.
Save data goes to `<rom>.sav` next to the ROM, save states to
`<rom>.state`. Nothing is read from the working directory or from next to
the executable.

Startup, in order: parse the command line (a usage error exits with status
2), read the settings file (a missing file means defaults), open the window
and the audio device, then load the ROM from the command line (an unreadable
ROM exits with status 1) or open the browser in the current directory.

## Tests and tools

```
cargo test
```

Tests need no external ROMs: each assembles its own from `Instruction`
values and boots it on the built-in BIOS. `tests/timing.rs` checks the
cycle count of every instruction class and bus region against the ARM7TDMI
rules. Golden-frame tests compare rendered frames with PNGs in
`tests/golden`; `UPDATE_GOLDEN=1` re-records them after you have checked
the new image.

`benchmark.rs` assembles a workload ROM: a scrolling tiled background,
moving sprites, a DMA tile animation, a Thumb loop in IWRAM, an ARM loop in
ROM, Direct Sound from a timer-driven DMA and a PSG note, waiting for VBlank
every frame. It is the bench's default and has a golden-frame test.

```
cargo run --release --example bench                                  # 1200 frames of the workload ROM
cargo run --release --example bench -- game.gba 400                  # or a ROM and a frame count
cargo run --release --example render -- game.gba 400 frame.png       # run 400 frames, save the picture
cargo run --release --example trace -- game.gba 1000 break=0x08000100 # trace 1000 instructions or stop at an address
```

## Layout

```
src/bits.rs                   Bits trait: bit and bit-range access on integers
src/system/instructions/      one file per instruction family: decode, execute, encode, disassemble
src/system/instructions/asm.rs   assembler with labels and literal pools, built on the encoders
src/system/cpu.rs             registers, banks, pipeline, exceptions
src/system/memory/            bus (wait states, prefetch), IO registers, timers, DMA, address map
src/system/bios.rs            built-in BIOS: assembled reset and IRQ code, native BIOS functions
src/system/ppu.rs             scanline renderer
src/system/apu.rs, synth.rs   sound channels, mixer, band-limited synthesis
src/system/save.rs, rtc.rs    SRAM, flash, EEPROM; GPIO with the S-3511 clock
src/system/state.rs           save-state format
src/system/gba.rs             the machine and its scanline scheduler
src/benchmark.rs              workload ROM for the bench and an end-to-end test
src/main.rs, menu.rs, audio.rs, config.rs, font.rs   window build
```

## The machine

`Gba` owns the `CPU`, the `Memory` and the `PPU`. Each step runs one
instruction, or skips to the next event while the CPU is halted, then hands
the elapsed cycles to memory, which advances timers, the APU and DMA. Two
events per scanline drive the display; the picture the frontend sees is the
one finished at the last VBlank, so it is always a whole frame.

```
        one scanline, 1232 cycles, 228 per frame (160 visible + 68 blank)
 |<-------------------- draw, 960 -------------------->|<----- hblank, 272 ----->|
 ^                                                     ^
 start_scanline                                        start_hblank
   VCOUNT, DISPSTAT flags, VCOUNT match IRQ              HBLANK flag and IRQ
   line 160: VBLANK flag, VBLANK IRQ, VBlank DMA,        lines 0..159: render the line,
             picture finished                                         HBlank DMA
```

## Instructions

Every family lives in one file under `system/instructions`: a plain data
struct, `decode_*` functions building it from an instruction word,
`execute`, `encode_arm` / `encode_thumb` and `disassemble`. The encoders are
checked against the decoders by round-trip tests over all 4096 ARM lookup
entries and all 65536 Thumb words.

```
 ARM word                                THUMB word
 bits 27..20 and 7..4 ──► 12-bit index   bits 15..6 ──► 10-bit index
                            │                              │
                     ARM_LUT[index]                  THUMB_LUT[index]
                            │                              │
             execute_arm::<INDEX>(word)     execute_thumb::<INDEX>(word)
                            │                              │
     put INDEX bits back, decode with the pattern table, execute
              "000xxxxx 1xx1" ─► load_store::decode_extra_arm
              "101xxxxx xxxx" ─► branch::decode_b_arm / decode_bl_arm
                            ▼
                  enum Instruction { DataProcessing, LoadStore, Branch, ... }
```

The index is a const generic, so each table entry compiles to its own
handler without a second description of any instruction.

## CPU

`cpu.rs` keeps the register file as a flat array and swaps banked registers
when the mode changes, so a register access is one index. `Register`, `Mode`
and `Psr` are types, not bare integers. A two-word fetch pipeline gives
self-modifying code, BIOS read protection and PC-relative reads their
hardware values.

## Memory

`memory/mod.rs` maps addresses to regions: `Bios`, `Ram`, `Io`, `Rom`,
`Gpio`, `Eeprom`, `Backup` and `Unmapped`. CPU accesses go through
`fetch_*`, `load_*` and `store_*`, which charge the bus for wait states,
sequential access and the ROM prefetch buffer; plain `read_*` / `write_*` are
untimed and used by DMA, the PPU, the BIOS functions and tests. Unmapped or
protected reads return the last fetched opcode.

IO registers live in `memory/io.rs`, with DMA and timer registers indexed by
channel. Timers keep a budget of cycles until the next overflow and report
overflows and IRQ requests to their owner; memory splits its cycle blocks at
those overflows. DMA channels latch their addresses on enable and run on
immediate, VBlank, HBlank or FIFO triggers.

## PPU

`ppu.rs` renders one scanline at a time into per-layer line buffers of
`Option<Color>`: text and affine backgrounds, the bitmap modes through the
same affine transform, and sprites with every shape, both mapping modes,
affine and double-size, the OBJ window, mosaic and the per-line cycle
budget. Composition takes the two topmost visible layers per pixel by
priority, applies window rules and colour effects, and writes RGB888.

## APU

```
 square 1, square 2, wave, noise ── evaluated on a 64-cycle grid ─┐
 FIFO A, FIFO B ── popped on timer overflow, refilled by DMA ─────┤
                                                                   ▼
                                      mixer: bias, volumes, clamp to 10 bits
                                                                   │ level changed at cycle t
                                                                   ▼
                   band-limited step: 32-tap windowed sinc, 256 phases, device rate
                                                                   ▼
                          15 Hz DC blocker ──► i16 stereo ──► audio device
```

The synthesizer runs at the audio device's rate, so nothing is resampled.
Turbo asks for a lower rate, which plays the same sound faster.

## BIOS

`bios.rs` assembles a 16 KB image with the reset sequence and the interrupt
dispatcher, and implements the BIOS functions natively: reset, halt and
IntrWait, division, square root and arc tangent, memory copies, affine
set-up, bit unpacking, the LZ77, Huffman, run-length and difference
decoders, SoundBias and MidiKey2Freq. The BIOS sound driver (functions 1Ah
to 1Eh, 20h to 24h and 28h to 2Ah) is not implemented: games bring their
own driver, and a call to one of these stops the emulator with a message.

## Saves, clock, states

`save.rs` detects the save type from the ROM's ID string and implements SRAM,
64K and 128K flash command sets and the EEPROM protocol. `rtc.rs` exposes
the cartridge GPIO pins with an S-3511 clock behind them, fed with the host
time. `state.rs` is a versioned binary format; every component has
`save_state` / `load_state`, and a state records the ROM length and hash so
it is only applied to the same game.

## Frontend

`main.rs` runs the winit event loop, presents frames through `softbuffer`
with integer scaling, streams audio through `cpal` and writes `.sav` when
save data changes. The audio device is the clock: after every buffer it
plays, the emulator runs frames until 50 ms of sound are queued again, so
video and sound never drift apart. Without an audio device frames are paced
by the wall clock; at Max they run unthrottled. `menu.rs` draws the Escape
menu with the bitmap font in `font.rs`; `config.rs` reads and writes the
settings file.

## Acknowledgments

- [GBATEK](https://problemkaputt.de/gbatek.htm) and the ARM7TDMI technical reference manual
- the public domain [font8x8](https://github.com/dhepper/font8x8) used by the menu
