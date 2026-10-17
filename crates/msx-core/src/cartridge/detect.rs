//! Independent content heuristics, not ROM identity or hardware compatibility.
//! The algorithm, score thresholds and deliberate limits are in docs/mapper-detection.md.
use super::{Mapper, Mapping};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Confidence {
    Invalid = 0,
    Insufficient = 1,
    Ambiguous = 2,
    PlainFormat = 3,
    Likely = 4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub mapper: Mapper,
    pub score: i32,
    pub register_windows: u32,
    pub banked_transfer: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    pub mapper: Option<Mapper>,
    pub confidence: Confidence,
    /// Validated MegaROM candidates, descending score; ties retain ABI order.
    pub candidates: Vec<Candidate>,
    pub explanation: String,
}

/// Inspect local bytes without modifying a machine, allocating a ROM copy or using a database.
pub fn detect_mapper(rom: &[u8]) -> Detection {
    let plain = Mapping::validate_explicit(rom, Mapper::Plain).ok();
    let mut candidates = Vec::new();
    let mut writes = false;
    let mut truncated = false;
    for mapper in [
        Mapper::Ascii8,
        Mapper::Ascii16,
        Mapper::Konami,
        Mapper::KonamiScc,
    ] {
        if let Ok(mapping) = Mapping::validate_explicit(rom, mapper) {
            let evidence = trace(rom, mapping);
            writes |= evidence.writes;
            truncated |= evidence.candidate.truncated;
            candidates.push(evidence.candidate);
        }
    }
    // A 48 KiB plain image cannot pass the power-of-two MegaROM rules.
    if let Some(mapping) = plain.as_ref() {
        let evidence = trace(rom, mapping.clone());
        writes |= evidence.writes;
        truncated |= evidence.candidate.truncated;
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.score));
    let best = candidates.first();
    let confident = best.is_some_and(|c| {
        !truncated
            && c.score >= 6
            && (c.register_windows >= 2 || c.banked_transfer)
            && candidates
                .get(1)
                .is_none_or(|next| c.score - next.score >= 4)
    });
    let (mapper, confidence, explanation) = if confident {
        let c = best.unwrap();
        (Some(c.mapper), Confidence::Likely, format!(
            "Auto: {} (estimate; score {}, {} register windows{}). Mapper inference does not verify BIOS, machine model or audio compatibility.",
            c.mapper.name(), c.score, c.register_windows,
            if c.banked_transfer { "; supported banked code transfer" } else { "" }
        ))
    } else if plain.is_some() && !writes {
        (
            Some(Mapper::Plain),
            Confidence::PlainFormat,
            "Auto: Plain (recognized size and AB header; software compatibility is unverified)."
                .into(),
        )
    } else {
        let confidence = if candidates.is_empty() && plain.is_none() {
            Confidence::Invalid
        } else if !truncated && best.is_some_and(|c| c.score >= 6) {
            Confidence::Ambiguous
        } else {
            Confidence::Insufficient
        };
        let hints = candidates
            .iter()
            .map(|c| format!("{} {}", c.mapper.name(), c.score))
            .collect::<Vec<_>>()
            .join(", ");
        let reason = match confidence {
            _ if truncated => "Insufficient mapper evidence: code analysis limit reached",
            Confidence::Invalid => "Invalid or unsupported ROM format: check size and AB header",
            Confidence::Ambiguous => "Ambiguous mapper evidence",
            _ => "Insufficient mapper evidence",
        };
        (None, confidence, format!("{reason}{}. Select its documented mapper manually in ROMs & advanced system options, then select the same file again.",
            if hints.is_empty() { String::new() } else { format!(" (scores: {hints})") }))
    };
    Detection {
        mapper,
        confidence,
        candidates,
        explanation,
    }
}

struct Evidence {
    candidate: Candidate,
    writes: bool,
}

#[derive(Clone)]
struct Path {
    pc: u16,
    mapping: Mapping,
    a: Option<u8>,
    /// Meaningful instructions since a direct transfer into a known switched bank.
    transferred: Option<u8>,
}

