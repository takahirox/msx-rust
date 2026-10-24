//! Original sequence vectors, derived from the behavioral references in docs/cpu-wz.md.
use msx_core::cpu::{Bus, Cpu, FetchWait};

const POLICIES: [FetchWait; 2] = [FetchWait::None, FetchWait::Msx];

struct Memory {
    bytes: Vec<u8>,
    input: u8,
    ports: Vec<(u16, u8)>,
    irq: bool,
    vector: u8,
}
impl Bus for Memory {
    fn read(&mut self, a: u16) -> u8 {
        self.bytes[usize::from(a)]
    }
    fn write(&mut self, a: u16, v: u8) {
        self.bytes[usize::from(a)] = v;
    }
    fn input(&mut self, p: u16) -> u8 {
        self.ports.push((p, self.input));
        self.input
    }
    fn output(&mut self, p: u16, v: u8) {
        self.ports.push((p, v));
    }
    fn irq(&self) -> bool {
        self.irq
    }
    fn interrupt_data(&self) -> u8 {
        self.vector
    }
}
fn fixture(code: &[u8], policy: FetchWait) -> (Cpu, Memory) {
    let mut cpu = Cpu::with_fetch_wait(policy);
    cpu.pc = 0x8000;
    cpu.sp = 0xf000;
    let mut bus = Memory {
        bytes: vec![0; 65536],
        input: 0,
        ports: vec![],
        irq: false,
        vector: 0xff,
    };
    bus.bytes[0x8000..0x8000 + code.len()].copy_from_slice(code);
    (cpu, bus)
}
fn step(cpu: &mut Cpu, bus: &mut Memory, cycles: u32, m1: u32, policy: FetchWait) {
    let expected = cycles + if policy == FetchWait::Msx { m1 } else { 0 };
    let before = cpu.tstates;
    assert_eq!(cpu.step(bus), expected);
    assert_eq!(cpu.tstates - before, u64::from(expected));
}

#[test]
fn issue_73_reproduction() {
    for policy in POLICIES {
        let (mut c, mut m) = fixture(&[0x3a, 0xff, 0x27, 0x21, 0, 0x40, 0xcb, 0x46], policy);
        c.f = 1;
        step(&mut c, &mut m, 13, 1, policy);
        step(&mut c, &mut m, 10, 1, policy);
        step(&mut c, &mut m, 12, 2, policy);
        assert_eq!(c.f, 0x7d);
    }
}

#[test]
fn every_bit_uses_wz_for_hl_and_operand_for_registers() {
    for policy in POLICIES {
        for carry in [0, 1] {
            // WZ high bits oppose both HL and the memory operand. Test set/clear,
            // S only for BIT 7, and preservation of carry for all eight bits.
            for (address, operand, clear, set, sign) in [
                (0x27ff, 0, 0x7c, 0x38, 0xb8),
                (0x0000, 0xff, 0x54, 0x10, 0x90),
            ] {
                for bit in 0..8 {
                    let (mut c, mut m) = fixture(
                        &[
                            0x3a,
                            address as u8,
                            (address >> 8) as u8,
                            0xcb,
                            0x46 + bit * 8,
                        ],
                        policy,
                    );
                    c.set_hl(if operand == 0 { 0x4000 } else { 0x6800 });
                    m.bytes[usize::from(c.hl())] = operand;
                    c.f = carry;
                    step(&mut c, &mut m, 13, 1, policy);
                    step(&mut c, &mut m, 12, 2, policy);
                    let expected = if operand == 0 {
                        clear
                    } else if bit == 7 {
                        sign
                    } else {
                        set
                    };
                    assert_eq!(c.f, expected | carry, "BIT {bit}, operand {operand:02x}");
                    // BIT b,B keeps the operand's 3/5, even with opposite WZ.
                    m.bytes[usize::from(c.pc)] = 0xcb;
                    m.bytes[usize::from(c.pc) + 1] = 0x40 + bit * 8;
                    c.r[0] = if operand == 0 { 0xff } else { 0 };
                    step(&mut c, &mut m, 8, 2, policy);
                    let flags = if c.r[0] == 0 {
                        0x54
                    } else if bit == 7 {
                        0xb8
                    } else {
                        0x38
                    };
                    assert_eq!(c.f, flags | carry);
                }
            }
        }
    }
}

