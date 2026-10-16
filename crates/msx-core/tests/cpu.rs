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
