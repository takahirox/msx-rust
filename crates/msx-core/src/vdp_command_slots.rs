//! Independently encoded bitmap service-grid facts for Issue #77.
//! Profile and measurement provenance: docs/msx2.md, autonomous command timing.
//! Coordinates are upstream /RAS master clocks, before the local +202 mapping.
#[derive(Clone, Copy)]
pub(super) enum Grid {
    Blank,
    NoSprites,
    Sprites,
}
impl Grid {
    pub(super) fn slot(self, t: u32) -> bool {
        match self {
            Self::Blank => {
                (t <= 120 && t.is_multiple_of(8))
                    || ((164..=1228).contains(&t)
                        && (t - 164).is_multiple_of(8)
                        && (t - 164) % 128 <= 112)
                    || ((1268..=1324).contains(&t) && (t - 1268).is_multiple_of(8))
                    || matches!(t, 1334 | 1344 | 1352 | 1360)
            }
            Self::NoSprites => {
                ((6..=118).contains(&t) && (t - 6).is_multiple_of(8))
                    || matches!(t, 162 | 170 | 182 | 188)
                    || ((214..=1212).contains(&t)
                        && matches!((t - 214) % 128, 0 | 6 | 32 | 38 | 64 | 96 | 102))
                    || ((1266..=1322).contains(&t) && (t - 1266).is_multiple_of(8))
                    || matches!(t, 1332 | 1342 | 1350 | 1358 | 1366)
            }
            Self::Sprites => {
                matches!(t, 28 | 92 | 162 | 170 | 188 | 1264 | 1330)
                    || ((220..=1212).contains(&t) && matches!((t - 220) % 128, 0 | 32 | 96))
            }
        }
    }
    pub(super) fn extra(self) -> i32 {
        i32::from(matches!(self, Self::Sprites))
    }
    pub(super) fn tight(self, t: u32) -> i32 {
        i32::from(matches!(self, Self::NoSprites) && t >= 6 && self.slot(t) && self.slot(t - 6))
    }
    /// Padding completed at this clock is invisible to the delay counter.
    pub(super) fn padding(self, t: u32) -> i32 {
        match (self, t) {
            (Self::Blank, 1334 | 1344) | (Self::NoSprites, 1332 | 1342) | (Self::Sprites, 1330) => {
                2
            }
            (Self::Sprites, 1337 | 1348) => 1,
            _ => 0,
        }
    }
}