// Probe via ordinary instructions after the instruction under test, keeping the
// complete latch assertion as a supplement to the observable flag assertion.
fn probe(c: &mut Cpu, m: &mut Memory, expected: u16, policy: FetchWait) {
    assert_eq!(c.wz, expected);
    c.pc = 0xa000;
    m.bytes[0xa000..0xa005].copy_from_slice(&[0x21, 0, 0x60, 0xcb, 0x46]);
    m.bytes[0x6000] = 0;
    let carry = c.f & 1;
    step(c, m, 10, 1, policy);
    step(c, m, 12, 2, policy);
    assert_eq!(c.f, 0x54 | carry | ((expected >> 8) as u8 & 0x28));
    assert_eq!(c.wz, expected);
}

#[test]
fn loads_arithmetic_exchange_and_nibble_families() {
    // Setup: BC=27FF, DE=FFFF, HL=07FF, IX=1FFF, IY=FFFF,
    // A=28, carry set, old WZ=1234, stack word=2800.
    let vectors: &[(&[u8], u16, u32, u32)] = &[
        (&[0x0a], 0x2800, 7, 1),
        (&[0x1a], 0x0000, 7, 1),
        (&[0x3a, 0xff, 0xff], 0x0000, 13, 1),
        (&[0x02], 0x2800, 7, 1),
        (&[0x12], 0x2800, 7, 1),
        (&[0x32, 0xff, 0xff], 0x2800, 13, 1),
        (&[0x22, 0xff, 0x27], 0x2800, 16, 1),
        (&[0x2a, 0xff, 0xff], 0x0000, 16, 1),
        (&[0xdd, 0x22, 0xff, 0xff], 0x0000, 20, 2),
        (&[0xfd, 0x2a, 0xff, 0x27], 0x2800, 20, 2),
        (&[0xed, 0x43, 0xff, 0xff], 0x0000, 20, 2),
        (&[0xed, 0x4b, 0xff, 0x27], 0x2800, 20, 2),
        (&[0xed, 0x53, 0xff, 0x27], 0x2800, 20, 2),
        (&[0xed, 0x5b, 0xff, 0xff], 0x0000, 20, 2),
        (&[0xed, 0x63, 0xff, 0xff], 0x0000, 20, 2),
        (&[0xed, 0x6b, 0xff, 0x27], 0x2800, 20, 2),
        (&[0xed, 0x73, 0xff, 0x27], 0x2800, 20, 2),
        (&[0xed, 0x7b, 0xff, 0xff], 0x0000, 20, 2),
        (&[0x09], 0x0800, 11, 1),
        (&[0x19], 0x0800, 11, 1),
        (&[0x29], 0x0800, 11, 1),
        (&[0x39], 0x0800, 11, 1),
        (&[0xdd, 0x09], 0x2000, 15, 2),
        (&[0xfd, 0x29], 0x0000, 15, 2),
        (&[0xed, 0x4a], 0x0800, 15, 2),
        (&[0xed, 0x5a], 0x0800, 15, 2),
        (&[0xed, 0x6a], 0x0800, 15, 2),
        (&[0xed, 0x7a], 0x0800, 15, 2),
        (&[0xed, 0x42], 0x0800, 15, 2),
        (&[0xed, 0x52], 0x0800, 15, 2),
        (&[0xed, 0x62], 0x0800, 15, 2),
        (&[0xed, 0x72], 0x0800, 15, 2),
        (&[0xe3], 0x2800, 19, 1),
        (&[0xdd, 0xe3], 0x2800, 23, 2),
        (&[0xfd, 0xe3], 0x2800, 23, 2),
        (&[0xed, 0x67], 0x0800, 18, 2),
        (&[0xed, 0x6f], 0x0800, 18, 2),
    ];
    for policy in POLICIES {
        for &(code, wz, cycles, m1) in vectors {
            let (mut c, mut m) = fixture(code, policy);
            c.set_bc(0x27ff);
            c.set_de(0xffff);
            c.set_hl(0x07ff);
            c.ix = 0x1fff;
            c.iy = 0xffff;
            c.set_a(0x28);
            c.f = 1;
            c.wz = 0x1234;
            m.bytes[0xf001] = 0x28;
            step(&mut c, &mut m, cycles, m1, policy);
            assert_eq!(c.wz, wz, "{code:02x?}");
            probe(&mut c, &mut m, wz, policy);
        }
    }
}

