//! One geometry adapter for the shared RAM profile publisher and editor.
//! Persisted geometry describes an offline library, never HID authorization.
use super::{APPLICATION_LAYOUT_UNSET_KEYCODE, SM62_APPLICATION_POSITIONS};
use crate::keyboard::KeyboardLayout;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ApplicationLayoutGeometry {
    version: u8,
    rows: u8,
    cols: u8,
    encoders: u8,
    controls: u8,
    // Physical descriptor occupancy; matrix holes are retained but never rendered.
    occupied: [u8; 32],
}

impl ApplicationLayoutGeometry {
    pub(crate) fn legacy(version: u8) -> Option<Self> {
        match version {
            7 => Some(Self {
                version,
                rows: 5,
                cols: 3,
                encoders: 1,
                controls: 15,
                occupied: [0; 32],
            }),
            8 => Some(Self {
                version,
                rows: 10,
                cols: 7,
                encoders: 0,
                controls: 62,
                occupied: [0; 32],
            }),
            _ => None,
        }
    }
    pub(crate) fn version(self) -> u8 {
        self.version
    }
    pub(crate) fn controls(self) -> usize {
        usize::from(self.controls)
    }
    pub(crate) fn presets(self) -> bool {
        self.version == 7
    }
    pub(crate) fn matrix(self) -> bool {
        self.version == 9
    }
    pub(crate) fn valid(self) -> bool {
        if self.version != 9 {
            return Self::legacy(self.version) == Some(self);
        }
        let keys = usize::from(self.rows) * usize::from(self.cols);
        self.rows > 0
            && self.cols > 0
            && keys <= 251
            && self.encoders <= 2
            && keys + 2 * usize::from(self.encoders) == self.controls()
            && (self.controls()..256).all(|i| !self.occupied(i))
            && (keys..self.controls()).all(|i| self.occupied(i))
            && (0..keys).any(|i| self.occupied(i))
    }
    fn occupied(self, control: usize) -> bool {
        control < 256 && self.occupied[control / 8] & (1 << (control % 8)) != 0
    }
    fn mark(&mut self, control: usize) {
        self.occupied[control / 8] |= 1 << (control % 8);
    }
    pub(crate) fn key(self, row: u8, col: u8) -> Option<usize> {
        match self.version {
            9 if row < self.rows && col < self.cols => {
                Some(usize::from(row) * usize::from(self.cols) + usize::from(col))
            }
            8 => SM62_APPLICATION_POSITIONS
                .iter()
                .position(|&p| p == (row, col)),
            7 => match (row, col) {
                (0, 2) => Some(12),
                (1..=4, 0..=2) => Some((usize::from(row) - 1) * 3 + usize::from(col)),
                _ => None,
            },
            _ => None,
        }
    }
    pub(crate) fn encoder(self, index: u8, direction: u8) -> Option<usize> {
        if index >= self.encoders || direction > 1 {
            return None;
        }
        Some(
            if self.matrix() {
                usize::from(self.rows) * usize::from(self.cols)
            } else {
                13
            } + 2 * usize::from(index)
                + usize::from(direction),
        )
    }
    pub(crate) fn complete(self, layers: &[Vec<u16>]) -> bool {
        layers.len() == 16
            && layers.iter().all(|l| {
                l.len() == self.controls()
                    && (!self.matrix()
                        || l.iter().enumerate().all(|(i, &v)| {
                            if self.occupied(i) {
                                v != APPLICATION_LAYOUT_UNSET_KEYCODE
                            } else {
                                v == APPLICATION_LAYOUT_UNSET_KEYCODE
                            }
                        }))
            })
    }
    pub(crate) fn descriptor(mut self, layout: &KeyboardLayout) -> Option<Self> {
        if layout.rows != usize::from(self.rows)
            || layout.cols != usize::from(self.cols)
            || layout.encoder_count() != usize::from(self.encoders)
            || layout
                .keys
                .iter()
                .any(|k| k.row >= self.rows || k.col >= self.cols)
            || layout
                .encoders
                .iter()
                .any(|e| self.encoder(e.encoder_idx, e.direction).is_none())
        {
            return None;
        }
        if self.matrix() {
            self.occupied = [0; 32];
            for k in &layout.keys {
                self.mark(self.key(k.row, k.col)?);
            }
            for i in usize::from(self.rows) * usize::from(self.cols)..self.controls() {
                self.mark(i);
            }
            // Both turns of every encoder must be represented by the real descriptor.
            for i in 0..self.encoders {
                for d in 0..2 {
                    if !layout
                        .encoders
                        .iter()
                        .any(|e| e.encoder_idx == i && e.direction == d)
                    {
                        return None;
                    }
                }
            }
        }
        self.valid().then_some(self)
    }
    /// Descriptor mapping only. This never grants protocol/write capability.
    pub(crate) fn for_layout(layout: &KeyboardLayout) -> Option<Self> {
        if layout.rows == 10 && layout.cols == 8 {
            return Self {
                version: 9,
                rows: 10,
                cols: 8,
                encoders: 2,
                controls: 84,
                occupied: [0; 32],
            }
            .descriptor(layout);
        }
        if layout.rows == 10
            && layout.cols == 7
            && layout.keys.len() == 62
            && layout.encoders.is_empty()
        {
            let g = Self::legacy(8)?;
            if layout.keys.iter().all(|k| g.key(k.row, k.col).is_some()) {
                return Some(g);
            }
        }
        if layout.rows == 5 && layout.cols == 3 {
            return Self::legacy(7)?.descriptor(layout);
        }
        None
    }
    pub(crate) fn request(nonce: u32) -> [u8; 32] {
        let mut p = [0; 32];
        p[..4].copy_from_slice(&[0xeb, 9, 0xa5, 1]);
        p[4..8].copy_from_slice(&nonce.to_le_bytes());
        p
    }
    pub(crate) fn capability(p: &[u8; 32], nonce: u32) -> Option<Self> {
        if p[..4] != [0xeb, 9, 0x5a, 1]
            || p[4..8] != nonce.to_le_bytes()
            || p[8] != 16
            || p[14..20] != [22, 1, 13, 27, 4, 12]
            || p[20..22] != 2500u16.to_le_bytes()
            || p[22..].iter().any(|&b| b != 0)
        {
            return None;
        }
        let keys = usize::from(p[9]) * usize::from(p[10]);
        let total = keys + 2 * usize::from(p[11]);
        if p[9] == 0
            || p[10] == 0
            || keys > 251
            || p[11] > 2
            || total > 255
            || usize::from(p[12]) != keys
            || usize::from(p[13]) != total
        {
            return None;
        }
        Some(Self {
            version: 9,
            rows: p[9],
            cols: p[10],
            encoders: p[11],
            controls: p[13],
            occupied: [0; 32],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matrix_v9_capability_rejects_nonce_reserved_and_derived_counts() {
        let nonce: u32 = 0x10203040;
        let mut p = [0; 32];
        p[..4].copy_from_slice(&[0xeb, 9, 0x5a, 1]);
        p[4..8].copy_from_slice(&nonce.to_le_bytes());
        p[8..20].copy_from_slice(&[16, 10, 8, 2, 80, 84, 22, 1, 13, 27, 4, 12]);
        p[20..22].copy_from_slice(&2500u16.to_le_bytes());
        assert_eq!(
            ApplicationLayoutGeometry::capability(&p, nonce)
                .unwrap()
                .controls(),
            84
        );
        for i in [
            0, 1, 2, 3, 4, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 31,
        ] {
            let mut bad = p;
            bad[i] ^= 1;
            assert!(
                ApplicationLayoutGeometry::capability(&bad, nonce).is_none(),
                "byte {i}"
            );
        }
    }
}
