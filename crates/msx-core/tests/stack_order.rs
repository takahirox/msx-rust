//! Original access traces; instruction bytes live away from stack/vector data.
use msx_core::cpu::{Bus, Cpu, FetchWait, C, PV, S, Z};

const PC: u16 = 0x2340;
const POLICIES: [FetchWait; 2] = [FetchWait::None, FetchWait::Msx];
// Literal boundary oracles: original SP, first write, second write/final SP.
const PUSH_ADDRESSES: [(u16, u16, u16); 3] = [
    (0x8000, 0x7fff, 0x7ffe),
    (0, 0xffff, 0xfffe),
    (1, 0, 0xffff),
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Access {
    Code(u16, u8), // Opcode, prefix or immediate operand read (not all are M1).
    Read(u16, u8),
    Write(u16, u8),
}
use Access::{Code, Read, Write};

struct RecordingBus {
    bytes: Vec<u8>,
    code_len: u16,
    trace: Vec<Access>,
    irq: bool,
    vector: u8,
}
impl Bus for RecordingBus {
    fn read(&mut self, address: u16) -> u8 {
        let value = self.bytes[usize::from(address)];
        self.trace
            .push(if (PC..PC + self.code_len).contains(&address) {
                Code(address, value)
            } else {
                Read(address, value)
            });
        value
    }
    fn write(&mut self, address: u16, value: u8) {
        self.trace.push(Write(address, value));
        self.bytes[usize::from(address)] = value;
    }
    fn input(&mut self, _: u16) -> u8 {
        panic!("unexpected input")
    }
    fn output(&mut self, _: u16, _: u8) {
        panic!("unexpected output")
    }
    fn irq(&self) -> bool {
        self.irq
    }
    fn interrupt_data(&self) -> u8 {
        self.vector
    }
}

fn fixture(code: &[u8], policy: FetchWait) -> (Cpu, RecordingBus) {
    let mut cpu = Cpu::with_fetch_wait(policy);
    cpu.pc = PC;
    cpu.tstates = 100; // Check accumulation, not just a zero-origin total.
    cpu.f = 0xa5;
    let mut bus = RecordingBus {
        bytes: vec![0; 65536],
        code_len: code.len() as u16,
        trace: vec![],
        irq: false,
        vector: 0xff,
    };
    bus.bytes[usize::from(PC)..usize::from(PC) + code.len()].copy_from_slice(code);
    (cpu, bus)
}

fn step(cpu: &mut Cpu, bus: &mut RecordingBus, cycles: u32, waits: u32, policy: FetchWait) {
    let expected = cycles + if policy == FetchWait::Msx { waits } else { 0 };
    let before = cpu.tstates;
    assert_eq!(cpu.step(bus), expected);
    assert_eq!(cpu.tstates - before, u64::from(expected));
}

fn trace(bus: &RecordingBus, code: &[u8], data: &[Access]) {
    let mut expected: Vec<_> = code
        .iter()
        .enumerate()
        .map(|(i, &v)| Code(PC + i as u16, v))
        .collect();
    expected.extend_from_slice(data);
    assert_eq!(bus.trace, expected);
}

fn pushed(cpu: &Cpu, bus: &RecordingBus, sp: u16, hi: u8, lo: u8) {
    assert_eq!(cpu.sp, sp);
    assert_eq!(bus.bytes[usize::from(sp)], lo);
    assert_eq!(bus.bytes[usize::from(sp.wrapping_add(1))], hi);
}

#[test]
fn push_all_pairs_high_first_with_wrapping_and_both_fetch_policies() {
    for policy in POLICIES {
        for code in [
            &[0xc5][..],
            &[0xd5],
            &[0xe5],
            &[0xf5],
            &[0xdd, 0xe5],
            &[0xfd, 0xe5],
        ] {
            for (sp, first, second) in PUSH_ADDRESSES {
                let (mut cpu, mut bus) = fixture(code, policy);
                cpu.sp = sp;
                cpu.set_bc(0x5aa5);
                cpu.set_de(0x5aa5);
                cpu.set_hl(0x5aa5);
                cpu.set_a(0x5a);
                cpu.ix = 0x5aa5;
                cpu.iy = 0x5aa5;
                let prefixed = code.len() == 2;
                step(
                    &mut cpu,
                    &mut bus,
                    if prefixed { 15 } else { 11 },
                    code.len() as u32,
                    policy,
                );
                trace(&bus, code, &[Write(first, 0x5a), Write(second, 0xa5)]);
                pushed(&cpu, &bus, second, 0x5a, 0xa5);
                assert_eq!(cpu.pc, PC + code.len() as u16);
                assert_eq!(cpu.f, 0xa5);
            }
        }
    }
}

#[test]
fn call_all_conditions_and_rst_write_return_address_high_first() {
    // Each tuple gives the flag and the state required to take that condition.
    let conditions = [
        (Z, false),
        (Z, true),
        (C, false),
        (C, true),
        (PV, false),
        (PV, true),
        (S, false),
        (S, true),
    ];
    for policy in POLICIES {
        for (sp, first, second) in PUSH_ADDRESSES {
            for opcode in [0xcd, 0xc4, 0xcc, 0xd4, 0xdc, 0xe4, 0xec, 0xf4, 0xfc] {
                for taken in [false, true] {
                    if opcode == 0xcd && !taken {
                        continue;
                    }
                    let code = [opcode, 0x78, 0x56];
                    let (mut cpu, mut bus) = fixture(&code, policy);
                    cpu.sp = sp;
                    if opcode != 0xcd {
                        let (flag, set) = conditions[((opcode - 0xc4) / 8) as usize];
                        cpu.f = if set == taken { flag } else { 0 };
                    }
                    let flags = cpu.f;
                    step(&mut cpu, &mut bus, if taken { 17 } else { 10 }, 1, policy);
                    if taken {
                        trace(&bus, &code, &[Write(first, 0x23), Write(second, 0x43)]);
                        pushed(&cpu, &bus, second, 0x23, 0x43);
                        assert_eq!(cpu.pc, 0x5678);
                    } else {
                        trace(&bus, &code, &[]);
                        assert_eq!((cpu.pc, cpu.sp), (0x2343, sp));
                    }
                    assert_eq!(cpu.f, flags);
                }
            }
            for (opcode, target) in [
                (0xc7, 0),
                (0xcf, 8),
                (0xd7, 16),
                (0xdf, 24),
                (0xe7, 32),
                (0xef, 40),
                (0xf7, 48),
                (0xff, 56),
            ] {
                let (mut cpu, mut bus) = fixture(&[opcode], policy);
                cpu.sp = sp;
                step(&mut cpu, &mut bus, 11, 1, policy);
                trace(&bus, &[opcode], &[Write(first, 0x23), Write(second, 0x41)]);
                pushed(&cpu, &bus, second, 0x23, 0x41);
                assert_eq!(cpu.pc, target);
                assert_eq!(cpu.f, 0xa5);
            }
        }
    }
}

#[test]
fn interrupt_entries_write_high_first_without_opcode_fetch_waits() {
    for policy in POLICIES {
        for (sp, first, second) in PUSH_ADDRESSES {
            // NMI, IM0's bus-supplied RST, IM1, IM2.
            for (mode, cycles, target) in
                [(3, 11, 0x66), (0, 13, 0x38), (1, 13, 0x38), (2, 19, 0x4567)]
            {
                let (mut cpu, mut bus) = fixture(&[], policy);
                cpu.sp = sp;
                cpu.im = mode;
                cpu.i = 0x90;
                cpu.iff1 = true;
                cpu.iff2 = true;
                cpu.halted = true;
                cpu.refresh = 0x8f;
                cpu.nmi_pending = mode == 3;
                bus.irq = mode != 3;
                bus.bytes[0x90ff] = 0x67;
                bus.bytes[0x9100] = 0x45;
                step(&mut cpu, &mut bus, cycles, 0, policy);
                let mut accesses = vec![Write(first, 0x23), Write(second, 0x40)];
                if mode == 2 {
                    accesses.extend([Read(0x90ff, 0x67), Read(0x9100, 0x45)]);
                }
                trace(&bus, &[], &accesses);
                pushed(&cpu, &bus, second, 0x23, 0x40);
                assert_eq!(cpu.pc, target);
                assert_eq!(cpu.f, 0xa5);
                assert!(!cpu.iff1 && !cpu.halted && !cpu.nmi_pending);
                assert_eq!(cpu.iff2, mode == 3);
                assert_eq!(cpu.refresh, 0x90);
            }
        }
    }
}

#[test]
fn exchange_reads_both_bytes_before_high_first_writes_including_wrap() {
    for policy in POLICIES {
        for (code, cycles) in [(&[0xe3][..], 19), (&[0xdd, 0xe3], 23), (&[0xfd, 0xe3], 23)] {
            for (sp, high_address) in [(0x8000, 0x8001), (0xffff, 0)] {
                let (mut cpu, mut bus) = fixture(code, policy);
                cpu.sp = sp;
                cpu.set_hl(0x5aa5);
                cpu.ix = 0x5aa5;
                cpu.iy = 0x5aa5;
                bus.bytes[usize::from(sp)] = 0x67;
                bus.bytes[usize::from(high_address)] = 0x89;
                step(&mut cpu, &mut bus, cycles, code.len() as u32, policy);
                trace(
                    &bus,
                    code,
                    &[
                        Read(sp, 0x67),
                        Read(high_address, 0x89),
                        Write(high_address, 0x5a),
                        Write(sp, 0xa5),
                    ],
                );
                assert_eq!(cpu.sp, sp);
                assert_eq!(cpu.f, 0xa5);
                assert_eq!(cpu.pc, PC + code.len() as u16);
                assert_eq!(cpu.hl(), if code[0] == 0xe3 { 0x8967 } else { 0x5aa5 });
                assert_eq!(cpu.ix, if code[0] == 0xdd { 0x8967 } else { 0x5aa5 });
                assert_eq!(cpu.iy, if code[0] == 0xfd { 0x8967 } else { 0x5aa5 });
                assert_eq!(bus.bytes[usize::from(sp)], 0xa5);
                assert_eq!(bus.bytes[usize::from(high_address)], 0x5a);
            }
        }
    }
}

#[test]
fn ordinary_word_stores_still_write_low_first_including_wrap() {
    for policy in POLICIES {
        for (prefix, opcode, cycles, waits) in [
            (&[][..], 0x22, 16, 1),
            (&[0xdd][..], 0x22, 20, 2),
            (&[0xfd][..], 0x22, 20, 2),
            (&[0xed][..], 0x43, 20, 2),
            (&[0xed][..], 0x53, 20, 2),
            (&[0xed][..], 0x63, 20, 2),
            (&[0xed][..], 0x73, 20, 2),
        ] {
            for (address, high_address) in [(0x8000u16, 0x8001), (0xffff, 0)] {
                let mut code = prefix.to_vec();
                code.extend([opcode, address as u8, (address >> 8) as u8]);
                let (mut cpu, mut bus) = fixture(&code, policy);
                cpu.set_bc(0x5aa5);
                cpu.set_de(0x5aa5);
                cpu.set_hl(0x5aa5);
                cpu.ix = 0x5aa5;
                cpu.iy = 0x5aa5;
                cpu.sp = 0x5aa5;
                step(&mut cpu, &mut bus, cycles, waits, policy);
                trace(
                    &bus,
                    &code,
                    &[Write(address, 0xa5), Write(high_address, 0x5a)],
                );
                assert_eq!(bus.bytes[usize::from(address)], 0xa5);
                assert_eq!(bus.bytes[usize::from(high_address)], 0x5a);
                assert_eq!(cpu.sp, 0x5aa5);
                assert_eq!(cpu.f, 0xa5);
                assert_eq!(cpu.pc, PC + code.len() as u16);
            }
        }
    }
}

#[test]
fn pop_and_return_keep_low_first_reads_including_wrap() {
    for policy in POLICIES {
        for (code, cycles) in [
            (&[0xc1][..], 10),
            (&[0xd1][..], 10),
            (&[0xe1][..], 10),
            (&[0xf1][..], 10),
            (&[0xdd, 0xe1][..], 14),
            (&[0xfd, 0xe1][..], 14),
            (&[0xc9][..], 10),
            (&[0xed, 0x45][..], 14),
            (&[0xed, 0x4d][..], 14),
        ] {
            for (sp, high_address, final_sp) in [(0x8000, 0x8001, 0x8002), (0xffff, 0, 1)] {
                let (mut cpu, mut bus) = fixture(code, policy);
                cpu.sp = sp;
                bus.bytes[usize::from(sp)] = 0x67;
                bus.bytes[usize::from(high_address)] = 0x89;
                step(&mut cpu, &mut bus, cycles, code.len() as u32, policy);
                trace(&bus, code, &[Read(sp, 0x67), Read(high_address, 0x89)]);
                assert_eq!(cpu.sp, final_sp);
                match code {
                    [0xc1] => assert_eq!(cpu.bc(), 0x8967),
                    [0xd1] => assert_eq!(cpu.de(), 0x8967),
                    [0xe1] => assert_eq!(cpu.hl(), 0x8967),
                    [0xf1] => assert_eq!((cpu.a(), cpu.f), (0x89, 0x67)),
                    [0xdd, 0xe1] => assert_eq!(cpu.ix, 0x8967),
                    [0xfd, 0xe1] => assert_eq!(cpu.iy, 0x8967),
                    _ => assert_eq!(cpu.pc, 0x8967),
                }
                if code != [0xf1] {
                    assert_eq!(cpu.f, 0xa5);
                }
                assert_eq!(bus.bytes[usize::from(sp)], 0x67);
                assert_eq!(bus.bytes[usize::from(high_address)], 0x89);
            }
        }
    }
}