#[test]
fn io_uses_original_accumulator_and_bc_even_when_destination_overwrites_them() {
    for policy in POLICIES {
        for (code, wz, cycles, m1, port) in [
            (vec![0xdb, 0xff], 0x2800, 11, 1, 0x27ff),
            (vec![0xd3, 0xff], 0x2700, 11, 1, 0x27ff),
            (vec![0xed, 0x40], 0x2800, 12, 2, 0x27ff),
            (vec![0xed, 0x48], 0x2800, 12, 2, 0x27ff),
            (vec![0xed, 0x70], 0x2800, 12, 2, 0x27ff),
            (vec![0xed, 0x78], 0x2800, 12, 2, 0x27ff),
            (vec![0xed, 0x41], 0x2800, 12, 2, 0x27ff),
            (vec![0xed, 0x71], 0x2800, 12, 2, 0x27ff),
            (vec![0xed, 0x79], 0x2800, 12, 2, 0x27ff),
        ] {
            let (mut c, mut m) = fixture(&code, policy);
            c.set_bc(0x27ff);
            c.set_a(0x27);
            m.input = 0xee;
            step(&mut c, &mut m, cycles, m1, policy);
            assert_eq!(m.ports[0].0, port);
            probe(&mut c, &mut m, wz, policy);
        }
        let (mut c, mut m) = fixture(&[0xed, 0x40], policy);
        c.set_bc(0xffff);
        step(&mut c, &mut m, 12, 2, policy);
        probe(&mut c, &mut m, 0, policy);
    }
}

#[test]
fn indexed_memory_families_and_bit_displacement_boundaries() {
    for policy in POLICIES {
        for prefix in [0xdd, 0xfd] {
            for (index, displacement, address, flags) in [
                (0x2781, 0x7f, 0x2800, 0x7d),
                (0x2880, 0x80, 0x2800, 0x7d),
                (0x0000, 0x80, 0xff80, 0x7d),
                (0xffff, 0x01, 0x0000, 0x55),
                (0x2800, 0xff, 0x27ff, 0x75),
            ] {
                for bit in 0..8 {
                    let (mut c, mut m) =
                        fixture(&[prefix, 0xcb, displacement, 0x46 + bit * 8], policy);
                    c.ix = index;
                    c.iy = index;
                    c.f = 1;
                    c.wz = 0x5000;
                    step(&mut c, &mut m, 20, 2, policy);
                    assert_eq!((c.wz, c.f), (address, flags));
                }
                for (tail, cycles) in [
                    (vec![0x46, displacement], 19), // LD B,(index+d)
                    (vec![0x74, displacement], 19), // LD (index+d),H
                    (vec![0x36, displacement, 0], 19),
                    (vec![0x86, displacement], 19),
                    (vec![0x34, displacement], 23),
                    (vec![0x35, displacement], 23),
                    (vec![0xcb, displacement, 0x00], 23), // RLC, also copy to B
                    (vec![0xcb, displacement, 0x86], 23),
                    (vec![0xcb, displacement, 0xc6], 23),
                ] {
                    let mut code = vec![prefix];
                    code.extend(tail);
                    let (mut c, mut m) = fixture(&code, policy);
                    c.ix = index;
                    c.iy = index;
                    step(&mut c, &mut m, cycles, 2, policy);
                    probe(&mut c, &mut m, address, policy);
                }
            }
        }
    }
}

