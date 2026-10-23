//! Independent implementation of the MSX2 Technical Handbook command interface.
//! A work unit completes every eight CPU T-states. This is deterministic, not
//! cycle-exact V9938 arbitration; CPU transfers additionally wait for TR.
use super::Vdp;

pub(super) struct Command {
    code: u8,
    op: u8,
    arg: u8,
    sx: i32,
    sy: i32,
    dx: i32,
    dy: i32,
    nx: usize,
    ny: usize,
    col: usize,
    row: usize,
    error: i32,
    color: u8,
    unit: usize,
    width: usize,
    pending: Option<u8>,
}
impl Command {
    fn step_x(&self) -> i32 {
        if self.arg & 4 != 0 {
            -1
        } else {
            1
        }
    }
    fn step_y(&self) -> i32 {
        if self.arg & 8 != 0 {
            -1
        } else {
            1
        }
    }
}
impl Vdp {
    pub(super) fn start_command(&mut self, value: u8) {
        self.command = None;
        self.command_cycles = 0;
        self.statuses[2] &= !0x81;
        let code = value >> 4;
        if code == 6 {
            self.statuses[2] &= !0x10;
        }
        if !self.msx2 || !(5..=8).contains(&self.mode()) || code < 4 {
            return;
        }
        let word = |r: usize, mask: u8| {
            (self.registers[r] as usize | ((self.registers[r + 1] & mask) as usize) << 8) as i32
        };
        let (width, bits) = self.bitmap_layout();
        let unit = if code >= 12 { 8 / bits } else { 1 };
        let sx = word(32, 1) / unit as i32 * unit as i32;
        let dx = word(36, 1) / unit as i32 * unit as i32;
        let nx = word(40, 3) as usize;
        let nx = if nx == 0 && code != 7 { 1024 } else { nx };
        let ny = word(42, 3) as usize;
        let mut c = Command {
            code,
            op: value & 15,
            arg: self.registers[45],
            sx,
            sy: word(34, 3),
            dx,
            dy: word(38, 3),
            nx: if code == 7 { nx } else { (nx / unit).max(1) },
            ny: if ny == 0 && code != 7 { 1024 } else { ny },
            col: 0,
            row: 0,
            // LINE resolves half-step ties toward the minor-axis advance.
            // NX=0 still draws one pixel without a minor-axis step.
            error: if nx == 0 { 1023 } else { ((nx - 1) / 2) as i32 },
            color: self.registers[44],
            unit,
            width,
            pending: None,
        };
        if code == 14 {
            c.sx = c.dx;
            c.nx = if c.step_x() > 0 {
                (width.saturating_sub(dx as usize)) / unit
            } else {
                dx as usize / unit + 1
            };
        }
        if code == 11 || code == 15 {
            c.pending = Some(c.color);
        }
        self.command = Some(c);
        self.statuses[2] |= 1;
    }
    pub(super) fn command_write(&mut self, value: u8) {
        if let Some(c) = &mut self.command {
            // Drawing/search consume the current color on their next work unit.
            // Transfer data remains separately latched and gated by TR below.
            if matches!(c.code, 5..=8 | 12) {
                c.color = value;
            }
            if matches!(c.code, 11 | 15) && self.statuses[2] & 0x80 != 0 {
                c.pending = Some(value);
                self.statuses[2] &= !0x80;
            }
        }
    }
    pub(super) fn command_read(&mut self) -> u8 {
        let result = self.statuses[7];
        if self.command.as_ref().is_some_and(|c| c.code == 10) && self.statuses[2] & 0x80 != 0 {
            self.statuses[2] &= !0x80;
            let mut c = self.command.take().unwrap();
            if !self.next_rectangle(&mut c) {
                self.command = Some(c);
            } else {
                self.finish_command(&c);
            }
        }
        result
    }
    fn finish_command(&mut self, c: &Command) {
        self.statuses[2] &= !0x81;
        self.registers[46] &= 15;
        if c.code == 7 {
            self.registers[38] = c.dy as u8;
            self.registers[39] = ((c.dy >> 8) & 3) as u8;
        }
        if c.code >= 8 {
            let dy = (c.dy + c.row as i32 * c.step_y()) as u16;
            if c.code != 10 {
                self.registers[38] = dy as u8;
                self.registers[39] = ((dy >> 8) & 3) as u8;
            }
            if matches!(c.code, 9 | 10 | 13 | 14) {
                let sy = (c.sy + c.row as i32 * c.step_y()) as u16;
                self.registers[34] = sy as u8;
                self.registers[35] = ((sy >> 8) & 3) as u8;
            }
            let ny = c.ny.saturating_sub(c.row) as u16;
            self.registers[42] = ny as u8;
            self.registers[43] = ((ny >> 8) & 3) as u8;
        }
    }
    fn next_rectangle(&self, c: &mut Command) -> bool {
        c.col += 1;
        let next_dx = c.dx + c.col as i32 * c.step_x() * c.unit as i32;
        let next_sx = c.sx + c.col as i32 * c.step_x() * c.unit as i32;
        let clipped = (c.code != 10 && !(0..c.width as i32).contains(&next_dx))
            || (matches!(c.code, 9 | 10 | 13 | 14) && !(0..c.width as i32).contains(&next_sx));
        if c.col >= c.nx || clipped {
            c.col = 0;
            c.row += 1;
            // Include row zero, then stop before any negative Y is masked
            // into wrapped VRAM. Forward Y retains ten-bit/physical wrapping.
            if c.step_y() < 0 {
                let rows = c.row as i32;
                if (c.code != 10 && rows > c.dy)
                    || (matches!(c.code, 9 | 10 | 13 | 14) && rows > c.sy)
                {
                    return true;
                }
            }
        }
        c.row >= c.ny
    }
    pub(super) fn tick_command(&mut self, cycles: u32) {
        if self.command.is_none() {
            return;
        }
        self.command_cycles += cycles;
        while self.command_cycles >= 8 {
            self.command_cycles -= 8;
            let Some(mut c) = self.command.take() else {
                break;
            };
            if self.statuses[2] & 0x80 != 0 {
                self.command = Some(c);
                self.command_cycles = 0;
                break;
            }
            if matches!(c.code, 11 | 15) && c.pending.is_none() {
                self.statuses[2] |= 0x80;
                self.command = Some(c);
                break;
            }
            let sx = c.sx + c.col as i32 * c.step_x() * c.unit as i32;
            let sy = (c.sy + c.row as i32 * c.step_y()) & 1023;
            let dx = c.dx + c.col as i32 * c.step_x() * c.unit as i32;
            let dy = (c.dy + c.row as i32 * c.step_y()) & 1023;
            let mut done = false;
            // External 64 KiB expansion VRAM is not installed in this profile.
            let source = if c.arg & 0x10 != 0 {
                255
            } else {
                self.point(sx as usize, sy as usize)
            };
            match c.code {
                4 => {
                    self.statuses[7] = source;
                    self.registers[44] = source;
                    done = true;
                }
                5 => {
                    if c.arg & 0x20 == 0 {
                        self.pset(dx as usize, dy as usize, c.color, c.op);
                    }
                    done = true;
                }
                6 => {
                    let matched = (source == c.color) != (c.arg & 2 != 0);
                    if matched {
                        self.statuses[2] |= 0x10;
                    }
                    let next = sx + c.step_x();
                    done = matched || !(0..c.width as i32).contains(&next);
                    let result = if matched { sx } else { next };
                    self.statuses[8] = result as u8;
                    self.statuses[9] = ((result >> 8) & 1) as u8;
                    c.col += 1;
                }
                7 => {
                    if c.arg & 0x20 == 0 {
                        self.pset(c.dx as usize, (c.dy & 1023) as usize, c.color, c.op);
                    }
                    if c.arg & 1 == 0 {
                        c.dx += c.step_x();
                        // X-major tests completion before the minor Y advance.
                        done = c.col >= c.nx || !(0..c.width as i32).contains(&c.dx);
                        if !done {
                            if c.error < c.ny as i32 {
                                c.error += c.nx as i32;
                                c.dy += c.step_y();
                            }
                            c.error = (c.error - c.ny as i32) & 1023;
                            done = c.dy < 0;
                        }
                    } else {
                        // Y-major advances Y even after the final pixel.
                        c.dy += c.step_y();
                        done = c.dy < 0;
                        if !done {
                            if c.error < c.ny as i32 {
                                c.error += c.nx as i32;
                                c.dx += c.step_x();
                            }
                            c.error = (c.error - c.ny as i32) & 1023;
                            done = c.col >= c.nx || !(0..c.width as i32).contains(&c.dx);
                        }
                    }
                    c.col += 1;
                }
                8 | 9 | 11 => {
                    let value = match c.code {
                        9 => source,
                        11 => c.pending.take().unwrap(),
                        _ => c.color,
                    };
                    if c.arg & 0x20 == 0 {
                        self.pset(dx as usize, dy as usize, value, c.op);
                    }
                    done = self.next_rectangle(&mut c);
                    if c.code == 11 && !done {
                        self.statuses[2] |= 0x80;
                    }
                }
                10 => {
                    self.statuses[7] = source;
                    self.registers[44] = source;
                    self.statuses[2] |= 0x80;
                }
                12..=15 => {
                    let (dest, _, _) = self.bitmap_address(dx as usize, dy as usize);
                    let (src, _, _) = self.bitmap_address(sx as usize, sy as usize);
                    let value = match c.code {
                        13 | 14 => {
                            if c.arg & 0x10 != 0 {
                                255
                            } else {
                                self.at(src)
                            }
                        }
                        15 => c.pending.take().unwrap(),
                        _ => c.color,
                    };
                    if c.arg & 0x20 == 0 {
                        self.set_byte(dest, value);
                    }
                    done = self.next_rectangle(&mut c);
                    if c.code == 15 && !done {
                        self.statuses[2] |= 0x80;
                    }
                }
                _ => done = true,
            }
            if done {
                self.finish_command(&c);
            } else {
                self.command = Some(c);
            }
        }
    }
}
