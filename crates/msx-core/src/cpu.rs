//! Independently implemented Z80 decoder, organized by the instruction bit fields.
//! Timings are elapsed T-states, including the selected opcode-fetch wait policy.
//! Access-phase synchronization and bus contention are not modeled.
pub const S: u8 = 0x80;
pub const Z: u8 = 0x40;
pub const H: u8 = 0x10;
pub const PV: u8 = 4;
pub const N: u8 = 2;
pub const C: u8 = 1;

/// Ordinary M1 fetch waits; interrupt acknowledgement uses separate timing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FetchWait {
    /// Standalone, zero-wait Z80 timing.
    #[default]
    None,
    /// One extra T-state per opcode/prefix fetch and HALT idle fetch.
    Msx,
}

pub trait Bus {
    fn read(&mut self, address: u16) -> u8;
    fn write(&mut self, address: u16, value: u8);
    fn input(&mut self, port: u16) -> u8;
    fn output(&mut self, port: u16, value: u8);
    fn irq(&self) -> bool {
        false
    }
    fn interrupt_data(&self) -> u8 {
        0xff
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cpu {
    /// B,C,D,E,H,L,(unused),A; F is stored separately.
    pub r: [u8; 8],
    pub f: u8,
    pub alternate: [u8; 8],
    pub alternate_f: u8,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub refresh: u8,
    pub iff1: bool,
    pub iff2: bool,
    pub im: u8,
    pub halted: bool,
    pub nmi_pending: bool,
    ei_delay: u8,
    fetch_wait: FetchWait,
    fetch_wait_cycles: u32,
    pub tstates: u64,
}
fn sz(v: u8) -> u8 {
    (v & 0xa8) | if v == 0 { Z } else { 0 }
}
fn szp(v: u8) -> u8 {
    sz(v) | if v.count_ones() & 1 == 0 { PV } else { 0 }
}
impl Cpu {
    pub fn new() -> Self {
        Self {
            sp: 0xffff,
            ..Self::default()
        }
    }
    pub fn with_fetch_wait(fetch_wait: FetchWait) -> Self {
        Self {
            fetch_wait,
            ..Self::new()
        }
    }
    pub fn a(&self) -> u8 {
        self.r[7]
    }
    pub fn set_a(&mut self, v: u8) {
        self.r[7] = v;
    }
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.r[0], self.r[1]])
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.r[2], self.r[3]])
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.r[4], self.r[5]])
    }
    pub fn set_bc(&mut self, v: u16) {
        [self.r[0], self.r[1]] = v.to_be_bytes();
    }
    pub fn set_de(&mut self, v: u16) {
        [self.r[2], self.r[3]] = v.to_be_bytes();
    }
    pub fn set_hl(&mut self, v: u16) {
        [self.r[4], self.r[5]] = v.to_be_bytes();
    }
    fn index(&self, idx: u8) -> u16 {
        match idx {
            1 => self.ix,
            2 => self.iy,
            _ => self.hl(),
        }
    }
    fn set_index(&mut self, idx: u8, v: u16) {
        match idx {
            1 => self.ix = v,
            2 => self.iy = v,
            _ => self.set_hl(v),
        }
    }
    fn pair(&self, p: u8, idx: u8, af: bool) -> u16 {
        match p {
            0 => self.bc(),
            1 => self.de(),
            2 => self.index(idx),
            _ if af => u16::from_be_bytes([self.a(), self.f]),
            _ => self.sp,
        }
    }
    fn set_pair(&mut self, p: u8, idx: u8, af: bool, v: u16) {
        match p {
            0 => self.set_bc(v),
            1 => self.set_de(v),
            2 => self.set_index(idx, v),
            _ if af => {
                self.set_a((v >> 8) as u8);
                self.f = v as u8;
            }
            _ => self.sp = v,
        }
    }
    fn byte(&mut self, bus: &mut impl Bus) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn word(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = self.byte(bus);
        u16::from_le_bytes([lo, self.byte(bus)])
    }
    fn refresh(&mut self) {
        self.refresh = (self.refresh & 0x80) | (self.refresh.wrapping_add(1) & 0x7f);
    }
    fn opcode(&mut self, bus: &mut impl Bus) -> u8 {
        self.fetch_wait();
        self.refresh();
        self.byte(bus)
    }
    fn fetch_wait(&mut self) {
        self.fetch_wait_cycles += u32::from(self.fetch_wait == FetchWait::Msx);
    }
    fn read16(bus: &mut impl Bus, a: u16) -> u16 {
        let lo = bus.read(a);
        u16::from_le_bytes([lo, bus.read(a.wrapping_add(1))])
    }
    fn write16(bus: &mut impl Bus, a: u16, v: u16) {
        bus.write(a, v as u8);
        bus.write(a.wrapping_add(1), (v >> 8) as u8);
    }
    fn push(&mut self, bus: &mut impl Bus, v: u16) {
        // Each write may change the mapping used by the following access.
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, (v >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, v as u8);
    }
    fn pop(&mut self, bus: &mut impl Bus) -> u16 {
        let v = Self::read16(bus, self.sp);
        self.sp = self.sp.wrapping_add(2);
        v
    }
    fn reg(&self, r: u8, idx: u8, address: u16, bus: &mut impl Bus) -> u8 {
        match r {
            6 => bus.read(address),
            4 if idx != 0 => (self.index(idx) >> 8) as u8,
            5 if idx != 0 => self.index(idx) as u8,
            _ => self.r[r as usize],
        }
    }
    fn put(&mut self, r: u8, idx: u8, address: u16, v: u8, bus: &mut impl Bus) {
        match r {
            6 => bus.write(address, v),
            4 if idx != 0 => self.set_index(idx, (self.index(idx) & 255) | ((v as u16) << 8)),
            5 if idx != 0 => self.set_index(idx, (self.index(idx) & 0xff00) | v as u16),
            _ => self.r[r as usize] = v,
        }
    }
    fn condition(&self, y: u8) -> bool {
        let flag = match y / 2 {
            0 => Z,
            1 => C,
            2 => PV,
            _ => S,
        };
        (self.f & flag != 0) == (y & 1 != 0)
    }
    fn relative(&mut self, d: u8) {
        self.pc = self.pc.wrapping_add_signed(d as i8 as i16);
    }
    fn alu(&mut self, y: u8, v: u8) {
        let a = self.a();
        let carry = u8::from(self.f & C != 0 && y & 1 != 0);
        match y {
            0..=3 | 7 => {
                let sub = y >= 2;
                let c = if y == 7 { 0 } else { carry };
                let wide = if sub {
                    (a as u16).wrapping_sub(v as u16 + c as u16)
                } else {
                    a as u16 + v as u16 + c as u16
                };
                let r = wide as u8;
                let overflow = if sub {
                    (a ^ v) & (a ^ r)
                } else {
                    !(a ^ v) & (a ^ r)
                } & 0x80
                    != 0;
                self.f = sz(r)
                    | ((a ^ v ^ r) & H)
                    | if sub { N } else { 0 }
                    | if wide > 255 { C } else { 0 }
                    | if overflow { PV } else { 0 };
                if y != 7 {
                    self.set_a(r);
                } else {
                    self.f = (self.f & !0x28) | (v & 0x28);
                }
            }
            _ => {
                let r = match y {
                    4 => a & v,
                    5 => a ^ v,
                    _ => a | v,
                };
                self.set_a(r);
                self.f = szp(r) | if y == 4 { H } else { 0 };
            }
        }
    }
    fn incdec(&mut self, v: u8, dec: bool) -> u8 {
        let r = if dec {
            v.wrapping_sub(1)
        } else {
            v.wrapping_add(1)
        };
        self.f = (self.f & C)
            | sz(r)
            | ((v ^ r) & H)
            | if dec { N } else { 0 }
            | if (!dec && v == 0x7f) || (dec && v == 0x80) {
                PV
            } else {
                0
            };
        r
    }
    /// Executes one instruction or interrupt acknowledge, returning consumed T-states.
    pub fn step(&mut self, bus: &mut impl Bus) -> u32 {
        self.fetch_wait_cycles = 0;
        let cycles = if self.nmi_pending {
            self.nmi_pending = false;
            self.halted = false;
            self.iff2 = self.iff1;
            self.iff1 = false;
            self.refresh();
            self.push(bus, self.pc);
            self.pc = 0x66;
            11
        } else if bus.irq() && self.iff1 && self.ei_delay == 0 {
            self.halted = false;
            self.iff1 = false;
            self.iff2 = false;
            self.refresh();
            if self.im == 0 {
                let op = bus.interrupt_data();
                self.decode(bus, op, 0) + 2
            } else {
                self.push(bus, self.pc);
                if self.im == 2 {
                    self.pc = Self::read16(bus, u16::from_be_bytes([self.i, bus.interrupt_data()]));
                    19
                } else {
                    self.pc = 0x38;
                    13
                }
            }
        } else if self.halted {
            self.fetch_wait();
            self.refresh();
            4
        } else {
            let mut idx = 0;
            let mut extra = 0;
            loop {
                let op = self.opcode(bus);
                if op == 0xdd || op == 0xfd {
                    idx = if op == 0xdd { 1 } else { 2 };
                    extra += 4;
                } else {
                    break extra
                        + match op {
                            0xcb => self.cb(bus, idx),
                            0xed => self.ed(bus),
                            _ => self.decode(bus, op, idx),
                        };
                }
                // A whole address space of prefixes still yields control to the scheduler.
                if extra == 262144 {
                    break extra;
                }
            }
        };
        let cycles = cycles + self.fetch_wait_cycles;
        self.ei_delay = self.ei_delay.saturating_sub(1);
        self.tstates += cycles as u64;
        cycles
    }
    fn decode(&mut self, bus: &mut impl Bus, op: u8, idx: u8) -> u32 {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let p = y >> 1;
        let q = y & 1;
        let uses_memory = (x == 1 && op != 0x76 && (y == 6 || z == 6))
            || (x == 2 && z == 6)
            || (x == 0 && (4..=6).contains(&z) && y == 6);
        let address = if idx != 0 && uses_memory {
            let d = self.byte(bus) as i8;
            self.index(idx).wrapping_add_signed(d as i16)
        } else {
            self.index(idx)
        };
        let mem_extra = if idx != 0 && uses_memory {
            if x == 0 && z == 6 {
                5
            } else {
                8
            }
        } else {
            0
        };
        let cycles = match x {
            0 => match z {
                0 => match y {
                    0 => 4,
                    1 => {
                        std::mem::swap(&mut self.r[7], &mut self.alternate[7]);
                        std::mem::swap(&mut self.f, &mut self.alternate_f);
                        4
                    }
                    2 => {
                        let d = self.byte(bus);
                        self.r[0] = self.r[0].wrapping_sub(1);
                        if self.r[0] != 0 {
                            self.relative(d);
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        let d = self.byte(bus);
                        self.relative(d);
                        12
                    }
                    _ => {
                        let d = self.byte(bus);
                        if self.condition(y - 4) {
                            self.relative(d);
                            12
                        } else {
                            7
                        }
                    }
                },
                1 => {
                    if q == 0 {
                        let v = self.word(bus);
                        self.set_pair(p, idx, false, v);
                        10
                    } else {
                        let a = self.index(idx);
                        let v = self.pair(p, idx, false);
                        let r = a.wrapping_add(v);
                        self.f = (self.f & (S | Z | PV))
                            | ((r >> 8) as u8 & 0x28)
                            | (((a ^ v ^ r) >> 8) as u8 & H)
                            | if a as u32 + v as u32 > 65535 { C } else { 0 };
                        self.set_index(idx, r);
                        11
                    }
                }
                2 => {
                    let a = match p {
                        0 => self.bc(),
                        1 => self.de(),
                        _ => self.word(bus),
                    };
                    if q == 0 {
                        if p == 2 {
                            Self::write16(bus, a, self.index(idx));
                        } else {
                            bus.write(a, self.a());
                        }
                    } else if p == 2 {
                        let v = Self::read16(bus, a);
                        self.set_index(idx, v);
                    } else {
                        let v = bus.read(a);
                        self.set_a(v);
                    }
                    match p {
                        0 | 1 => 7,
                        2 => 16,
                        _ => 13,
                    }
                }
                3 => {
                    let v =
                        self.pair(p, idx, false)
                            .wrapping_add(if q == 0 { 1 } else { u16::MAX });
                    self.set_pair(p, idx, false, v);
                    6
                }
                4 | 5 => {
                    let v = self.reg(y, idx, address, bus);
                    let r = self.incdec(v, z == 5);
                    self.put(y, idx, address, r, bus);
                    if y == 6 {
                        11
                    } else {
                        4
                    }
                }
                6 => {
                    let v = self.byte(bus);
                    self.put(y, idx, address, v, bus);
                    if y == 6 {
                        10
                    } else {
                        7
                    }
                }
                _ => {
                    let a = self.a();
                    match y {
                        0..=3 => {
                            let c = self.f & C;
                            let (v, out) = match y {
                                0 => (a.rotate_left(1), a >> 7),
                                1 => (a.rotate_right(1), a & 1),
                                2 => ((a << 1) | c, a >> 7),
                                _ => ((a >> 1) | (c << 7), a & 1),
                            };
                            self.set_a(v);
                            self.f = (self.f & (S | Z | PV)) | (v & 0x28) | out;
                        }
                        4 => {
                            let mut correction = 0;
                            let mut carry = self.f & C;
                            if self.f & H != 0 || (self.f & N == 0 && a & 15 > 9) {
                                correction |= 6;
                            }
                            if carry != 0 || (self.f & N == 0 && a > 0x99) {
                                correction |= 0x60;
                                carry = C;
                            }
                            let r = if self.f & N != 0 {
                                a.wrapping_sub(correction)
                            } else {
                                a.wrapping_add(correction)
                            };
                            self.f = (self.f & N) | carry | ((a ^ r) & H) | szp(r);
                            self.set_a(r);
                        }
                        5 => {
                            self.set_a(!a);
                            self.f = (self.f & (S | Z | PV | C)) | H | N | (!a & 0x28);
                        }
                        6 => self.f = (self.f & (S | Z | PV)) | (a & 0x28) | C,
                        _ => {
                            self.f = (self.f & (S | Z | PV))
                                | (a & 0x28)
                                | if self.f & C != 0 { H } else { C }
                        }
                    };
                    4
                }
            },
            1 => {
                if op == 0x76 {
                    self.halted = true;
                    4
                } else {
                    let reg_idx = if y == 6 || z == 6 { 0 } else { idx };
                    let v = self.reg(z, reg_idx, address, bus);
                    self.put(y, reg_idx, address, v, bus);
                    if y == 6 || z == 6 {
                        7
                    } else {
                        4
                    }
                }
            }
            2 => {
                let v = self.reg(z, idx, address, bus);
                self.alu(y, v);
                if z == 6 {
                    7
                } else {
                    4
                }
            }
            _ => match z {
                0 => {
                    if self.condition(y) {
                        self.pc = self.pop(bus);
                        11
                    } else {
                        5
                    }
                }
                1 => {
                    if q == 0 {
                        let v = self.pop(bus);
                        self.set_pair(p, idx, true, v);
                        10
                    } else {
                        match p {
                            0 => {
                                self.pc = self.pop(bus);
                                10
                            }
                            1 => {
                                for r in 0..6 {
                                    std::mem::swap(&mut self.r[r], &mut self.alternate[r]);
                                }
                                4
                            }
                            2 => {
                                self.pc = self.index(idx);
                                4
                            }
                            _ => {
                                self.sp = self.index(idx);
                                6
                            }
                        }
                    }
                }
                2 => {
                    let a = self.word(bus);
                    if self.condition(y) {
                        self.pc = a;
                    }
                    10
                }
                3 => match y {
                    0 => {
                        self.pc = self.word(bus);
                        10
                    }
                    2 => {
                        let port = u16::from_be_bytes([self.a(), self.byte(bus)]);
                        bus.output(port, self.a());
                        11
                    }
                    3 => {
                        let port = u16::from_be_bytes([self.a(), self.byte(bus)]);
                        let v = bus.input(port);
                        self.set_a(v);
                        11
                    }
                    4 => {
                        let v = Self::read16(bus, self.sp);
                        let original = self.index(idx);
                        // Capture both reads before writing high byte, then low byte.
                        bus.write(self.sp.wrapping_add(1), (original >> 8) as u8);
                        bus.write(self.sp, original as u8);
                        self.set_index(idx, v);
                        19
                    }
                    5 => {
                        let de = self.de();
                        self.set_de(self.hl());
                        self.set_hl(de);
                        4
                    }
                    6 => {
                        self.iff1 = false;
                        self.iff2 = false;
                        4
                    }
                    7 => {
                        self.iff1 = true;
                        self.iff2 = true;
                        self.ei_delay = 2;
                        4
                    }
                    _ => unreachable!("prefix decoded by step"),
                },
                4 => {
                    let a = self.word(bus);
                    if self.condition(y) {
                        self.push(bus, self.pc);
                        self.pc = a;
                        17
                    } else {
                        10
                    }
                }
                5 => {
                    if q == 0 {
                        self.push(bus, self.pair(p, idx, true));
                        11
                    } else {
                        let a = self.word(bus);
                        self.push(bus, self.pc);
                        self.pc = a;
                        17
                    }
                }
                6 => {
                    let v = self.byte(bus);
                    self.alu(y, v);
                    7
                }
                _ => {
                    self.push(bus, self.pc);
                    self.pc = (y as u16) * 8;
                    11
                }
            },
        };
        cycles + mem_extra
    }
    fn cb(&mut self, bus: &mut impl Bus, idx: u8) -> u32 {
        let address = if idx == 0 {
            self.hl()
        } else {
            let d = self.byte(bus) as i8;
            self.index(idx).wrapping_add_signed(d as i16)
        };
        let op = if idx == 0 {
            self.opcode(bus)
        } else {
            self.byte(bus)
        };
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let v = self.reg(if idx == 0 { z } else { 6 }, 0, address, bus);
        let r = match x {
            0 => {
                let carry = self.f & C;
                let (r, c) = match y {
                    0 => (v.rotate_left(1), v >> 7),
                    1 => (v.rotate_right(1), v & 1),
                    2 => ((v << 1) | carry, v >> 7),
                    3 => ((v >> 1) | (carry << 7), v & 1),
                    4 => (v << 1, v >> 7),
                    5 => ((v >> 1) | (v & 0x80), v & 1),
                    6 => ((v << 1) | 1, v >> 7),
                    _ => (v >> 1, v & 1),
                };
                self.f = szp(r) | c;
                r
            }
            1 => {
                let zero = v & (1 << y) == 0;
                self.f = (self.f & C)
                    | H
                    | if zero { Z | PV } else { 0 }
                    | if y == 7 && !zero { S } else { 0 }
                    | if idx != 0 {
                        (address >> 8) as u8 & 0x28
                    } else {
                        v & 0x28
                    };
                v
            }
            2 => v & !(1 << y),
            _ => v | (1 << y),
        };
        if x != 1 {
            if idx != 0 {
                bus.write(address, r);
                if z != 6 {
                    self.put(z, 0, address, r, bus);
                }
            } else {
                self.put(z, 0, address, r, bus);
            }
        }
        if idx != 0 {
            if x == 1 {
                16
            } else {
                19
            }
        } else if z == 6 {
            if x == 1 {
                12
            } else {
                15
            }
        } else {
            8
        }
    }
    fn ed(&mut self, bus: &mut impl Bus) -> u32 {
        let op = self.opcode(bus);
        let y = (op >> 3) & 7;
        let p = y >> 1;
        let q = y & 1;
        if op >> 6 == 1 {
            match op & 7 {
                0 => {
                    let v = bus.input(self.bc());
                    if y != 6 {
                        self.r[y as usize] = v;
                    }
                    self.f = (self.f & C) | szp(v);
                    12
                }
                1 => {
                    bus.output(self.bc(), if y == 6 { 0 } else { self.r[y as usize] });
                    12
                }
                2 => {
                    let a = self.hl();
                    let v = self.pair(p, 0, false);
                    let c = (self.f & C) as u32;
                    let wide = if q == 0 {
                        (a as u32).wrapping_sub(v as u32 + c)
                    } else {
                        a as u32 + v as u32 + c
                    };
                    let r = wide as u16;
                    self.f = ((r >> 8) as u8 & 0xa8)
                        | if r == 0 { Z } else { 0 }
                        | (((a ^ v ^ r) >> 8) as u8 & H)
                        | if wide > 65535 { C } else { 0 }
                        | if q == 0 { N } else { 0 }
                        | if (if q == 0 { a ^ v } else { !(a ^ v) }) & (a ^ r) & 0x8000 != 0 {
                            PV
                        } else {
                            0
                        };
                    self.set_hl(r);
                    15
                }
                3 => {
                    let a = self.word(bus);
                    if q == 0 {
                        Self::write16(bus, a, self.pair(p, 0, false));
                    } else {
                        let v = Self::read16(bus, a);
                        self.set_pair(p, 0, false, v);
                    }
                    20
                }
                4 => {
                    let v = self.a();
                    self.set_a(0);
                    self.alu(2, v);
                    8
                }
                5 => {
                    self.pc = self.pop(bus);
                    self.iff1 = self.iff2;
                    14
                }
                6 => {
                    self.im = match y {
                        2 | 6 => 1,
                        3 | 7 => 2,
                        _ => 0,
                    };
                    8
                }
                _ => {
                    match y {
                        0 => self.i = self.a(),
                        1 => self.refresh = self.a(),
                        2 | 3 => {
                            let v = if y == 2 { self.i } else { self.refresh };
                            self.set_a(v);
                            self.f = (self.f & C) | sz(v) | if self.iff2 { PV } else { 0 };
                        }
                        4 | 5 => {
                            let v = bus.read(self.hl());
                            let a = self.a();
                            let (mem, acc) = if y == 4 {
                                ((a << 4) | (v >> 4), (a & 0xf0) | (v & 15))
                            } else {
                                ((v << 4) | (a & 15), (a & 0xf0) | (v >> 4))
                            };
                            bus.write(self.hl(), mem);
                            self.set_a(acc);
                            self.f = (self.f & C) | szp(acc);
                        }
                        _ => return 8,
                    };
                    if y >= 4 {
                        18
                    } else {
                        9
                    }
                }
            }
        } else if op & 0xe4 == 0xa0 {
            let kind = op & 3;
            let decrement = op & 8 != 0;
            let delta = if decrement { u16::MAX } else { 1 };
            let repeat = op & 0x10 != 0;
            let v = if kind == 2 {
                bus.input(self.bc())
            } else {
                bus.read(self.hl())
            };
            match kind {
                0 => {
                    bus.write(self.de(), v);
                    self.set_de(self.de().wrapping_add(delta));
                    self.set_bc(self.bc().wrapping_sub(1));
                    let sum = self.a().wrapping_add(v);
                    self.f = (self.f & (S | Z | C))
                        | if self.bc() != 0 { PV } else { 0 }
                        | (sum & 8)
                        | ((sum & 2) << 4);
                }
                1 => {
                    let r = self.a().wrapping_sub(v);
                    let half = (self.a() ^ v ^ r) & H;
                    let adj = r.wrapping_sub(u8::from(half != 0));
                    self.set_bc(self.bc().wrapping_sub(1));
                    self.f = (self.f & C)
                        | (sz(r) & !0x28)
                        | half
                        | N
                        | if self.bc() != 0 { PV } else { 0 }
                        | (adj & 8)
                        | ((adj & 2) << 4);
                }
                _ => {
                    self.r[0] = self.r[0].wrapping_sub(1);
                    if kind == 2 {
                        bus.write(self.hl(), v);
                    } else {
                        bus.output(self.bc(), v);
                    }
                    let other = if kind == 2 {
                        self.r[1].wrapping_add(if decrement { 255 } else { 1 })
                    } else {
                        self.hl().wrapping_add(delta) as u8
                    };
                    let sum = v as u16 + other as u16;
                    self.f = sz(self.r[0])
                        | if v & 128 != 0 { N } else { 0 }
                        | if sum > 255 { H | C } else { 0 }
                        | (szp(((sum as u8) & 7) ^ self.r[0]) & PV);
                }
            }
            self.set_hl(self.hl().wrapping_add(delta));
            let again = if kind < 2 {
                self.bc() != 0 && (kind == 0 || self.f & Z == 0)
            } else {
                self.r[0] != 0
            };
            if repeat && again {
                self.pc = self.pc.wrapping_sub(2);
                21
            } else {
                16
            }
        } else {
            8
        }
    }
}