#[test]
fn conditional_control_flow_and_indirect_jumps() {
    for policy in POLICIES {
        for condition in 0..8 {
            for taken in [false, true] {
                // NZ,Z,NC,C,PO,PE,P,M with literal false/true flag inputs.
                let (no, yes) = [
                    (0x40, 0),
                    (0, 0x40),
                    (1, 0),
                    (0, 1),
                    (4, 0),
                    (0, 4),
                    (0x80, 0),
                    (0, 0x80),
                ][condition];
                for kind in [0xc2, 0xc4, 0xc0] {
                    // JP, CALL, RET
                    let code = [kind + condition as u8 * 8, 0, 0x28];
                    let (mut c, mut m) = fixture(&code, policy);
                    c.wz = 0x68ff;
                    c.f = if taken { yes } else { no };
                    m.bytes[0xf001] = 0x28;
                    let (cycles, pc, wz) = match (kind, taken) {
                        (0xc2, true) => (10, 0x2800, 0x2800),
                        (0xc2, false) => (10, 0x8003, 0x2800),
                        (0xc4, true) => (17, 0x2800, 0x2800),
                        (0xc4, false) => (10, 0x8003, 0x2800),
                        (_, true) => (11, 0x2800, 0x2800),
                        (_, false) => (5, 0x8001, 0x68ff),
                    };
                    step(&mut c, &mut m, cycles, 1, policy);
                    assert_eq!(c.pc, pc);
                    probe(&mut c, &mut m, wz, policy);
                }
            }
        }
        for (code, flags, b, cycles, pc, wz) in [
            (vec![0x18, 0xfc], 0, 0, 12, 0x7ffe, 0x7ffe),
            (vec![0x10, 0x7f], 0, 2, 13, 0x8081, 0x8081),
            (vec![0x10, 0x7f], 0, 1, 8, 0x8002, 0x68ff),
            (vec![0x20, 0x80], 0, 0, 12, 0x7f82, 0x7f82),
            (vec![0x20, 0x80], 0x40, 0, 7, 0x8002, 0x68ff),
            (vec![0x28, 0x80], 0x40, 0, 12, 0x7f82, 0x7f82),
            (vec![0x28, 0x80], 0, 0, 7, 0x8002, 0x68ff),
            (vec![0x30, 0x80], 0, 0, 12, 0x7f82, 0x7f82),
            (vec![0x30, 0x80], 1, 0, 7, 0x8002, 0x68ff),
            (vec![0x38, 0x80], 1, 0, 12, 0x7f82, 0x7f82),
            (vec![0x38, 0x80], 0, 0, 7, 0x8002, 0x68ff),
            (vec![0xc3, 0, 0x28], 0, 0, 10, 0x2800, 0x2800),
            (vec![0xcd, 0, 0x28], 0, 0, 17, 0x2800, 0x2800),
            (vec![0xc9], 0, 0, 10, 0x2800, 0x2800),
            (vec![0xed, 0x45], 0, 0, 14, 0x2800, 0x2800),
            (vec![0xed, 0x4d], 0, 0, 14, 0x2800, 0x2800),
            (vec![0xe9], 0, 0, 4, 0x4000, 0x68ff),
            (vec![0xdd, 0xe9], 0, 0, 8, 0x5000, 0x68ff),
            (vec![0xfd, 0xe9], 0, 0, 8, 0x6000, 0x68ff),
        ] {
            let (mut c, mut m) = fixture(&code, policy);
            c.wz = 0x68ff;
            c.f = flags;
            c.r[0] = b;
            c.set_hl(0x4000);
            c.ix = 0x5000;
            c.iy = 0x6000;
            m.bytes[0xf001] = 0x28;
            step(
                &mut c,
                &mut m,
                cycles,
                if matches!(code[0], 0xdd | 0xfd | 0xed) {
                    2
                } else {
                    1
                },
                policy,
            );
            assert_eq!(c.pc, pc);
            probe(&mut c, &mut m, wz, policy);
        }
        for (op, destination) in [
            (0xc7, 0),
            (0xcf, 8),
            (0xd7, 16),
            (0xdf, 24),
            (0xe7, 32),
            (0xef, 40),
            (0xf7, 48),
            (0xff, 56),
        ] {
            let (mut c, mut m) = fixture(&[op], policy);
            step(&mut c, &mut m, 11, 1, policy);
            assert_eq!(c.pc, destination);
            probe(&mut c, &mut m, destination, policy);
        }
    }
}

