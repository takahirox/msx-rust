//! Original raw DSK drive and deterministic WD2793 subset. See docs/disks.md.
pub const SECTOR_SIZE: usize = 512;
// Exact 32 us MFM bytes at 3,579,545 Hz, in 1/100000 CPU cycles.
const CLOCK_SCALE: u64 = 100_000;
const BYTE_TIME: u64 = 11_454_544;
// 1 MHz WD2793 rate profile, measured against the 3,579,545 Hz CPU clock.
// Deadlines stay in 1/100000 CPU-cycle units; tick observes ceil(n * interval),
// avoiding accumulated per-step rounding and tick-partition dependence.
const STEP_MS: [u64; 4] = [6, 12, 20, 30];
const REVOLUTION: u64 = 71_590_900_000; // 1/100000 CPU-cycle units, 300 rpm
const SETTLE: u64 = 3_579_545 * 30 * 100; // 30 ms, HLT assumed high

fn id_phase(id: u64) -> u64 {
    // Nine normal-track IDs, cylinder = physical head, side = selected side,
    // sector = id + 1, size = 512, good CRC. See docs/disks.md for ordering.
    (id * 715_909).div_ceil(9) * CLOCK_SCALE
}

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
    Settle,
    Verify,
    SectorSettle,
    SectorSearch,
    Read,
    WriteRequest,
    WriteCheck,
    WriteLeadIn,
    Write,
    ReadTail,
    SectorEnd,
}

