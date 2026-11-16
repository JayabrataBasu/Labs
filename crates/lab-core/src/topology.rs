//! Spatial structure as a compressed adjacency list, so rules only ever ask for neighbours.
use crate::LabError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topology {
    pub width: u32,
    pub height: u32,
    pub wrap: bool,
    offsets: Vec<u32>,
    adjacency: Vec<u32>,
}

impl Topology {
    /// A `width` x `height` lattice with von Neumann neighbourhoods; site index is `y * width + x`.
    ///
    /// Wrapped lattices need both dimensions >= 3, otherwise a site would list a neighbour twice.
    pub fn lattice(width: u32, height: u32, wrap: bool) -> Result<Self, LabError> {
        if width == 0 || height == 0 {
            return Err(LabError::Config("lattice width and height must be at least 1".into()));
        }
        if wrap && (width < 3 || height < 3) {
            return Err(LabError::Config("a wrapped lattice needs width and height of at least 3".into()));
        }
        let n = width.checked_mul(height).ok_or(LabError::Overflow("lattice size"))?;
        n.checked_mul(4).ok_or(LabError::Overflow("lattice size"))?;
        let mut offsets = Vec::with_capacity(n as usize + 1);
        let mut adjacency = Vec::with_capacity(n as usize * 4);
        offsets.push(0);
        for y in 0..height {
            for x in 0..width {
                // Order: north, east, south, west.
                if y > 0 {
                    adjacency.push((y - 1) * width + x);
                } else if wrap {
                    adjacency.push((height - 1) * width + x);
                }
                if x + 1 < width {
                    adjacency.push(y * width + x + 1);
                } else if wrap {
                    adjacency.push(y * width);
                }
                if y + 1 < height {
                    adjacency.push((y + 1) * width + x);
                } else if wrap {
                    adjacency.push(x);
                }
                if x > 0 {
                    adjacency.push(y * width + x - 1);
                } else if wrap {
                    adjacency.push(y * width + width - 1);
                }
                offsets.push(adjacency.len() as u32);
            }
        }
        Ok(Topology { width, height, wrap, offsets, adjacency })
    }

    pub fn n_sites(&self) -> u32 {
        self.offsets.len() as u32 - 1
    }

    /// Neighbours of `site` in canonical order (north, east, south, west; missing ones omitted).
    pub fn neighbours(&self, site: u32) -> &[u32] {
        let s = site as usize;
        &self.adjacency[self.offsets[s] as usize..self.offsets[s + 1] as usize]
    }
}