#[test]
fn block_repeat_and_final_iterations_including_match_and_prefix_wrap() {
    for policy in POLICIES {
        for (op, bc, a, wz, pc, cycles) in [
            (0xa0, 2, 0, 0x27ff, 0x8002, 16), // LDI/LDD preserve
            (0xa8, 2, 0, 0x27ff, 0x8002, 16),
            (0xb0, 2, 0, 0x8001, 0x8000, 21),
            (0xb8, 2, 0, 0x8001, 0x8000, 21),
            (0xb0, 1, 0, 0x27ff, 0x8002, 16),
            (0xb8, 1, 0, 0x27ff, 0x8002, 16),
            (0xa1, 2, 0, 0x2800, 0x8002, 16), // CPI/CPD adjust old latch
            (0xa9, 2, 0, 0x27fe, 0x8002, 16),
            (0xb1, 2, 1, 0x8001, 0x8000, 21),
            (0xb9, 2, 1, 0x8001, 0x8000, 21),
            (0xb1, 2, 0, 0x2800, 0x8002, 16), // Match, counter not exhausted
            (0xb9, 2, 0, 0x27fe, 0x8002, 16),
            (0xb1, 1, 1, 0x2800, 0x8002, 16), // Counter exhausted, no match
            (0xb9, 1, 1, 0x27fe, 0x8002, 16),
            (0xa2, 0x28ff, 0, 0x2900, 0x8002, 16), // IN uses old BC
            (0xaa, 0x2800, 0, 0x27ff, 0x8002, 16),
            (0xa3, 0x28ff, 0, 0x2800, 0x8002, 16), // OUT uses new BC
            (0xab, 0x2800, 0, 0x26ff, 0x8002, 16),
            (0xb2, 0x28ff, 0, 0x8001, 0x8000, 21),
            (0xba, 0x2800, 0, 0x8001, 0x8000, 21),
            (0xb3, 0x28ff, 0, 0x8001, 0x8000, 21),
            (0xbb, 0x2800, 0, 0x8001, 0x8000, 21),
            (0xb2, 0x01ff, 0, 0x0200, 0x8002, 16),
            (0xba, 0x0100, 0, 0x00ff, 0x8002, 16),
            (0xb3, 0x01ff, 0, 0x0100, 0x8002, 16),
            (0xbb, 0x0100, 0, 0xffff, 0x8002, 16),
        ] {
            let (mut c, mut m) = fixture(&[0xed, op], policy);
            c.set_bc(bc);
            c.set_hl(0x4000);
            c.set_de(0x5000);
            c.set_a(a);
            c.wz = 0x27ff;
            step(&mut c, &mut m, cycles, 2, policy);
            assert_eq!((c.pc, c.wz), (pc, wz), "ED {op:02x}");
            if op & 3 >= 2 {
                let port = if op & 3 == 2 {
                    bc
                } else {
                    bc.wrapping_sub(0x100)
                };
                assert_eq!(m.ports, [(port, 0)]);
            }
            probe(&mut c, &mut m, wz, policy);
        }
        for op in [0xb0, 0xb8, 0xb1, 0xb9, 0xb2, 0xba, 0xb3, 0xbb] {
            // Ignored DD/FD prefixes do not become part of the repeated opcode.
            // Place the ED at FFFFh, so ED-address+1 wraps to zero.
            let (mut c, mut m) = fixture(&[], policy);
            c.pc = 0xfffd;
            m.bytes[0xfffd] = 0xdd;
            m.bytes[0xfffe] = 0xfd;
            m.bytes[0xffff] = 0xed;
            m.bytes[0] = op;
            c.set_bc(0x0202);
            c.set_a(1);
            c.set_hl(0x4000);
            c.set_de(0x5000);
            step(&mut c, &mut m, 29, 4, policy);
            assert_eq!((c.pc, c.wz), (0xffff, 0));
            step(&mut c, &mut m, if op & 3 >= 2 { 16 } else { 21 }, 2, policy);
            let terminal = match op {
                0xb2 => 0x0103,
                0xba => 0x0101,
                0xb3 => 0x0003,
                0xbb => 0x0001,
                _ => 0,
            };
            assert_eq!(c.wz, terminal);
        }
        // Final compare adjusts the previous repeat latch; LDIR leaves it intact.
        for (op, final_wz) in [
            (0xb0, 0x8001),
            (0xb8, 0x8001),
            (0xb1, 0x8002),
            (0xb9, 0x8000),
        ] {
            let (mut c, mut m) = fixture(&[0xed, op], policy);
            c.set_bc(2);
            c.set_a(1);
            c.set_hl(0x4000);
            c.set_de(0x5000);
            step(&mut c, &mut m, 21, 2, policy);
            assert_eq!(c.wz, 0x8001);
            step(&mut c, &mut m, 16, 2, policy);
            probe(&mut c, &mut m, final_wz, policy);
        }
    }
}