#[derive(Clone, Copy, Debug, Default)]
enum Movement {
    #[default]
    Restore,
    Seek {
        target: u8,
    },
    Step {
        update: bool,
    },
    Complete,
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
    movement: Movement,
    clock: u64,
    restore_pulses: u8,
    search_indexes: u8,
    step_interval: u64,
    next_event: u64,
    direction_in: bool,
    type_one: bool,
    verify: bool,
    transfer: Transfer,
    write_sector: bool,
    compare_side: Option<u8>,
    index: usize,
    supplied: Option<u8>,
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
            movement: Movement::default(),
            clock: 0,
            restore_pulses: 0,
            search_indexes: 0,
            step_interval: 0,
            next_event: 0,
            direction_in: true,
            type_one: true,
            verify: false,
            transfer: Transfer::Idle,
            write_sector: false,
            compare_side: None,
            index: 0,
            supplied: None,
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
    fn selected_a(&self) -> bool {
        self.control & 3 == 1
    }
    fn track_zero(&self) -> bool {
        // Installed A has a motor-independent zero sensor in this profile.
        self.selected_a() && self.physical_track == 0
    }
    fn ready(&self) -> bool {
        self.disk.is_some() && self.control & 0x83 == 0x81
    }
    pub fn read(&mut self, register: u8) -> u8 {
        match register {
            0 => {
                self.irq = false;
                let mut s = (self.status & !0x80) | if self.ready() { 0 } else { 0x80 };
                if self.type_one {
                    s &= !6;
                    if self.track_zero() {
                        s |= 4;
                    }
                    if self.rotation < 7000 && self.ready() {
                        s |= 2;
                    }
                    if self.selected_a() && self.disk.as_ref().is_some_and(|d| d.write_protected) {
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
                if matches!(
                    self.transfer,
                    Transfer::Read | Transfer::ReadTail | Transfer::SectorEnd
                ) && !self.write_sector
                {
                    self.drq = false;
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
                if self.drq && self.write_sector {
                    self.supplied = Some(value);
                    self.drq = false;
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
                if changed
                    && matches!(
                        self.transfer,
                        Transfer::SectorSettle
                            | Transfer::SectorSearch
                            | Transfer::Read
                            | Transfer::ReadTail
                            | Transfer::WriteRequest
                            | Transfer::WriteCheck
                            | Transfer::WriteLeadIn
                            | Transfer::Write
                            | Transfer::SectorEnd
                    )
                {
                    self.finish(0x80);
                }
                if changed && self.transfer == Transfer::Verify {
                    self.schedule_search(false);
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
        self.movement = Movement::Complete;
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
        if !self.type_one && !self.ready() {
            self.finish(0x80);
            return;
        }
        if self.type_one {
            self.verify = cmd & 4 != 0;
            self.movement = match cmd & 0xf0 {
                0x00 => {
                    self.direction_in = false;
                    self.track = 0xff;
                    self.data = 0;
                    Movement::Restore
                }
                0x10 => Movement::Seek { target: self.data },
                _ => {
                    match cmd & 0xe0 {
                        0x40 => self.direction_in = true,
                        0x60 => self.direction_in = false,
                        _ => {} // STEP retains the preceding direction.
                    }
                    Movement::Step {
                        update: cmd & 0x10 != 0,
                    }
                }
            };
            self.transfer = Transfer::Seek;
            self.restore_pulses = 0;
            self.step_interval = 3_579_545 * STEP_MS[(cmd & 3) as usize] * 100;
            self.next_event = self.clock + self.step_interval;
            // Adopt the pinned openMSX scheduling convention: first eligible
            // pulse now, then one interval after every pulse, including the last.
            // Internal command startup latency is not modeled.
            self.movement_event();
        } else {
            match cmd & 0xe0 {
                0x80 | 0xa0 => {
                    self.write_sector = cmd & 0x20 != 0;
                    self.multi = cmd & 0x10 != 0;
                    self.compare_side = (cmd & 2 != 0).then_some((cmd >> 3) & 1);
                    if self.write_sector && self.disk.as_ref().is_some_and(|d| d.write_protected) {
                        self.finish(0x40);
                    } else if cmd & 4 != 0 {
                        self.transfer = Transfer::SectorSettle;
                        self.next_event = self.clock + SETTLE;
                    } else {
                        self.start_sector_search();
                    }
                }
                _ => self.finish(0x10), // Read address/track and format are outside raw-sector profile.
            }
        }
    }
    fn finish_movement(&mut self) {
        if self.verify {
            self.transfer = Transfer::Settle;
            self.next_event = self.clock + SETTLE;
        } else {
            self.finish(0);
        }
    }
    fn schedule_search(&mut self, inclusive: bool) {
        self.next_event = u64::MAX;
        // No rotating selected media means no index/ID input, not a synthetic
        // wall-clock timeout. Selection/motor writes can resume this search.
        if !self.ready() {
            return;
        }
        let base = self.clock / REVOLUTION * REVOLUTION;
        for phase in (0..9).map(id_phase) {
            let mut event = base + phase;
            if event < self.clock || (!inclusive && event == self.clock) {
                event += REVOLUTION;
            }
            self.next_event = self.next_event.min(event);
        }
    }
    fn verification_event(&mut self) {
        let phase = self.clock % REVOLUTION;
        // ID 1 and rising index coincide. Process the ID first so a matching
        // good-CRC cylinder wins over the fifth-index failure at this boundary.
        let has_id = self
            .disk
            .as_ref()
            .is_some_and(|disk| disk.offset(self.physical_track, self.side, 1).is_some());
        if has_id && self.track == self.physical_track {
            self.finish(0);
        } else if phase == 0 {
            self.search_indexes += 1;
            if self.search_indexes == 5 {
                self.finish(0x10);
            }
        }
        if self.transfer == Transfer::Verify {
            self.schedule_search(false);
        }
    }
    fn movement_event(&mut self) {
        let update = match self.movement {
            Movement::Complete => {
                self.finish_movement();
                return;
            }
            Movement::Restore => true,
            Movement::Seek { target } => {
                if self.track == target {
                    self.finish_movement();
                    return;
                }
                self.direction_in = target > self.track;
                true
            }
            Movement::Step { update } => {
                self.movement = Movement::Complete;
                update
            }
        };
        // The zero sensor suppresses an outward pulse and repairs Track, even
        // for non-updating STEP. In particular Track 0 must not underflow here.
        if !self.direction_in && self.track_zero() {
            self.track = 0;
            self.finish_movement();
            return;
        }
        if matches!(self.movement, Movement::Restore) && self.restore_pulses == 255 {
            self.finish(0x10);
            return;
        }
        if matches!(self.movement, Movement::Restore) {
            self.restore_pulses += 1;
        }
        if update {
            self.track = if self.direction_in {
                self.track.wrapping_add(1)
            } else {
                self.track.wrapping_sub(1)
            };
        }
        if !self.selected_a() {
            return; // Controller registers advance; absent drives have no mechanics.
        }
        if self.direction_in {
            // This raw-image drive has a hard upper stop at 79. The attempted
            // pulse updates Track but cannot move the head; report seek error
            // immediately and retain every earlier pulse. Invalid Track values
            // cannot alias valid sectors (ID search checks the physical cylinder).
            if self.physical_track == 79 {
                self.finish(0x10);
                return;
            }
            self.physical_track += 1;
        } else {
            self.physical_track -= 1;
        }
    }
    fn start_sector_search(&mut self) {
        self.transfer = Transfer::SectorSearch;
        self.search_indexes = 0;
        self.schedule_search(true);
    }
    fn sector_search_event(&mut self) {
        let phase = self.clock % REVOLUTION;
        // As for Type I, a matching ID precedes the coincident rising index.
        let sector = (0..9).find(|&id| id_phase(id) == phase).unwrap() as u8 + 1;
        let offset = self
            .disk
            .as_ref()
            .and_then(|disk| disk.offset(self.physical_track, self.side, sector));
        if let Some(offset) = offset.filter(|_| {
            self.track == self.physical_track
                && self.sector == sector
                && self.compare_side.is_none_or(|side| side == self.side)
        }) {
            self.offset = offset;
            self.buffer
                .copy_from_slice(&self.disk.as_ref().unwrap().bytes[offset..offset + SECTOR_SIZE]);
            self.index = 0;
            self.supplied = None;
            self.transfer = if self.write_sector {
                Transfer::WriteRequest
            } else {
                Transfer::Read
            };
            // ID phase is the start of FE. Fixed synthetic MFM header/gap
            // offsets; see the timing table in docs/disks.md.
            self.next_event = self.clock + BYTE_TIME * if self.write_sector { 9 } else { 46 };
            return;
        }
        if phase == 0 {
            self.search_indexes += 1;
            if self.search_indexes == 5 {
                self.finish(self.status & 4 | 0x10);
                return;
            }
        }
        self.schedule_search(false);
    }
    fn sector_end(&mut self) {
        if self.write_sector {
            let Some(d) = &mut self.disk else {
                self.finish(self.status & 4 | 0x80);
                return;
            };
            if d.write_protected {
                self.finish(self.status & 4 | 0x40);
                return;
            }
            let Some(dest) = d.bytes.get_mut(self.offset..self.offset + SECTOR_SIZE) else {
                self.finish(self.status & 4 | 0x10);
                return;
            };
            if dest != self.buffer {
                dest.copy_from_slice(&self.buffer);
                d.dirty = true;
            }
        }
        self.drq = false;
        if self.multi {
            self.sector = self.sector.wrapping_add(1);
            self.start_sector_search();
        } else {
            self.finish(self.status & 4);
        }
    }
    fn read_event(&mut self) {
        if self.drq {
            self.status |= 4;
        }
        self.data = self.buffer[self.index];
        self.drq = true;
        self.index += 1;
        self.next_event += BYTE_TIME;
        if self.index == SECTOR_SIZE {
            self.transfer = Transfer::ReadTail;
        }
    }
    fn write_event(&mut self) {
        // A missed position is gone forever. A late CPU write services only
        // the request currently visible, and cannot repair earlier zeroes.
        self.buffer[self.index] = self.supplied.take().unwrap_or_else(|| {
            self.status |= 4;
            0
        });
        self.index += 1;
        if self.index == SECTOR_SIZE {
            self.drq = false;
            self.transfer = Transfer::SectorEnd;
            // One byte still in the serializer, two CRC bytes, trailing FE.
            self.next_event += 4 * BYTE_TIME;
        } else {
            self.drq = true;
            self.next_event += BYTE_TIME;
        }
    }
    pub fn tick(&mut self, cycles: u32) {
        let end = self.clock + u64::from(cycles) * CLOCK_SCALE;
        if self.force_mask & 4 != 0 && end / REVOLUTION > self.clock / REVOLUTION && self.ready() {
            self.irq = true;
        }
        // Visit absolute fractional deadlines in chronological order, including
        // phase transitions inside one large CPU tick.
        while self.transfer != Transfer::Idle && self.next_event <= end {
            self.clock = self.next_event;
            match self.transfer {
                Transfer::Seek => {
                    self.next_event += self.step_interval;
                    self.movement_event();
                }
                Transfer::Settle => {
                    self.transfer = Transfer::Verify;
                    self.search_indexes = 0;
                    self.schedule_search(true);
                }
                Transfer::Verify => self.verification_event(),
                Transfer::SectorSettle => self.start_sector_search(),
                Transfer::SectorSearch => self.sector_search_event(),
                Transfer::Read => self.read_event(),
                Transfer::ReadTail => {
                    // The final byte has the same one-byte service budget.
                    // Keep it available through the CRC tail, even if late.
                    if self.drq {
                        self.status |= 4;
                    }
                    self.transfer = Transfer::SectorEnd;
                    self.next_event += BYTE_TIME;
                }
                Transfer::WriteRequest => {
                    self.drq = true;
                    self.transfer = Transfer::WriteCheck;
                    self.next_event += 8 * BYTE_TIME;
                }
                Transfer::WriteCheck => {
                    if let Some(first) = self.supplied.take() {
                        self.buffer[0] = first;
                        self.index = 1;
                        self.transfer = Transfer::WriteLeadIn;
                        self.next_event += 27 * BYTE_TIME;
                    } else {
                        self.finish(4);
                    }
                }
                Transfer::WriteLeadIn => {
                    self.drq = true;
                    self.transfer = Transfer::Write;
                    self.next_event += BYTE_TIME;
                }
                Transfer::Write => self.write_event(),
                Transfer::SectorEnd => self.sector_end(),
                _ => unreachable!(),
            }
        }
        self.clock = end;
        self.rotation = ((end % REVOLUTION) / CLOCK_SCALE) as u32;
    }
}
