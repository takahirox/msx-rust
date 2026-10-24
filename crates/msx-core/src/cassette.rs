//! Read-only MSX CAS reconstruction. Timing is in rational quarter-bit units;
//! no pulse/waveform buffer or per-edge allocation is needed.
pub const MARKER: [u8; 8] = [0x1f, 0xa6, 0xde, 0xba, 0xcc, 0x13, 0x7d, 0x74];
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_BLOCKS: usize = 65536;
const CLOCK: u64 = 3_579_545;

struct Block {
    bytes: std::ops::Range<usize>,
    leader: u64,
    data: u64,
    end: u64,
}
struct Image {
    bytes: Vec<u8>,
    blocks: Vec<Block>,
    end: u64,
}

/// Tape position is retained across motor stops, reset and firmware changes.
/// The PPI latch is the sole motor authority; callers supply its active-low C4.
pub struct Cassette {
    image: Option<Image>,
    rate: u32,
    // Quarter-bit units multiplied by CLOCK: retains sub-edge fractional phase.
    position: u64,
}
impl Default for Cassette {
    fn default() -> Self {
        Self {
            image: None,
            rate: 1200,
            position: 0,
        }
    }
}
impl Cassette {
    pub fn mount(&mut self, bytes: Vec<u8>, rate: u32) -> Result<(), String> {
        if !matches!(rate, 1200 | 2400) {
            return Err("CAS rate must be 1200 or 2400 baud.".into());
        }
        if bytes.len() > MAX_BYTES || !bytes.starts_with(&MARKER) {
            return Err(
                "Select an MSX CAS image beginning with its block marker (maximum 16 MiB).".into(),
            );
        }
        let mut blocks = Vec::new();
        let mut start = MARKER.len();
        let mut end = 0;
        // 0: header/unknown block; 1: binary/BASIC body; 2: ASCII body/continuation.
        let mut expected = 0;
        loop {
            let next = bytes[start..]
                .windows(MARKER.len())
                .position(|w| w == MARKER)
                .map_or(bytes.len(), |n| start + n);
            let payload = &bytes[start..next];
            if payload.is_empty() {
                return Err("CAS contains an empty or truncated block.".into());
            }
            let long = expected == 0;
            if long
                && payload.len() >= 10
                && [0xd0, 0xd3, 0xea].contains(&payload[0])
                && payload[..10].iter().all(|&b| b == payload[0])
            {
                if payload.len() < 16 {
                    return Err(
                        "CAS file header needs ten type bytes and six filename bytes.".into(),
                    );
                }
                expected = if payload[0] == 0xea { 2 } else { 1 };
            } else if expected == 1 || (expected == 2 && payload.contains(&0x1a)) {
                expected = 0;
            }
            if blocks.len() == MAX_BLOCKS {
                return Err("CAS has too many blocks (maximum 65536).".into());
            }
            blocks
                .try_reserve(1)
                .map_err(|_| "Cannot reserve CAS block metadata.")?;
            let leader = end + u64::from(rate) * 4 * if long { 2 } else { 1 };
            // 16000/32000 or 4000/8000 high-frequency cycles; each is two quarters.
            let data = leader + (if long { 32000 } else { 8000 }) * u64::from(rate / 1200);
            end = data + payload.len() as u64 * 44;
            blocks.push(Block {
                bytes: start..next,
                leader,
                data,
                end,
            });
            if next == bytes.len() {
                break;
            }
            start = next + MARKER.len();
        }
        if expected != 0 {
            return Err("CAS standard file is missing a body or ASCII end-of-file byte.".into());
        }
        self.image = Some(Image { bytes, blocks, end });
        self.rate = rate;
        self.position = 0;
        Ok(())
    }
    pub fn rewind(&mut self) {
        self.position = 0;
    }
    pub fn eject(&mut self) {
        self.image = None;
        self.position = 0;
    }
    pub fn mounted(&self) -> bool {
        self.image.is_some()
    }
    pub fn rate(&self) -> u32 {
        self.rate
    }
    pub fn ended(&self) -> bool {
        self.image
            .as_ref()
            .is_some_and(|i| self.position == i.end * CLOCK)
    }
    pub fn position_cycles(&self) -> u64 {
        self.position / (u64::from(self.rate) * 4)
    }
    pub fn tick(&mut self, cycles: u32, motor: bool) {
        if motor {
            if let Some(image) = &self.image {
                self.position = (self.position + u64::from(cycles) * u64::from(self.rate) * 4)
                    .min(image.end * CLOCK);
            }
        }
    }
    pub fn input(&self, motor: bool) -> bool {
        if !motor {
            return false;
        }
        let Some(image) = &self.image else {
            return false;
        };
        let quarter = self.position / CLOCK;
        let index = image.blocks.partition_point(|b| b.end <= quarter);
        let Some(block) = image.blocks.get(index) else {
            return false;
        };
        if quarter < block.leader {
            return false;
        }
        if quarter < block.data {
            return (quarter - block.leader).is_multiple_of(2);
        }
        let offset = quarter - block.data;
        let byte = image.bytes[block.bytes.start + (offset / 44) as usize];
        let bit = offset % 44 / 4;
        let one = bit >= 9 || (bit != 0 && byte & (1 << (bit - 1)) != 0);
        if one {
            offset.is_multiple_of(2)
        } else {
            offset % 4 < 2
        }
    }
}