fn register(kind: Mapper, address: u16) -> Option<(usize, u16)> {
    let a = address as usize;
    match kind {
        Mapper::Ascii8 if (0x6000..0x8000).contains(&a) => {
            Some(((a - 0x6000) / 0x800, address & 0xf800))
        }
        Mapper::Ascii16 if (0x6000..0x6800).contains(&a) => Some((0, 0x6000)),
        Mapper::Ascii16 if (0x7000..0x7800).contains(&a) => Some((1, 0x7000)),
        Mapper::Konami if (0x6000..0xc000).contains(&a) => {
            Some(((a - 0x6000) / 0x2000, address & 0xe000))
        }
        Mapper::KonamiScc if (0x5000..0xb800).contains(&a) && a & 0x1800 == 0x1000 => {
            Some(((a - 0x5000) / 0x2000, address & 0xf800))
        }
        _ => None,
    }
}

fn trace(rom: &[u8], initial: Mapping) -> Evidence {
    let mut pending = Vec::new();
    for header in [0x4000u16, 0x8000] {
        if initial.read(rom, header) == b'A' && initial.read(rom, header + 1) == b'B' {
            for offset in [2, 4, 6, 8] {
                let pc = u16::from_le_bytes([
                    initial.read(rom, header + offset),
                    initial.read(rom, header + offset + 1),
                ]);
                if (0x4000..0xc000).contains(&pc) {
                    pending.push(Path {
                        pc,
                        mapping: initial.clone(),
                        a: None,
                        transferred: None,
                    });
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut windows = [0; 4];
    let mut unsupported = BTreeSet::new();
    let mut writes = false;
    let mut banked_transfer = false;
    let mut truncated = false;
    // Bounds are independent of file size. No speculative path can hang detection.
    for _ in 0..8192 {
        let Some(mut path) = pending.pop() else { break };
        if !(0x4000..0xc000).contains(&path.pc) {
            continue;
        }
        let terminal = |path: &Path| path.transferred.is_some_and(|n| n >= 4);
        if !seen.insert((path.pc, path.mapping.banks, path.a)) {
            banked_transfer |= terminal(&path);
            continue;
        }
        let read = |offset: u16| path.mapping.read(rom, path.pc.wrapping_add(offset));
        let op = read(0);
        // Padding is not evidence of code. A long NOP run can hide a data table.
        if (op == 0 || op == 0xff) && (0..16).all(|i| read(i) == op) {
            continue;
        }
        let word = u16::from_le_bytes([read(1), read(2)]);
        let length = instruction_length(op, read(1));
        if path.pc as usize + length > 0xc000 {
            continue;
        }
        let next = path.pc + length as u16;
        if op != 0 && op != 0xff {
            path.transferred = path.transferred.map(|n| (n + 1).min(4));
        }
        if op == 0x32 && (0x4000..0xc000).contains(&word) {
            writes = true;
            if let Some((index, start)) = register(initial.kind, word) {
                windows[index] = windows[index].max(if word == start { 3 } else { 2 });
                if let Some(a) = path.a {
                    path.mapping.write(word, a);
                } else {
                    // The subsequent byte layout is unknown; keep the evidence already found.
                    continue;
                }
            } else {
                unsupported.insert(word / 0x800);
            }
        }
        path.a = match op {
            0x3e => Some(path.mapping.read(rom, path.pc + 1)),
            0x00 | 0x32 | 0xd3 | 0xf3 | 0xfb | 0xc3 | 0x18 => path.a,
            _ => None,
        };
        // Stop at returns, HALT, indirect transfers, RST and repeated index prefixes.
        if op == 0xc9
            || op == 0x76
            || op == 0xe9
            || op & 0xc7 == 0xc7
            || (op == 0xed && read_ed_return(&path.mapping, rom, path.pc))
            || ([0xdd, 0xfd].contains(&op)
                && [0xdd, 0xfd, 0xed, 0xe9].contains(&path.mapping.read(rom, path.pc + 1)))
        {
            if op == 0xc9 || op == 0x76 || op == 0xed {
                banked_transfer |= terminal(&path);
            }
            continue;
        }
        let absolute = op == 0xc3 || op == 0xcd || op & 0xc7 == 0xc2 || op & 0xc7 == 0xc4;
        let relative = op == 0x10 || op == 0x18 || op & 0xe7 == 0x20;
        if absolute || relative {
            let target = if absolute {
                word
            } else {
                next.wrapping_add_signed(path.mapping.read(rom, path.pc + 1) as i8 as i16)
            };
            let mut branch = path.clone();
            branch.pc = target;
            // Compare physical bank selections, not byte equality (banks may contain identical data).
            if (0x4000..0xc000).contains(&target) {
                let index = (target as usize - 0x4000)
                    / if initial.kind == Mapper::Ascii16 {
                        0x4000
                    } else {
                        0x2000
                    };
                let bank_count = rom.len()
                    / if initial.kind == Mapper::Ascii16 {
                        0x4000
                    } else {
                        0x2000
                    };
                if branch.transferred.is_none()
                    && branch.mapping.banks[index] as usize % bank_count
                        != initial.banks[index] as usize % bank_count
                {
                    branch.transferred = Some(0);
                }
            }
            if pending.len() < 64 {
                pending.push(branch);
            } else {
                truncated = true;
            }
            if op == 0xc3 || op == 0x18 {
                continue;
            }
            // A call may change any register or mapper bank; do not invent a return state.
            if op == 0xcd || op & 0xc7 == 0xc4 {
                continue;
            }
        }
        path.pc = next;
        if pending.len() < 64 {
            pending.push(path);
        } else {
            truncated = true;
        }
    }
    let register_windows = windows.iter().filter(|&&n| n != 0).count() as u32;
    Evidence {
        candidate: Candidate {
            mapper: initial.kind,
            score: windows.iter().sum::<i32>() - 3 * unsupported.len() as i32
                + if banked_transfer { 6 } else { 0 },
            register_windows,
            banked_transfer,
            truncated: truncated || !pending.is_empty(),
        },
        writes,
    }
}

fn read_ed_return(mapping: &Mapping, rom: &[u8], pc: u16) -> bool {
    mapping.read(rom, pc + 1) & 0xc7 == 0x45
}

// Z80 length decoding by opcode fields. Prefix operands are skipped as a unit,
// so an immediate byte or indexed displacement cannot become a spurious LD (nn),A.
fn instruction_length(op: u8, second: u8) -> usize {
    if op == 0xcb {
        return 2;
    }
    if op == 0xed {
        return if second & 0xc7 == 0x43 { 4 } else { 2 };
    }
    if op == 0xdd || op == 0xfd {
        if [0xdd, 0xfd].contains(&second) {
            return 2;
        }
        if second == 0xcb {
            return 4;
        }
        if second == 0xed {
            return 2;
        } // Unsupported prefix combination: stopped below.
        let memory =
            (second >> 6 == 1 && second != 0x76 && (second & 7 == 6 || second >> 3 & 7 == 6))
                || (second >> 6 == 2 && second & 7 == 6)
                || (second >> 6 == 0 && second >> 3 & 7 == 6 && [4, 5, 6].contains(&(second & 7)));
        return 1 + instruction_length(second, 0) + usize::from(memory);
    }
    let x = op >> 6;
    let y = op >> 3 & 7;
    let z = op & 7;
    match (x, z) {
        (0, 0) if y >= 2 => 2,
        (0, 1) if y & 1 == 0 => 3,
        (0, 2) if y >= 4 => 3,
        (0, 6) | (3, 6) => 2,
        (3, 2) | (3, 4) => 3,
        (3, 3) if y == 0 => 3,
        (3, 3) if y == 2 || y == 3 => 2,
        (3, 5) if y == 1 => 3,
        _ => 1,
    }
}
