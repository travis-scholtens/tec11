//! Self-contained, versioned binary sparse `DeltaOverlay` (`T11D`) codec for downstream
//! games (`FR-007`, `US4`). Untouched chunks consume zero persistent storage.

use std::collections::BTreeMap;

use crate::chunk::ChunkCoord;
use crate::materials::{HexPrismCell, MaterialId};
use crate::LocalTerrainError;

const MAGIC: &[u8; 4] = b"T11D";
pub const DELTA_FORMAT_VERSION: u16 = 1;

/// Sparse per-chunk cell edit overlay (`data-model.md`: `DeltaOverlay`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeltaOverlay {
    pub version: u16,
    pub anchor_hash: u64,
    pub chunks: BTreeMap<ChunkCoord, BTreeMap<u16, HexPrismCell>>,
}

impl DeltaOverlay {
    pub fn new(anchor_hash: u64) -> Self {
        Self {
            version: DELTA_FORMAT_VERSION,
            anchor_hash,
            chunks: BTreeMap::new(),
        }
    }

    pub fn set_cell(&mut self, coord: ChunkCoord, local_idx: u16, cell: HexPrismCell) {
        self.chunks
            .entry(coord)
            .or_default()
            .insert(local_idx, cell);
    }

    pub fn get_chunk_edits(&self, coord: ChunkCoord) -> Option<&BTreeMap<u16, HexPrismCell>> {
        self.chunks.get(&coord)
    }

    pub fn modified_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn modified_cell_count(&self) -> usize {
        self.chunks.values().map(|m| m.len()).sum()
    }

    /// Encode only modified chunks & cells into compact little-endian `T11D` bytes.
    pub fn encode_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(18 + self.modified_chunk_count() * 14 + self.modified_cell_count() * 6);
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.anchor_hash.to_le_bytes());
        buf.extend_from_slice(&(self.chunks.len() as u32).to_le_bytes());
        for (coord, cells) in &self.chunks {
            buf.extend_from_slice(&coord.cq.to_le_bytes());
            buf.extend_from_slice(&coord.cr.to_le_bytes());
            buf.extend_from_slice(&coord.cz.to_le_bytes());
            buf.extend_from_slice(&(cells.len() as u16).to_le_bytes());
            for (&idx, &cell) in cells {
                buf.extend_from_slice(&idx.to_le_bytes());
                buf.extend_from_slice(&cell.material.0.to_le_bytes());
                buf.push(cell.fluid);
                buf.push(cell.flags);
            }
        }
        buf
    }

    /// Decode `T11D` bytes and validate version and `expected_anchor_hash`.
    pub fn decode_bytes(bytes: &[u8], expected_anchor_hash: u64) -> Result<Self, LocalTerrainError> {
        if bytes.len() < 18 || &bytes[0..4] != MAGIC {
            return Err(LocalTerrainError::InvalidDelta("Invalid T11D magic header"));
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != DELTA_FORMAT_VERSION {
            return Err(LocalTerrainError::InvalidDelta("Unsupported T11D delta version"));
        }
        let anchor_hash = u64::from_le_bytes(bytes[6..14].try_into().unwrap());
        if anchor_hash != expected_anchor_hash {
            return Err(LocalTerrainError::AnchorHashMismatch {
                expected: expected_anchor_hash,
                actual: anchor_hash,
            });
        }
        let chunk_count = u32::from_le_bytes(bytes[14..18].try_into().unwrap()) as usize;
        let mut cursor = 18usize;
        let mut chunks = BTreeMap::new();

        for _ in 0..chunk_count {
            if cursor + 14 > bytes.len() {
                return Err(LocalTerrainError::InvalidDelta("Truncated chunk header"));
            }
            let cq = i32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            let cr = i32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap());
            let cz = i32::from_le_bytes(bytes[cursor + 8..cursor + 12].try_into().unwrap());
            let cell_count = u16::from_le_bytes(bytes[cursor + 12..cursor + 14].try_into().unwrap()) as usize;
            cursor += 14;

            if cursor + cell_count * 6 > bytes.len() {
                return Err(LocalTerrainError::InvalidDelta("Truncated chunk cell list"));
            }
            let mut cells = BTreeMap::new();
            for _ in 0..cell_count {
                let idx = u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
                let mat = u16::from_le_bytes(bytes[cursor + 2..cursor + 4].try_into().unwrap());
                let fluid = bytes[cursor + 4];
                let flags = bytes[cursor + 5];
                cursor += 6;
                cells.insert(
                    idx,
                    HexPrismCell {
                        material: MaterialId(mat),
                        fluid,
                        flags,
                    },
                );
            }
            chunks.insert(ChunkCoord { cq, cr, cz }, cells);
        }

        Ok(Self {
            version,
            anchor_hash,
            chunks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::cell_flags;

    #[test]
    fn test_delta_overlay_binary_roundtrip_and_anchor_validation() {
        let mut delta = DeltaOverlay::new(0x1234_5678_9abc_def0);
        let c1 = ChunkCoord { cq: 0, cr: 0, cz: 10 };
        let c2 = ChunkCoord { cq: 1, cr: 0, cz: 10 };
        delta.set_cell(
            c1,
            42,
            HexPrismCell {
                material: MaterialId::AIR,
                fluid: 0,
                flags: cell_flags::EXCAVATED_BY_DELTA,
            },
        );
        delta.set_cell(c2, 100, HexPrismCell::solid(MaterialId::GRANITE));

        let bytes = delta.encode_bytes();
        assert!(bytes.len() < 128);

        let decoded = DeltaOverlay::decode_bytes(&bytes, 0x1234_5678_9abc_def0)
            .expect("Valid delta should decode");
        assert_eq!(decoded, delta);

        let bad = DeltaOverlay::decode_bytes(&bytes, 0xdead_beef);
        assert!(matches!(bad, Err(LocalTerrainError::AnchorHashMismatch { .. })));
    }
}
