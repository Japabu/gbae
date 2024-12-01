# GBAE - GBA Emulator (LLM Context Guide)

> **Purpose:** This guide provides rapid context for LLMs working on the GBAE emulator after context has been wiped.

## Quick Facts
- **Language:** Rust
- **Type:** Game Boy Advance emulator
- **Components:** ARM CPU, PPU (graphics), Memory-mapped I/O, MCP debugging server
- **Build:** `cargo build` / `cargo run`
- **State:** Emulator starts in HALTED state, must use `continue_execution` to run
- **Files Needed:** `gba_bios.bin` and `rom.gba` in project root

## Critical Gotchas (Read First!)

### 1. PPU Background Layer Bug
**COMMON MISTAKE:** Hardcoding BG2 when BIOS uses BG0
- BIOS sets `DISPCNT = 0x0100` (BG0 enabled, bit 8 = 1)
- If PPU only renders BG2, logo won't display
- **Fix:** Check DISPCNT bits 8-11, render the enabled BG layer
- **Location:** `src/system/ppu.rs:draw_scanline()`

### 2. MCP State Requirements
**COMMON MISTAKE:** Calling `read_memory` while emulator is running
- Most MCP inspection tools require HALTED state
- CPU runs at 16MHz - reading during execution gives inconsistent data
- **Pattern:** `halt_execution` → inspect → `continue_execution`
- **Works anytime:** `get_screenshot`, `add_breakpoint`, `halt_execution`

### 3. I/O Register Visibility
**COMMON MISTAKE:** BG control registers are private
- `bg0_cnt`, `bg1_cnt`, `bg3_cnt` must be `pub` in `src/system/memory.rs`
- Only `bg2_cnt` was public initially
- PPU needs access to all BG control registers for proper rendering

## MCP Tools Quick Reference

**Server:** `http://127.0.0.1:3000/mcp` (reconnect: `/mcp` command)

### Works Anytime
- `continue_execution` - Resume from halt
- `halt_execution` - **NEW** Pause immediately (works while running!)
- `get_screenshot` - Capture screen
- `add_breakpoint(address)` - Set breakpoint
- `remove_breakpoint(address)` - Clear breakpoint
- `list_breakpoints` - Show all breakpoints

### Requires HALTED State
- `step(count=1)` - Step N instructions
- `get_cpu_state` - All registers + CPSR
- `read_register(register)` - Single register (0-15, pc, sp, lr)
- `read_memory(address, length)` - Raw memory bytes
- `disassemble(address, count=10)` - Disassemble code
- `read_palette` - Palette RAM

## File Map (Where to Look)

| Component | File | Key Functions |
|-----------|------|---------------|
| Main loop | `src/main.rs:80-115` | V-COUNT update, cpu.cycle(), PPU rendering |
| MCP command defs | `src/mcp.rs` | `DebugCommand` enum, tool definitions |
| MCP execution | `src/debugger.rs:execute_command()` | Command handlers, breakpoint logic |
| PPU rendering | `src/system/ppu.rs:draw_scanline()` | **BUG LOCATION** - BG layer selection |
| I/O registers | `src/system/memory.rs:136-180` | `IoRegisters` struct - check `pub` visibility |
| CPU emulation | `src/system/cpu.rs` | ARM/THUMB execution |
| Memory map | `src/system/memory.rs` | Address decoding |

## Architecture Quick Reference

### Memory Map
```
0x00000000  BIOS (16KB)
0x02000000  WRAM (256KB)
0x03000000  WRAM on-chip (32KB)
0x04000000  I/O Registers (DISPCNT, VCOUNT, BGxCNT, etc.)
0x05000000  Palette RAM (1KB)
0x06000000  VRAM (96KB)
0x07000000  OAM (1KB)
0x08000000  Cartridge ROM
```

### Critical I/O Registers
| Address | Register | Purpose | Key Bits |
|---------|----------|---------|----------|
| 0x04000000 | DISPCNT | Display control | Bits 8-11: BG0-BG3 enable |
| 0x04000006 | VCOUNT | Current scanline | 0-227 |
| 0x04000008 | BG0CNT | BG0 control | Bits 2-3: char base, 8-12: screen base |
| 0x0400000A | BG1CNT | BG1 control | Same as BG0CNT |
| 0x0400000C | BG2CNT | BG2 control | Same as BG0CNT |
| 0x0400000E | BG3CNT | BG3 control | Same as BG0CNT |

### Timing
- CPU: +2 cycles per instruction
- Scanline: Every 1232 cycles
- Display: Lines 0-159 visible, 160-227 V-blank

## Debugging Workflow

