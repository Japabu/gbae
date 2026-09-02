use gbae::system::gba::Gba;
use gbae::system::instructions::{format_instruction_arm, format_instruction_thumb};
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::panic;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: trace <bios> <rom> [max_steps] [watch=<hex>] [break=<hex>] [break_cpsr=<mask>:<value>] [pc_min=<hex>]");
        std::process::exit(1);
    }
    let bios = fs::read(&args[1]).expect("Failed to read BIOS");
    let rom = fs::read(&args[2]).expect("Failed to read ROM");
    let mut max_steps = u64::MAX;
    let mut watch: Option<u32> = None;
    let mut break_pc: Option<u32> = None;
    let mut break_cpsr: Option<(u32, u32)> = None;
    let mut pc_min = 0u32;
    let mut profile: Option<(u64, u64)> = None;
    let mut profile_counts: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    for arg in &args[3..] {
        if let Some(value) = arg.strip_prefix("watch=") {
            watch = u32::from_str_radix(value.trim_start_matches("0x"), 16).ok();
        } else if let Some(value) = arg.strip_prefix("break=") {
            break_pc = u32::from_str_radix(value.trim_start_matches("0x"), 16).ok();
        } else if let Some(value) = arg.strip_prefix("break_cpsr=") {
            let (mask, expected) = value.split_once(':').expect("break_cpsr needs mask:value");
            break_cpsr = Some((u32::from_str_radix(mask, 16).unwrap(), u32::from_str_radix(expected, 16).unwrap()));
        } else if let Some(value) = arg.strip_prefix("profile=") {
            let (start, end) = value.split_once(':').expect("profile needs start:end");
            profile = Some((start.parse().unwrap(), end.parse().unwrap()));
        } else if let Some(value) = arg.strip_prefix("pc_min=") {
            pc_min = u32::from_str_radix(value.trim_start_matches("0x"), 16).unwrap();
        } else {
            max_steps = arg.parse().expect("Failed to parse step count");
        }
    }

    let mut gba = Gba::new(bios, rom);
    let mut history: VecDeque<(u32, bool, u32, [u32; 16], u32)> = VecDeque::new();
    let mut steps = 0u64;
    let mut watched_value = watch.map_or(0, |address| gba.mem.read_u32(address));

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        while steps < max_steps {
            let pc = gba.cpu.get_r(15);
            if let Some((start, end)) = profile {
                if steps >= start && steps < end {
                    *profile_counts.entry(pc).or_insert(0) += 1;
                }
                if steps == end {
                    break;
                }
            }
            if steps > 0 && break_pc == Some(pc) {
                println!("break at {:08X} after {} steps", pc, steps);
                break;
            }
            if let Some((mask, expected)) = break_cpsr {
                if pc >= pc_min && gba.cpu.get_cpsr() & mask == expected {
                    println!("cpsr break at {:08X} after {} steps, cpsr {:08X}", pc, steps, gba.cpu.get_cpsr());
                    break;
                }
            }
            let thumb = gba.cpu.get_thumb_state();
            let word = if thumb { gba.mem.read_u16(pc) as u32 | (gba.mem.read_u16(pc + 2) as u32) << 16 } else { gba.mem.read_u32(pc) };
            let mut registers = [0u32; 16];
            for i in 0..16 {
                registers[i] = gba.cpu.get_r(i as u8);
            }
            if history.len() == 40 {
                history.pop_front();
            }
            history.push_back((pc, thumb, word, registers, gba.cpu.get_cpsr()));
            gba.step();
            steps += 1;
            if let Some(address) = watch {
                let value = gba.mem.read_u32(address);
                if value != watched_value {
                    println!("step {} pc {:08X} wrote {:08X}: {:08X} -> {:08X}", steps, pc, address, watched_value, value);
                    watched_value = value;
                }
            }
        }
    }));

    println!("steps executed: {}", steps);
    if profile.is_some() {
        let mut counts: Vec<(u32, u64)> = profile_counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        println!("hottest addresses:");
        for (pc, count) in counts.iter().take(60) {
            let thumb = pc & 1 == 1 || gba.cpu.get_thumb_state();
            let text = if thumb { format_instruction_thumb(gba.mem.read_u16(*pc), gba.mem.read_u16(pc + 2), *pc) } else { format_instruction_arm(gba.mem.read_u32(*pc), *pc) };
            println!("  {:08X} {:>9} {}", pc, count, text.lines().next().unwrap_or(""));
        }
        println!("distinct addresses: {}", counts.len());
    }
    println!("last instructions (oldest first):");
    for (pc, thumb, word, _, _) in history.iter() {
        let text = if *thumb { format_instruction_thumb(*word as u16, (*word >> 16) as u16, *pc) } else { format_instruction_arm(*word, *pc) };
        println!("  {:08X} {} {}", pc, if *thumb { "T" } else { "A" }, text.lines().next().unwrap_or(""));
    }
    if let Some((_, _, _, registers, cpsr)) = history.back() {
        println!("registers before last instruction:");
        for (i, value) in registers.iter().enumerate() {
            println!("  r{:<2} = {:08X}", i, value);
        }
        println!("  cpsr = {:08X}", cpsr);
    }
    if result.is_err() {
        std::process::exit(1);
    }
}
