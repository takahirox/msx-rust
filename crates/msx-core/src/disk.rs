//! Original raw DSK drive and deterministic WD2793 subset. See docs/disks.md.
pub const SECTOR_SIZE: usize = 512;
pub const BYTE_CYCLES: u32 = 114;
pub const SERVICE_TIMEOUT: u32 = 4096;
const SEEK_CYCLES: u32 = 21_478;

#[derive(Debug)]
pub struct Disk {
    bytes: Vec<u8>,
    sides: u8,
    pub write_protected: bool,
    pub dirty: bool,
    pub changed: bool,
}
impl Disk {
    pub fn new(bytes: Vec<u8>, write_protected: bool) -> Result<Self, String> {
        let sides = match bytes.len() {
            368_640 => 1,
            737_280 => 2,
            _ => return Err("Unsupported DSK: use raw 360 or 720 KiB (80 tracks, 1/2 sides, 9 sectors/track, 512 bytes/sector). DMK/FDI and truncated images are unsupported.".into()),
        };
        Ok(Self {
            bytes,
            sides,
            write_protected,
            dirty: false,
            changed: true,
        })
    }
    pub fn sides(&self) -> u8 {
        self.sides
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn offset(&self, track: u8, side: u8, sector: u8) -> Option<usize> {
        if track >= 80 || side >= self.sides || !(1..=9).contains(&sector) {
            return None;
        }
        Some(
            ((track as usize * self.sides as usize + side as usize) * 9 + sector as usize - 1)
                * SECTOR_SIZE,
        )
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Transfer {
    #[default]
    Idle,
    Seek,
    Read,
    Write,
}

pub struct Controller {
    pub disk: Option<Disk>,
    pub track: u8,
    pub sector: u8,
    pub data: u8,
    pub side: u8,
    pub control: u8,
    pub irq: bool,
    pub drq: bool,
    side_latch: u8,
    drive_latch: u8,
    empty_changed: bool,
    status: u8,
    physical_track: u8,
    target: u8,
    track_target: Option<u8>,
    direction_in: bool,
    type_one: bool,
    verify: bool,
    transfer: Transfer,
    delay: u32,
    deadline: u32,
    index: usize,
    offset: usize,
    buffer: [u8; SECTOR_SIZE],
    multi: bool,
    force_mask: u8,
    rotation: u32,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            disk: None,
            track: 0,
            sector: 1,
            data: 0,
            side: 0,
            control: 0,
            irq: false,
            drq: false,
            side_latch: 0,
            drive_latch: 0,
            empty_changed: false,
            status: 0,
            physical_track: 0,
            target: 0,
            track_target: None,
            direction_in: true,
            type_one: true,
            verify: false,
            transfer: Transfer::Idle,
            delay: 0,
            deadline: 0,
            index: 0,
            offset: 0,
            buffer: [0; SECTOR_SIZE],
            multi: false,
            force_mask: 0,
            rotation: 0,
        }
    }
}
impl Controller {
    pub fn reset(&mut self) {
        let disk = self.disk.take();
        let empty_changed = self.empty_changed;
        *self = Self {
            disk,
            empty_changed,
            sector: 1,
            ..Self::default()
        };
    }
    pub fn mount(&mut self, disk: Disk) -> Result<(), String> {
        if self.disk.as_ref().is_some_and(|d| d.dirty) {
            return Err("Export modified DSK before replacing this disk.".into());
        }
        self.disk = Some(disk);
        self.finish(0);
        Ok(())
    }
    pub fn eject(&mut self) -> Result<(), String> {
        if self.disk.as_ref().is_some_and(|d| d.dirty) {
            return Err("Export modified DSK before ejecting this disk.".into());
        }
        self.disk = None;
        self.empty_changed = true;
        self.finish(0x80);
        Ok(())
    }
    /// Philips NMS1200-style latches/readback, selected from the pinned openMSX
    /// profile. Only Hardware's page-1 slot decoder may call these methods.
    pub fn read_philips(&mut self, register: u8) -> u8 {
        match register {
            0..=3 => self.read(register),
            4 => self.side_latch,
            5 => {
                // Absent B and no-drive use the upstream dummy drive (unchanged).
                let changed = if self.drive_latch & 1 == 0 {
                    if let Some(disk) = &mut self.disk {
                        std::mem::take(&mut disk.changed)
                    } else {
                        std::mem::take(&mut self.empty_changed)
                    }
                } else {
                    false
                };
                (self.drive_latch & !4) | if changed { 0 } else { 4 }
            }
            7 => !(if self.irq { 0x40 } else { 0 }) & !(if self.drq { 0x80 } else { 0 }),
            _ => 0xff,
        }
    }
    pub fn write_philips(&mut self, register: u8, value: u8) {
        match register {
            0..=3 => self.write(register, value),
            4 => {
                self.side_latch = value;
                self.write(4, value);
            }
            5 => {
                self.drive_latch = value;
                let selected = match value & 3 {
                    0 | 2 => 1, // A
                    1 => 2,     // absent B
                    _ => 0,     // no drive
                };
                self.write(5, (value & 0x80) | selected);
            }
            _ => {}
        }
    }
    fn ready(&self) -> bool {
        self.disk.is_some() && self.control & 0x83 == 0x81
    }
    pub fn read(&mut self, register: u8) -> u8 {
        match register {
            0 => {
                self.irq = false;
                let mut s = self.status | if self.ready() { 0 } else { 0x80 };
                if self.type_one {
                    s &= !6;
                    if self.physical_track == 0 {
                        s |= 4;
                    }
                    if self.rotation < 7000 && self.ready() {
                        s |= 2;
                    }
                    if self.disk.as_ref().is_some_and(|d| d.write_protected) {
                        s |= 0x40;
                    }
                } else if self.drq {
                    s |= 2;
                }
                s
            }
            1 => self.track,
            2 => self.sector,
            3 => {
                if self.transfer == Transfer::Read && self.drq {
                    self.data = self.buffer[self.index];
                    self.advance();
                }
                self.data
            }
            4 => self.side,
            5 => self.control,
            // Profile extension: media geometry/change, read acknowledges change.
            6 => self.disk.as_mut().map_or(0, |d| {
                let v = 0x80 | (d.sides - 1) | if d.changed { 2 } else { 0 };
                d.changed = false;
                v
            }),
            7 => (if self.irq { 0x80 } else { 0 }) | if self.drq { 0x40 } else { 0 },
            _ => 255,
        }
    }
    pub fn write(&mut self, register: u8, value: u8) {
        match register {
            0 => self.command(value),
            1 if self.transfer == Transfer::Idle => self.track = value,
            2 if self.transfer == Transfer::Idle => self.sector = value,
            3 => {
                self.data = value;
                if self.transfer == Transfer::Write && self.drq {
                    self.buffer[self.index] = value;
                    self.advance();
                }
            }
            4 | 5 => {
                let was_ready = self.ready();
                let changed = if register == 4 {
                    self.side != value & 1
                } else {
                    self.control != value
                };
                if register == 4 {
                    self.side = value & 1;
                } else {
                    self.control = value;
                }
                if changed && self.transfer != Transfer::Idle {
                    self.finish(0x80);
                }
                if (was_ready && !self.ready() && self.force_mask & 2 != 0)
                    || (!was_ready && self.ready() && self.force_mask & 1 != 0)
                {
                    self.irq = true;
                }
            }
            _ => {}
        }
    }
    fn finish(&mut self, error: u8) {
        self.status = error;
        self.transfer = Transfer::Idle;
        self.drq = false;
        self.irq = true;
    }
    fn command(&mut self, cmd: u8) {
        if cmd & 0xf0 == 0xd0 {
            self.finish(0);
            self.force_mask = cmd & 7;
            self.irq = cmd & 8 != 0;
            return;
        }
        if self.transfer != Transfer::Idle {
            return;
        }
        self.irq = false;
        self.drq = false;
        self.force_mask = 0;
        self.type_one = cmd & 0x80 == 0;
        self.status = 1;
        if !self.ready() {
            self.finish(0x80);
            return;
        }
        if self.type_one {
            self.verify = cmd & 4 != 0;
            self.target = match cmd & 0xe0 {
                0 => {
                    if cmd & 0x10 == 0 {
                        self.direction_in = false;
                        0
                    } else {
                        // SEEK emits steps until Track equals Data; it cannot
                        // discover or repair a pre-existing physical head offset.
                        if self.data != self.track {
                            self.direction_in = self.data > self.track;
                        }
                        if self.data >= self.track {
                            self.physical_track.saturating_add(self.data - self.track)
                        } else {
                            self.physical_track.saturating_sub(self.track - self.data)
                        }
                    }
                }
                0x20 => {
                    if self.direction_in {
                        self.physical_track.saturating_add(1)
                    } else {
                        self.physical_track.saturating_sub(1)
                    }
                }
                0x40 => {
                    self.direction_in = true;
                    self.physical_track.saturating_add(1)
                }
                _ => {
                    self.direction_in = false;
                    self.physical_track.saturating_sub(1)
                }
            };
            self.track_target = match cmd & 0xe0 {
                0 if cmd & 0x10 == 0 => Some(0), // restore uses the track-zero sensor
                0 => Some(self.data),
                _ if cmd & 0x10 == 0 => None,
                // Updating steps change the register independently of the head.
                _ if self.direction_in => Some(self.track.wrapping_add(1)),
                _ => Some(self.track.wrapping_sub(1)),
            };
            self.transfer = Transfer::Seek;
            self.delay = SEEK_CYCLES * [1, 2, 3, 5][(cmd & 3) as usize];
        } else {
            match cmd & 0xe0 {
                0x80 | 0xa0 => {
                    self.transfer = if cmd & 0x20 == 0 {
                        Transfer::Read
                    } else {
                        Transfer::Write
                    };
                    self.multi = cmd & 0x10 != 0;
                    // Side compare, if requested, compares C with selected side.
                    if cmd & 2 != 0 && (cmd >> 3) & 1 != self.side {
                        self.finish(0x10);
                        return;
                    }
                    self.start_sector();
                }
                _ => self.finish(0x10), // Read address/track and format are outside raw-sector profile.
            }
        }
    }
    fn start_sector(&mut self) {
        let Some(disk) = &self.disk else {
            self.finish(0x80);
            return;
        };
        if self.track != self.physical_track {
            self.finish(0x10);
            return;
        }
        let Some(offset) = disk.offset(self.track, self.side, self.sector) else {
            self.finish(0x10);
            return;
        };
        if self.transfer == Transfer::Write && disk.write_protected {
            self.finish(0x40);
            return;
        }
        self.offset = offset;
        self.buffer
            .copy_from_slice(&disk.bytes[offset..offset + SECTOR_SIZE]);
        self.index = 0;
        self.delay = BYTE_CYCLES;
        self.status = 1;
    }
    fn advance(&mut self) {
        self.drq = false;
        self.index += 1;
        if self.index == SECTOR_SIZE {
            if self.transfer == Transfer::Write {
                let Some(d) = &mut self.disk else {
                    self.finish(0x80);
                    return;
                };
                if d.write_protected {
                    self.finish(0x40);
                    return;
                }
                let Some(dest) = d.bytes.get_mut(self.offset..self.offset + SECTOR_SIZE) else {
                    self.finish(0x10);
                    return;
                };
                if dest != self.buffer {
                    dest.copy_from_slice(&self.buffer);
                    d.dirty = true;
                }
            }
            if self.multi {
                self.sector = self.sector.wrapping_add(1);
                self.start_sector();
            } else {
                self.finish(0);
            }
        } else {
            self.delay = BYTE_CYCLES;
        }
    }
    pub fn tick(&mut self, cycles: u32) {
        let old_rotation = self.rotation;
        self.rotation = ((u64::from(self.rotation) + u64::from(cycles)) % 715_909) as u32; // 300 rpm at Z80 clock
        if self.force_mask & 4 != 0 && self.rotation < old_rotation && self.ready() {
            self.irq = true;
        }
        if self.transfer == Transfer::Idle {
            return;
        }
        if !self.ready() {
            self.finish(0x80);
            return;
        }
        if self.drq {
            self.deadline = self.deadline.saturating_sub(cycles);
            if self.deadline == 0 {
                self.finish(4);
            }
        } else {
            self.delay = self.delay.saturating_sub(cycles);
            if self.delay == 0 {
                if self.transfer == Transfer::Seek {
                    if self.target >= 80 {
                        self.finish(0x10);
                    } else {
                        self.physical_track = self.target;
                        if let Some(track) = self.track_target {
                            self.track = track;
                        }
                        let verified = self.track == self.physical_track
                            && self.disk.as_ref().is_some_and(|disk| {
                                disk.offset(self.physical_track, self.side, 1).is_some()
                            });
                        self.finish(if self.verify && !verified { 0x10 } else { 0 });
                    }
                } else {
                    self.drq = true;
                    self.deadline = SERVICE_TIMEOUT;
                }
            }
        }
    }
}
