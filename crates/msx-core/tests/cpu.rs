use msx_core::cpu::{Bus, Cpu, C, H, N, PV, S, Z};
struct Memory {
    bytes: Vec<u8>,
    ports: Vec<(u16, u8)>,
    irq: bool,
    vector: u8,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            bytes: vec![0; 65536],
            ports: vec![],
            irq: false,
            vector: 255,
        }
    }
}
impl Bus for Memory {
    fn read(&mut self, a: u16) -> u8 {
        self.bytes[a as usize]
    }
    fn write(&mut self, a: u16, v: u8) {
        self.bytes[a as usize] = v;
    }
    fn input(&mut self, p: u16) -> u8 {
        (p >> 8) as u8
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
fn fixture(code: &[u8]) -> (Cpu, Memory) {
    let mut m = Memory::default();
    m.bytes[..code.len()].copy_from_slice(code);
    (Cpu::new(), m)
}
#[test]
fn arithmetic_exhaustive() {
    let (mut c, mut m) = fixture(&[0, 0]);
    for (op, sub, with_carry, compare) in [
        (0xc6, false, false, false),
        (0xce, false, true, false),
        (0xd6, true, false, false),
        (0xde, true, true, false),
        (0xfe, true, false, true),
    ] {
        m.bytes[0] = op;
        for a in 0..=255u16 {
            for b in 0..=255u16 {
                for carry in 0..=u16::from(with_carry) {
                    c.pc = 0;
                    c.set_a(a as u8);
                    c.f = carry as u8;
                    m.bytes[1] = b as u8;
                    assert_eq!(c.step(&mut m), 7);
                    let signed = if sub {
                        a as i32 - b as i32 - carry as i32
                    } else {
                        a as i32 + b as i32 + carry as i32
                    };
                    let r = signed as u8;
                    let signed_a = a as u8 as i8 as i32;
                    let signed_b = b as u8 as i8 as i32;
                    let signed_r = if sub {
                        signed_a - signed_b - carry as i32
                    } else {
                        signed_a + signed_b + carry as i32
                    };
                    let half = if sub {
                        (a & 15) < (b & 15) + carry
                    } else {
                        (a & 15) + (b & 15) + carry > 15
                    };
                    let flags = (r & S)
                        | if r == 0 { Z } else { 0 }
                        | if half { H } else { 0 }
                        | if !(-128..=127).contains(&signed_r) {
                            PV
                        } else {
                            0
                        }
                        | if sub { N } else { 0 }
                        | if !(0..=255).contains(&signed) { C } else { 0 };
                    assert_eq!(c.f & 0xd7, flags, "op {op:02x}, {a:02x}, {b:02x}, {carry}");
                    assert_eq!(c.a(), if compare { a as u8 } else { r });
                }
            }
        }
    }
}
#[test]
fn decimal_adjust_all_valid_bcd_sums() {
    let (mut c, mut m) = fixture(&[0xc6, 0, 0x27]);
    for a in 0..100 {
        for b in 0..100 {
            c.pc = 0;
            c.set_a(((a / 10) * 16 + a % 10) as u8);
            m.bytes[1] = ((b / 10) * 16 + b % 10) as u8;
            c.step(&mut m);
            assert_eq!(c.step(&mut m), 4);
            let sum = (a + b) % 100;
            assert_eq!(c.a(), ((sum / 10) * 16 + sum % 10) as u8);
            assert_eq!(c.f & C != 0, a + b >= 100);
        }
    }
}
#[test]
fn stack_call_branch_exchanges_and_io() {
    let (mut c, mut m) = fixture(&[
        0x31, 0x00, 0x90, 0x3e, 0x42, 0xcd, 0x0c, 0x00, 0xd3, 0x98, 0x76, 0, 0xf5, 0x3e, 1, 0xf1,
        0xc9,
    ]);
    let times = [10, 7, 17, 11, 7, 10, 10, 11, 4];
    for t in times {
        assert_eq!(c.step(&mut m), t);
    }
    assert!(c.halted);
    assert_eq!(c.sp, 0x9000);
    assert_eq!(m.ports, vec![(0x4298, 0x42)]);
    assert_eq!(c.pc, 11);
    let r = c.refresh;
    c.step(&mut m);
    assert_eq!(c.refresh, r + 1);
    c.halted = false;
    c.pc = 0x100;
    m.bytes[0x100..0x104].copy_from_slice(&[0x08, 0xd9, 0xeb, 0xe3]);
    c.set_hl(0x1234);
    c.set_de(0x5678);
    c.alternate[4] = 0xab;
    c.alternate[5] = 0xcd;
    c.alternate[7] = 0x99;
    c.alternate_f = 0x81;
    c.step(&mut m);
    assert_eq!((c.a(), c.f), (0x99, 0x81));
    c.step(&mut m);
    assert_eq!(c.hl(), 0xabcd);
    c.step(&mut m);
    assert_eq!(c.de(), 0xabcd);
    m.bytes[0x9000] = 0x11;
    m.bytes[0x9001] = 0x22;
    assert_eq!(c.step(&mut m), 19);
    assert_eq!(c.hl(), 0x2211);
}
#[test]
fn index_and_bit_prefix_vectors() {
    let (mut c, mut m) = fixture(&[
        0xdd, 0x21, 0x00, 0x80, // LD IX,8000
        0xfd, 0x21, 0x00, 0x90, // LD IY,9000
        0xdd, 0x36, 0xfe, 0x81, // LD (IX-2),81
        0xdd, 0xcb, 0xfe, 0x06, // RLC (IX-2)
        0xfd, 0x36, 1, 0x80, // LD (IY+1),80
        0xfd, 0xcb, 1, 0x7e, // BIT 7,(IY+1)
        0xfd, 0x66, 1, // LD H,(IY+1), real H
        0xcb, 0xbc, // RES 7,H
        0xcb, 0xc4, // SET 0,H
        0xcb, 0x3c, // SRL H
    ]);
    for t in [14, 14, 19, 23, 19, 20, 19, 8, 8, 8] {
        assert_eq!(c.step(&mut m), t);
    }
    assert_eq!(m.bytes[0x7ffe], 3);
    assert_eq!(c.iy, 0x9000);
    assert_eq!(c.r[4], 0);
    assert_eq!(c.f & (Z | PV | C), Z | PV | C);
}
#[test]
fn ed_transfer_search_16bit_and_nibbles() {
    let (mut c, mut m) = fixture(&[0xed, 0xb0, 0xed, 0xb1, 0xed, 0x4a, 0xed, 0x67, 0xed, 0x6f]);
    c.set_hl(0x2000);
    c.set_de(0x3000);
    c.set_bc(3);
    m.bytes[0x2000..0x2003].copy_from_slice(&[1, 2, 3]);
    for t in [21, 21, 16] {
        assert_eq!(c.step(&mut m), t);
    }
    assert_eq!(&m.bytes[0x3000..0x3003], &[1, 2, 3]);
    c.set_a(2);
    c.set_hl(0x3000);
    c.set_bc(3);
    assert_eq!(c.step(&mut m), 21);
    assert_eq!(c.step(&mut m), 16);
    assert_eq!(c.hl(), 0x3002);
    assert_ne!(c.f & Z, 0);
    c.set_hl(0x7fff);
    c.set_bc(0);
    c.f = C;
    assert_eq!(c.step(&mut m), 15);
    assert_eq!(c.hl(), 0x8000);
    assert_eq!(c.f & (S | H | PV | C | N), S | H | PV);
    c.set_hl(0x4000);
    m.bytes[0x4000] = 0x34;
    c.set_a(0x12);
    assert_eq!(c.step(&mut m), 18);
    assert_eq!((c.a(), m.bytes[0x4000]), (0x14, 0x23));
    c.step(&mut m);
    assert_eq!((c.a(), m.bytes[0x4000]), (0x12, 0x34));
}
#[test]
fn interrupts_ei_halt_nmi_and_all_modes() {
    let (mut c, mut m) = fixture(&[0xfb, 0x76]);
    m.irq = true;
    c.im = 1;
    assert_eq!(c.step(&mut m), 4);
    assert_eq!(c.step(&mut m), 4);
    assert!(c.halted);
    assert_eq!(c.step(&mut m), 13);
    assert_eq!(c.pc, 0x38);
    assert!(!c.halted);
    assert_eq!(m.bytes[0xfffd], 2);
    for (mode, timing, dest) in [(0, 13, 0x38), (1, 13, 0x38), (2, 19, 0x4567)] {
        c = Cpu::new();
        c.im = mode;
        c.i = 0x80;
        c.iff1 = true;
        c.iff2 = true;
        c.pc = 0x1234;
        m.bytes[0x80ff] = 0x67;
        m.bytes[0x8100] = 0x45;
        assert_eq!(c.step(&mut m), timing);
        assert_eq!(c.pc, dest);
        assert!(!c.iff1 && !c.iff2);
    }
    c.pc = 0x1234;
    c.iff1 = true;
    c.nmi_pending = true;
    assert_eq!(c.step(&mut m), 11);
    assert_eq!(c.pc, 0x66);
    assert!(!c.iff1 && c.iff2);
    m.irq = false;
    m.bytes[0x66] = 0xed;
    m.bytes[0x67] = 0x45;
    assert_eq!(c.step(&mut m), 14);
    assert_eq!(c.pc, 0x1234);
    assert!(c.iff1);
}
#[test]
fn logical_parity_incdec_and_conditional_timing() {
    let (mut c, mut m) = fixture(&[0, 0]);
    for (op, result) in [(0xe6, 0x55), (0xee, 0xaa), (0xf6, 0xff)] {
        m.bytes[0] = op;
        m.bytes[1] = 0x55;
        c.pc = 0;
        c.set_a(0xff);
        c.step(&mut m);
        assert_eq!(c.a(), result);
        assert_ne!(c.f & PV, 0);
    }
    for v in 0..=255u8 {
        c.pc = 0;
        c.r[0] = v;
        c.f = C;
        m.bytes[0] = 4;
        m.bytes[1] = 5;
        c.step(&mut m);
        assert_eq!(c.r[0], v.wrapping_add(1));
        assert_eq!(c.f & N, 0);
        assert_eq!(c.f & PV != 0, v == 0x7f);
        c.step(&mut m);
        assert_eq!(c.r[0], v);
        assert_eq!(c.f & C, C);
        assert_eq!(c.f & PV != 0, v == 0x7f);
    }
    m.bytes[..4].copy_from_slice(&[0x20, 2, 0, 0]);
    c.pc = 0;
    c.f = Z;
    assert_eq!(c.step(&mut m), 7);
    assert_eq!(c.pc, 2);
    c.pc = 0;
    c.f = 0;
    assert_eq!(c.step(&mut m), 12);
    assert_eq!(c.pc, 4);
}

// Original timing fixtures. All expectations list bare Z80 / MSX elapsed T-states.
fn timed_step(c: &mut Cpu, m: &mut Memory, expected: u32) {
    let before = c.tstates;
    assert_eq!(c.step(m), expected);
    assert_eq!(c.tstates - before, u64::from(expected));
}

#[test]
fn ordinary_fetch_wait_vectors() {
    use msx_core::cpu::FetchWait;
    let vectors: &[(&[u8], u8, u32, u32)] = &[
        (&[0x00], 0, 4, 5),
        (&[0x76], 0, 4, 5),
        (&[0x3e, 0x42], 0, 7, 8),
        (&[0xdb, 0x98], 0, 11, 12),
        (&[0xd3, 0x98], 0, 11, 12),
        (&[0xcb, 0x00], 0, 8, 10),
        (&[0xed, 0x44], 0, 8, 10),
        (&[0xdd, 0x21, 0, 0x80], 0, 14, 16),
        (&[0xdd, 0xcb, 0xfe, 0x46], 0, 20, 22),
        (&[0xfd, 0xcb, 1, 0x06], 0, 23, 25),
        (&[0xdd, 0xfd, 0x00], 0, 12, 15),
        (&[0xdd, 0xdd, 0x3e, 7], 0, 15, 18),
        (&[0xfd, 0xdd, 0xed, 0x44], 0, 16, 20),
        (&[0xfd, 0xdd, 0xcb, 1, 0x46], 0, 24, 27),
        (&[0x20, 2], 0, 12, 13),       // JR NZ taken
        (&[0x20, 2], Z, 7, 8),         // JR NZ untaken
        (&[0xc2, 0, 0x80], 0, 10, 11), // JP NZ taken
        (&[0xc2, 0, 0x80], Z, 10, 11),
        (&[0xc4, 0, 0x80], 0, 17, 18), // CALL NZ taken
        (&[0xc4, 0, 0x80], Z, 10, 11),
        (&[0xc0], 0, 11, 12), // RET NZ taken
        (&[0xc0], Z, 5, 6),
    ];
    for policy in [FetchWait::None, FetchWait::Msx] {
        for &(code, flags, bare, msx) in vectors {
            let (_, mut m) = fixture(code);
            let mut c = Cpu::with_fetch_wait(policy);
            c.f = flags;
            c.ix = 0x8000;
            c.iy = 0x9000;
            timed_step(
                &mut c,
                &mut m,
                if policy == FetchWait::Msx { msx } else { bare },
            );
            if code == [0x76] {
                for _ in 0..3 {
                    timed_step(&mut c, &mut m, if policy == FetchWait::Msx { 5 } else { 4 });
                    assert_eq!(c.pc, 1);
                }
            }
        }
        let (_, mut m) = fixture(&[0xed, 0xb0]);
        let mut c = Cpu::with_fetch_wait(policy);
        c.set_hl(0x2000);
        c.set_de(0x3000);
        c.set_bc(2);
        m.bytes[0x2000..0x2002].copy_from_slice(&[42, 99]);
        timed_step(
            &mut c,
            &mut m,
            if policy == FetchWait::Msx { 23 } else { 21 },
        );
        assert_eq!(c.pc, 0);
        timed_step(
            &mut c,
            &mut m,
            if policy == FetchWait::Msx { 18 } else { 16 },
        );
        assert_eq!(&m.bytes[0x3000..0x3002], &[42, 99]);
    }
}

#[test]
fn waits_do_not_follow_refresh_register_changes() {
    use msx_core::cpu::FetchWait;
    for policy in [FetchWait::None, FetchWait::Msx] {
        let (_, mut m) = fixture(&[0xed, 0x4f, 0xed, 0x5f, 0]); // LD R,A; LD A,R; NOP
        let mut c = Cpu::with_fetch_wait(policy);
        c.refresh = 0xfd;
        c.set_a(0xfe);
        let ed = if policy == FetchWait::Msx { 11 } else { 9 };
        timed_step(&mut c, &mut m, ed);
        assert_eq!(c.refresh, 0xfe);
        timed_step(&mut c, &mut m, ed);
        assert_eq!((c.a(), c.refresh), (0x80, 0x80));
        let nop = if policy == FetchWait::Msx { 5 } else { 4 };
        for (initial, expected) in [(0x7f, 0), (0xff, 0x80)] {
            c.pc = 4;
            c.refresh = initial;
            timed_step(&mut c, &mut m, nop);
            assert_eq!(c.refresh, expected);
        }
        m.bytes.fill(0xdd);
        c.pc = 0;
        c.refresh = 0xa5;
        for _ in 0..2 {
            timed_step(
                &mut c,
                &mut m,
                if policy == FetchWait::Msx {
                    327680
                } else {
                    262144
                },
            );
            assert_eq!((c.pc, c.refresh), (0, 0xa5));
        }
        m.bytes[0] = 0;
        timed_step(&mut c, &mut m, nop); // No stale waits after the bounded yield.
    }
}

#[test]
fn interrupt_acknowledgement_has_no_ordinary_fetch_wait() {
    use msx_core::cpu::FetchWait;
    // Zilog UM0080 pp.12–14 distinguishes acknowledgement from ordinary M1;
    // openMSX Z80.hh specifies NMI=11, IM0/1=13 and IM2=19 on MSX as well.
    for policy in [FetchWait::None, FetchWait::Msx] {
        let fetch = if policy == FetchWait::Msx { 5 } else { 4 };
        for (mode, entry, destination) in [(0, 13, 0x38), (1, 13, 0x38), (2, 19, 0x4567)] {
            let (_, mut m) = fixture(&[0xfb, 0x76]);
            let mut c = Cpu::with_fetch_wait(policy);
            c.im = mode;
            c.i = 0x80;
            m.bytes[0x80ff..0x8101].copy_from_slice(&[0x67, 0x45]);
            m.irq = true;
            timed_step(&mut c, &mut m, fetch); // EI
            timed_step(&mut c, &mut m, fetch); // EI delay permits HALT entry.
            assert!(c.halted);
            timed_step(&mut c, &mut m, entry);
            assert!(!c.halted);
            assert_eq!((c.pc, c.refresh), (destination, 3));
            assert_eq!(&m.bytes[0xfffd..0xffff], &[2, 0]);
            timed_step(&mut c, &mut m, fetch); // First handler opcode gets a wait.
        }
        let (_, mut m) = fixture(&[0x76]);
        let mut c = Cpu::with_fetch_wait(policy);
        c.iff1 = true;
        timed_step(&mut c, &mut m, fetch);
        c.nmi_pending = true;
        timed_step(&mut c, &mut m, 11);
        assert_eq!(c.pc, 0x66);
        assert!(!c.halted && !c.iff1 && c.iff2);
        timed_step(&mut c, &mut m, fetch);

        // IM0 still executes the supplied opcode and reads its operands at PC.
        let (_, mut m) = fixture(&[0x34, 0x12]);
        let mut c = Cpu::with_fetch_wait(policy);
        c.iff1 = true;
        m.irq = true;
        m.vector = 0xc3; // JP 1234h; acknowledgement supplies JP, not a memory fetch.
        timed_step(&mut c, &mut m, 12);
        assert_eq!(c.pc, 0x1234);
        timed_step(&mut c, &mut m, fetch);
    }
}