### Standard Pattern
```
1. continue_execution          # Run
2. (wait for issue or breakpoint)
3. halt_execution              # Pause
4. get_cpu_state               # Check PC/registers
5. read_memory / disassemble   # Inspect
6. step / continue_execution   # Resume
```

### Key Debugging Facts
- Debug output goes to **stderr**
- Breakpoints auto-halt execution
- V-COUNT updates **before** `cpu.cycle()` in main loop
- ARM = 4 bytes/instruction, THUMB = 2 bytes/instruction
- PC is always +8 ahead in ARM mode (+4 in THUMB) due to pipelining

## Quick Fixes Reference

### Make I/O Register Public
```rust
// src/system/memory.rs:140
pub struct IoRegisters {
    pub disp_cnt: u16,
    pub bg0_cnt: u16,  // ← Add pub
    pub bg1_cnt: u16,  // ← Add pub
    pub bg2_cnt: u16,
    pub bg3_cnt: u16,  // ← Add pub
    // ...
}
```

### Add Conditional Debug Logging
```rust
// In main.rs main loop
if cpu.get_cycles() % 100000 == 0 {
    eprintln!("PC: {:#010X} at cycle {}", cpu.get_r(15), cpu.get_cycles());
}
```

### Check DISPCNT Changes
```rust
// In memory.rs I/O write handler
if offset == 0x00 {  // DISPCNT
    let old = self.io_registers.disp_cnt;
    self.io_registers.disp_cnt = value;
    if old != value {
        eprintln!("DISPCNT changed to: {:#06X}", value);
    }
}
```

## PPU Implementation (Common Bug Area!)

### BIOS Logo Rendering - What Actually Happens
1. BIOS decompresses logo from ROM header bytes 0x04-0x9F (Huffman compressed)
2. BIOS writes tile data to VRAM starting at 0x06000000
3. BIOS sets up **BG0** (not BG2!) with tilemap
4. BIOS sets `DISPCNT = 0x0100` → **BG0 enabled** (bit 8 = 1)
5. BIOS sets palette to grayscale gradient (0x0000, 0x0421, 0x0842, etc.)

### Expected Debug Output
```
DISPCNT changed to: 0x0100 (Mode: 0, ForcedBlank: 0, BG0: 1, BG1: 0, BG2: 0, BG3: 0)
Palette[0-7]: 0x0000 0x0421 0x0842 0x0C63 0x1084 0x18C6 0x1CE7 0x2108
```

### PPU Rendering Logic (src/system/ppu.rs)
```rust
// CORRECT implementation:
let bg0_enabled = (disp_cnt & (1 << 8)) != 0;
let bg1_enabled = (disp_cnt & (1 << 9)) != 0;
let bg2_enabled = (disp_cnt & (1 << 10)) != 0;
let bg3_enabled = (disp_cnt & (1 << 11)) != 0;

if mode == 0 {
    if bg0_enabled {
        self.draw_mode0_bg_scanline(y, &mut fb, mem, 0);  // BG number!
    } else if bg1_enabled { /* ... */ }
    // etc.
}
```

**WRONG (common bug):**
```rust
// Hardcoded to BG2 - logo won't display!
if mode == 0 {
    self.draw_mode0_bg2_scanline(y, &mut fb, mem);  // Always BG2!
}
```

## Troubleshooting Checklist

### Logo Not Displaying
```
☐ Check DISPCNT: read_memory 0x04000000 2
  → Should show 0x0100 (BG0 enabled)
☐ Check PPU draws correct BG layer
  → src/system/ppu.rs:draw_scanline() - must check DISPCNT bits 8-11
☐ Verify BG0CNT is public
  → src/system/memory.rs:140 - must be `pub bg0_cnt: u16`
☐ Check palette loaded
  → Should see grayscale: 0x0000, 0x0421, 0x0842, 0x0C63...
☐ Check VRAM has tile data
  → read_memory 0x06000000 256 - should NOT be all zeros after BIOS runs
```

### MCP "Channel Closed" Error
```
☐ Emulator is RUNNING when inspection tool called
  → halt_execution first!
☐ Too many commands sent in parallel
  → Space out commands, wait for responses
☐ Emulator crashed/panicked
  → Check stderr for panic messages
```

### Adding New MCP Command
```
1. Add to DebugCommand enum (mcp.rs)
2. Add to DebugResponse enum (mcp.rs)
3. Add tool definition in handle_tools_list() (mcp.rs)
4. Add handler in handle_tool_call() (mcp.rs)
5. Add execute_foo() function (mcp.rs)
6. Add case in execute_command() (debugger.rs)
```
