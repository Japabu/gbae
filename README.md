# GBAE - Game Boy Advance Emulator

A Game Boy Advance emulator written in Rust.

## Features

- ARM7TDMI CPU emulation
  - ARM and Thumb instruction sets
  - CPU flag handling
- Memory system
  - BIOS ROM
  - Game ROM (cartridge)
  - Work RAM
  - Memory-mapped I/O

## Building

Requires Rust and Cargo. To build:

```bash
cargo build
```

For optimized release build:

```bash
cargo build --release
```

## Running

To run the emulator:

```bash
cargo run
```

## Testing

Run the test suite:

```bash
cargo test
```

## Acknowledgments

- [GBATEK](https://problemkaputt.de/gbatek.htm) - Technical documentation
- [ARM7TDMI Technical Reference Manual](https://documentation-service.arm.com/static/5e8e353cef2d0b5d1f41a560) - CPU documentation
