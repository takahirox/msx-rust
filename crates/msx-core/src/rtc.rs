//! RP-5C01 compatible nibble registers. Deterministic emulated time, no host clock
//! or persistence. Alarm output is unwired, as in the MSX2 standard profile.
pub struct Rtc {
    blocks: [[u8; 13]; 4],
    control: [u8; 3],
    pub selected: u8,
    cycles: u32,
}
impl Default for Rtc {
    fn default() -> Self {
        let mut blocks = [[0; 13]; 4];
        blocks[0][7] = 1; // 1980-01-01 00:00:00, Tuesday
        blocks[0][9] = 1;
        blocks[0][6] = 2;
        blocks[1][10] = 1; // 24-hour clock
        Self {
            blocks,
            control: [8, 0, 0],
            selected: 0,
            cycles: 0,
        }
    }
}
impl Rtc {
    pub fn read(&self) -> u8 {
        let r = (self.selected & 15) as usize;
        0xf0 | if r >= 13 {
            self.control[r - 13]
        } else {
            self.blocks[(self.control[0] & 3) as usize][r]
        }
    }
    pub fn write(&mut self, v: u8) {
        let r = (self.selected & 15) as usize;
        if r >= 13 {
            self.control[r - 13] = v & 15;
            if r == 15 && v & 2 != 0 {
                self.cycles = 0;
            }
        } else {
            self.blocks[(self.control[0] & 3) as usize][r] = v & 15;
        }
    }
    fn number(&self, r: usize) -> u8 {
        self.blocks[0][r] + self.blocks[0][r + 1] * 10
    }
    fn set(&mut self, r: usize, v: u8) {
        self.blocks[0][r] = v % 10;
        self.blocks[0][r + 1] = v / 10;
    }
    pub fn tick(&mut self, cycles: u32) {
        if self.control[0] & 8 == 0 {
            return;
        }
        self.cycles += cycles;
        while self.cycles >= crate::psg::CPU_HZ {
            self.cycles -= crate::psg::CPU_HZ;
            let sec = self.number(0) + 1;
            self.set(0, sec % 60);
            if sec < 60 {
                continue;
            }
            let min = self.number(2) + 1;
            self.set(2, min % 60);
            if min < 60 {
                continue;
            }
            let hours = self.number(4);
            let hour = if self.blocks[1][10] & 1 != 0 {
                hours
            } else {
                (hours % 20) % 12 + if self.blocks[0][5] & 2 != 0 { 12 } else { 0 }
            };
            let h = (hour + 1) % 24;
            if self.blocks[1][10] & 1 != 0 {
                self.set(4, h);
            } else {
                let h12 = h % 12;
                self.set(4, h12);
                self.blocks[0][5] |= if h >= 12 { 2 } else { 0 };
            }
            if h != 0 {
                continue;
            }
            self.blocks[0][6] = (self.blocks[0][6] + 1) % 7;
            let year = self.number(11);
            let month = self.number(9).clamp(1, 12);
            let days = [
                31,
                if year.is_multiple_of(4) { 29 } else { 28 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ][month as usize - 1];
            let day = self.number(7) + 1;
            if day <= days {
                self.set(7, day);
                continue;
            }
            self.set(7, 1);
            self.set(9, month % 12 + 1);
            if month == 12 {
                self.set(11, (year + 1) % 100);
                self.blocks[1][11] = (year + 1) % 4;
            }
        }
    }
}