#[test]
fn low_byte_carry_and_borrow_are_observable_through_compare_then_bit() {
    for policy in POLICIES {
        for (seed, compare, intermediate, final_wz, flags) in [
            (vec![0x3a, 0xfe, 0x27], 0xa1, 0x27ff, 0x2800, 0x7d),
            (vec![0x3a, 0xff, 0x27], 0xa9, 0x2800, 0x27ff, 0x75),
            (vec![0x32, 0xff, 0x40], 0xa9, 0x2800, 0x27ff, 0x75),
            (vec![0xd3, 0xff], 0xa9, 0x2800, 0x27ff, 0x75),
            (vec![0x3a, 0xff, 0xff], 0xa9, 0x0000, 0xffff, 0x7d),
            (vec![0x3a, 0xfe, 0xff], 0xa1, 0xffff, 0x0000, 0x55),
        ] {
            let mut code = seed.clone();
            code.extend([0x21, 0, 0x40, 0x01, 2, 0, 0xed, compare, 0xcb, 0x46]);
            let (mut c, mut m) = fixture(&code, policy);
            c.set_a(0x28);
            c.f = 1;
            step(
                &mut c,
                &mut m,
                if seed[0] == 0xd3 { 11 } else { 13 },
                1,
                policy,
            );
            assert_eq!(c.wz, intermediate);
            step(&mut c, &mut m, 10, 1, policy);
            step(&mut c, &mut m, 10, 1, policy);
            step(&mut c, &mut m, 16, 2, policy);
            step(&mut c, &mut m, 12, 2, policy);
            assert_eq!((c.wz, c.f), (final_wz, flags));
        }
    }
}

#[test]
fn unlisted_instructions_preserve_latch_in_sequences() {
    for policy in POLICIES {
        for (code, cycles, m1) in [
            (vec![0x00], 4, 1),
            (vec![0x21, 0, 0x40], 10, 1),
            (vec![0x01, 0x34, 0x12], 10, 1),
            (vec![0xdd, 0x21, 0, 0x50], 14, 2),
            (vec![0x23], 6, 1),
            (vec![0x2b], 6, 1),
            (vec![0xdd, 0x23], 10, 2),
            (vec![0x34], 11, 1),
            (vec![0x35], 11, 1),
            (vec![0x36, 0], 10, 1),
            (vec![0x7e], 7, 1),
            (vec![0x77], 7, 1),
            (vec![0x86], 7, 1),
            (vec![0x08], 4, 1),
            (vec![0xd9], 4, 1),
            (vec![0xeb], 4, 1),
            (vec![0xc5], 11, 1),
            (vec![0xc1], 10, 1),
            (vec![0xf9], 6, 1),
            (vec![0xcb, 0x06], 15, 2),
            (vec![0xcb, 0x86], 15, 2),
            (vec![0xcb, 0xc6], 15, 2),
            (vec![0xcb, 0x40], 8, 2),
            (vec![0xed, 0x00], 8, 2),
            (vec![0xed, 0x44], 8, 2),
            (vec![0xed, 0x47], 9, 2),
            (vec![0xed, 0x57], 9, 2),
            (vec![0xed, 0x5e], 8, 2),
            (vec![0xf3], 4, 1),
            (vec![0xfb], 4, 1),
            (vec![0x27], 4, 1),
            (vec![0x37], 4, 1),
            (vec![0x3f], 4, 1),
            (vec![0xdd, 0xfd, 0x00], 12, 3),
        ] {
            let mut sequence = vec![0x3a, 0xff, 0x27];
            sequence.extend(&code);
            let (mut c, mut m) = fixture(&sequence, policy);
            c.set_hl(0x4000);
            step(&mut c, &mut m, 13, 1, policy);
            step(&mut c, &mut m, cycles, m1, policy);
            probe(&mut c, &mut m, 0x2800, policy);
        }
        let (mut c, mut m) = fixture(&[0x3a, 0xff, 0x27, 0x76], policy);
        step(&mut c, &mut m, 13, 1, policy);
        step(&mut c, &mut m, 4, 1, policy);
        step(&mut c, &mut m, 4, 1, policy);
        assert_eq!(c.wz, 0x2800);
        assert_eq!(c.clone().wz, 0x2800);
    }
    assert_eq!(Cpu::new().wz, 0);
    assert_eq!(Cpu::default().wz, 0);
}

