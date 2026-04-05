//! Wire format for frogma presence snapshots.
//!
//! Per ADR-0002, one snapshot on the wire is 42 bytes, little-endian.
//! No framing; UDP gives us datagram boundaries for free. If we ever need
//! more than one snapshot per packet, that's a later version.

pub const SNAPSHOT_LEN: usize = 42;
pub const WIRE_VERSION: u8 = 0;

/// A single peer's presence snapshot. Cheap to copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapshot {
    pub peer_id: u64,
    pub seq: u32,
    pub t_send_ms: u64,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub hp: u16,
    pub hp_max: u16,
    pub vocation: u8,
    pub pose: u8,
}

#[derive(Debug, PartialEq)]
pub enum WireError {
    Short(usize),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Short(n) => write!(f, "short packet: expected {SNAPSHOT_LEN} bytes, got {n}"),
        }
    }
}

impl std::error::Error for WireError {}

impl Snapshot {
    pub fn encode(&self) -> [u8; SNAPSHOT_LEN] {
        let mut b = [0u8; SNAPSHOT_LEN];
        b[0..8].copy_from_slice(&self.peer_id.to_le_bytes());
        b[8..12].copy_from_slice(&self.seq.to_le_bytes());
        b[12..20].copy_from_slice(&self.t_send_ms.to_le_bytes());
        b[20..24].copy_from_slice(&self.pos[0].to_le_bytes());
        b[24..28].copy_from_slice(&self.pos[1].to_le_bytes());
        b[28..32].copy_from_slice(&self.pos[2].to_le_bytes());
        b[32..36].copy_from_slice(&self.yaw.to_le_bytes());
        b[36..38].copy_from_slice(&self.hp.to_le_bytes());
        b[38..40].copy_from_slice(&self.hp_max.to_le_bytes());
        b[40] = self.vocation;
        b[41] = self.pose;
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        if b.len() < SNAPSHOT_LEN {
            return Err(WireError::Short(b.len()));
        }
        Ok(Self {
            peer_id: u64::from_le_bytes(b[0..8].try_into().unwrap()),
            seq: u32::from_le_bytes(b[8..12].try_into().unwrap()),
            t_send_ms: u64::from_le_bytes(b[12..20].try_into().unwrap()),
            pos: [
                f32::from_le_bytes(b[20..24].try_into().unwrap()),
                f32::from_le_bytes(b[24..28].try_into().unwrap()),
                f32::from_le_bytes(b[28..32].try_into().unwrap()),
            ],
            yaw: f32::from_le_bytes(b[32..36].try_into().unwrap()),
            hp: u16::from_le_bytes(b[36..38].try_into().unwrap()),
            hp_max: u16::from_le_bytes(b[38..40].try_into().unwrap()),
            vocation: b[40],
            pose: b[41],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Snapshot {
        Snapshot {
            peer_id: 0xdead_beef_cafe_babe,
            seq: 0x1234_5678,
            t_send_ms: 1_712_345_678_901,
            pos: [12.5, -3.75, 100.0],
            yaw: 1.5707,
            hp: 847,
            hp_max: 1200,
            vocation: 3,
            pose: 1,
        }
    }

    #[test]
    fn size_is_42() {
        assert_eq!(SNAPSHOT_LEN, 42);
        assert_eq!(sample().encode().len(), 42);
    }

    #[test]
    fn roundtrip() {
        let s = sample();
        let bytes = s.encode();
        let back = Snapshot::decode(&bytes).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn short_packet_rejects() {
        let r = Snapshot::decode(&[0u8; 10]);
        assert!(matches!(r, Err(WireError::Short(10))));
    }

    #[test]
    fn field_layout_is_little_endian() {
        let s = Snapshot {
            peer_id: 0x0102_0304_0506_0708,
            ..sample()
        };
        let b = s.encode();
        assert_eq!(&b[0..8], &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }
}