#[test]
fn repeated_input_can_replace_its_opcode_to_probe_intermediate_wz() {
    for policy in POLICIES {
        // Input replaces B2/BA with 46h, so the next iteration executes ED 46
        // (IM 0), then BIT 0,(HL). ED straddles a page boundary: the repeated
        // latch is 2800h, opposing both old-BC input latches (0201h/01FFh).
        for (opcode, flags) in [(0xb2, 0x38), (0xba, 0x39)] {
            let (mut c, mut m) = fixture(&[], policy);
            m.bytes[0x27ff..0x2803].copy_from_slice(&[0xed, opcode, 0xcb, 0x46]);
            c.pc = 0x27ff;
            c.set_hl(0x2800);
            c.set_bc(0x0200);
            m.input = 0x46;
            step(&mut c, &mut m, 21, 2, policy);
            assert_eq!((c.pc, c.wz), (0x27ff, 0x2800));
            step(&mut c, &mut m, 8, 2, policy);
            step(&mut c, &mut m, 12, 2, policy); // (HL)=CB/ED, bit 0 set.
            assert_eq!(c.f, flags);
            assert_eq!(c.wz, 0x2800);
        }
    }
}

#[test]
fn interrupts_between_repeats_and_im0_indirect_jump_observation() {
    for policy in POLICIES {
        for (mode, destination, cycles) in
            [(0, 0x38, 13), (1, 0x38, 13), (2, 0x2800, 19), (3, 0x66, 11)]
        {
            let (mut c, mut m) = fixture(&[0xed, 0xb0], policy);
            c.set_hl(0x4000);
            c.set_de(0x5000);
            c.set_bc(2);
            c.iff1 = true;
            c.i = 0xff;
            c.im = mode;
            // IM2 word crosses FFFFh. IM0 FF is RST 38h.
            m.bytes[0xffff] = 0;
            m.bytes[0] = 0x28;
            step(&mut c, &mut m, 21, 2, policy);
            assert_eq!(c.wz, 0x8001);
            m.irq = mode != 3;
            c.nmi_pending = mode == 3;
            step(&mut c, &mut m, cycles, 0, policy);
            assert_eq!((c.pc, c.wz), (destination, destination));
            assert_eq!(&m.bytes[0xeffe..0xf000], &[0, 0x80]);
            m.irq = false;
            let return_code: &[u8] = if mode == 3 {
                &[0xed, 0x45]
            } else {
                &[0xed, 0x4d]
            };
            m.bytes[usize::from(destination)..usize::from(destination) + 2]
                .copy_from_slice(return_code);
            step(&mut c, &mut m, 14, 2, policy);
            assert_eq!((c.pc, c.wz), (0x8000, 0x8000));
            step(&mut c, &mut m, 16, 2, policy);
            assert_eq!(c.wz, 0x8000); // Terminal LDIR preserves return latch.
        }
        // Supported IM0 E9 supplies JP (HL); no fetch wait on the acknowledge,
        // and no WZ update. Observe repeat state at a destination with opposite bits.
        let (mut c, mut m) = fixture(&[0xed, 0xb2], policy);
        c.set_bc(0x0200);
        c.set_hl(0x4000);
        step(&mut c, &mut m, 21, 2, policy);
        c.set_hl(0x2800);
        m.bytes[0x2800..0x2805].copy_from_slice(&[0x21, 0, 0x40, 0xcb, 0x46]);
        c.iff1 = true;
        m.irq = true;
        m.vector = 0xe9;
        step(&mut c, &mut m, 6, 0, policy);
        assert_eq!((c.pc, c.wz), (0x2800, 0x8001));
        m.irq = false;
        step(&mut c, &mut m, 10, 1, policy);
        step(&mut c, &mut m, 12, 2, policy);
        assert_eq!(c.f, 0x54);

        // CPDR's terminal decrement uses the return destination latch, rather
        // than the preceding repeat latch, after an interrupt between iterations.
        let (mut c, mut m) = fixture(&[0xed, 0xb9], policy);
        c.set_hl(0x4000);
        c.set_bc(2);
        c.set_a(1);
        c.f = 1;
        step(&mut c, &mut m, 21, 2, policy);
        assert_eq!(c.wz, 0x8001);
        c.nmi_pending = true;
        m.bytes[0x66..0x68].copy_from_slice(&[0xed, 0x45]);
        step(&mut c, &mut m, 11, 0, policy);
        step(&mut c, &mut m, 14, 2, policy);
        assert_eq!(c.wz, 0x8000);
        step(&mut c, &mut m, 16, 2, policy);
        probe(&mut c, &mut m, 0x7fff, policy);
        assert_eq!(c.f, 0x7d);
    }
}

#[test]
fn supported_im0_uses_supplied_instruction_semantics() {
    for policy in POLICIES {
        // Acknowledge supplies the opcode; operands start at the interrupted PC.
        // Untaken conditional JP/CALL still latch their immediate destination.
        for (opcode, flags, cycles, pc, wz) in [
            (0x00, 0, 6, 0x8000, 0x68ff),
            (0xc3, 0, 12, 0x2800, 0x2800),
            (0xc2, 0x40, 12, 0x8002, 0x2800),
            (0xc4, 0x40, 12, 0x8002, 0x2800),
            (0xcd, 0, 19, 0x2800, 0x2800),
            (0xc9, 0, 12, 0x2800, 0x2800),
        ] {
            let (mut c, mut m) = fixture(&[0, 0x28], policy);
            c.wz = 0x68ff;
            c.f = flags;
            c.iff1 = true;
            c.im = 0;
            m.irq = true;
            m.vector = opcode;
            m.bytes[0xf001] = 0x28;
            step(&mut c, &mut m, cycles, 0, policy);
            assert_eq!((c.pc, c.wz), (pc, wz));
            m.irq = false;
            probe(&mut c, &mut m, wz, policy);
        }
    }
}

#[test]
fn machine_diagnostic_and_reset_on_both_generations() {
    use msx_core::{Machine, Model};
    let code: Vec<u8> = include_str!("../../../tests/fixtures/wz-probe.hex")
        .lines()
        .flat_map(|line| line.split('#').next().unwrap().split_whitespace())
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect();
    for model in [Model::Msx1, Model::Msx2] {
        let mut main = vec![0; 32768];
        main[..3].copy_from_slice(&[0xc3, 0, 1]);
        main[0x2d] = u8::from(model == Model::Msx2);
        main[0x100..0x100 + code.len()].copy_from_slice(&code);
        let mut m = Machine::default();
        m.set_model(model);
        if model == Model::Msx2 {
            let mut sub = vec![0; 16384];
            sub[..2].copy_from_slice(b"CD");
            m.load_msx2_firmware(&main, &sub).unwrap();
        } else {
            m.load_bios(&main).unwrap();
        }
        for _ in 0..2 {
            assert_eq!(m.cpu.wz, 0);
            for _ in 0..1000 {
                m.step();
                if m.cpu.halted {
                    break;
                }
            }
            assert!(m.cpu.halted);
            let expected = [0x7d, 0x91, 0x39, 0x7d, 0x75, 0x7d, 0x75, 0x7d];
            let captured: Vec<_> = (0xc100..0xc108).map(|a| m.hardware.read(a)).collect();
            assert_eq!(captured, expected);
            assert_eq!(&m.hardware.vdp.vram[..8], &expected);
            m.cpu.wz = 0xffff;
            m.reset();
        }
    }
}
