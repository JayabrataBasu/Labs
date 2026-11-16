//! One canonical byte form of the state: it is both the checkpoint format and the hash input.
//! All integers are little-endian, so the bytes are identical on every platform.
use crate::{EntityMeta, Gene, GeneKind, LabError, Ledger, State, Topology};
use sha2::{Digest, Sha256};

/// Version 2 added genomes, lineage fields, conversion ledgers and birth/death counters.
pub const FORMAT_VERSION: u32 = 2;
const MAGIC: &[u8; 4] = b"LABS";

pub fn encode(state: &State, config_hash: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + 8 * (state.site_stock.len() + state.ent_stock.len()) + 24 * state.entities.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(config_hash);
    put(&mut out, state.tick);
    put(&mut out, state.k as u64);
    out.extend_from_slice(&state.topo.width.to_le_bytes());
    out.extend_from_slice(&state.topo.height.to_le_bytes());
    out.push(u8::from(state.topo.wrap));
    put(&mut out, state.entities.len() as u64);
    put(&mut out, state.next_entity_id);
    put(&mut out, state.births);
    put(&mut out, state.deaths);
    put_all(&mut out, &state.site_stock);
    for e in &state.entities {
        put(&mut out, e.id);
        out.extend_from_slice(&e.site.to_le_bytes());
        put_opt(&mut out, e.parent);
        put_opt(&mut out, e.mother);
        put(&mut out, e.born);
    }
    put_all(&mut out, &state.ent_stock);
    for genome in &state.genomes {
        out.extend_from_slice(&(genome.len() as u32).to_le_bytes());
        for g in genome {
            out.extend_from_slice(&[g.kind as u8, g.a, g.b, g.sense]);
            out.extend_from_slice(&g.level.to_le_bytes());
            out.extend_from_slice(&g.threshold.to_le_bytes());
        }
    }
    put_all(&mut out, &state.ledger.influx);
    put_all(&mut out, &state.ledger.dissipated);
    put_all(&mut out, &state.ledger.converted_in);
    put_all(&mut out, &state.ledger.converted_out);
    put_all(&mut out, &state.initial_total);
    out
}

pub fn decode(bytes: &[u8]) -> Result<(State, [u8; 32]), LabError> {
    let mut r = Reader { bytes, pos: 0 };
    if r.take(4)? != MAGIC {
        return Err(LabError::Codec("not a lab checkpoint (bad magic)".into()));
    }
    let version = r.u32()?;
    if version != FORMAT_VERSION {
        return Err(LabError::Codec(format!("checkpoint format version {version}, this build reads version {FORMAT_VERSION}")));
    }
    let mut config_hash = [0u8; 32];
    config_hash.copy_from_slice(r.take(32)?);
    let tick = r.u64()?;
    let k = r.len()?;
    let (width, height) = (r.u32()?, r.u32()?);
    let wrap = match r.take(1)?[0] {
        0 => false,
        1 => true,
        other => return Err(LabError::Codec(format!("invalid wrap flag {other}"))),
    };
    let n_entities = r.len()?;
    let next_entity_id = r.u64()?;
    let births = r.u64()?;
    let deaths = r.u64()?;
    let topo = Topology::lattice(width, height, wrap).map_err(|e| LabError::Codec(e.to_string()))?;
    let too_big = || LabError::Codec("declared sizes overflow".into());
    let n_site_values = (topo.n_sites() as usize).checked_mul(k).ok_or_else(too_big)?;
    let n_ent_values = n_entities.checked_mul(k).ok_or_else(too_big)?;
    let site_stock = r.u64s(n_site_values)?;
    let mut entities = Vec::with_capacity(n_entities.min(r.remaining() / 22));
    for _ in 0..n_entities {
        let id = r.u64()?;
        let site = r.u32()?;
        let parent = r.opt()?;
        let mother = r.opt()?;
        let born = r.u64()?;
        entities.push(EntityMeta { id, site, parent, mother, born });
    }
    let ent_stock = r.u64s(n_ent_values)?;
    let mut genomes = Vec::with_capacity(entities.len());
    for _ in 0..n_entities {
        let n_genes = r.u32()? as usize;
        let mut genome = Vec::with_capacity(n_genes.min(r.remaining() / 8));
        for _ in 0..n_genes {
            let raw = r.take(8)?;
            let kind = GeneKind::from_u8(raw[0]).ok_or_else(|| LabError::Codec(format!("invalid gene kind {}", raw[0])))?;
            genome.push(Gene {
                kind,
                a: raw[1],
                b: raw[2],
                sense: raw[3],
                level: u16::from_le_bytes([raw[4], raw[5]]),
                threshold: u16::from_le_bytes([raw[6], raw[7]]),
            });
        }
        genomes.push(genome);
    }
    let ledger = Ledger { influx: r.u64s(k)?, dissipated: r.u64s(k)?, converted_in: r.u64s(k)?, converted_out: r.u64s(k)? };
    let initial_total = r.u64s(k)?;
    if r.remaining() != 0 {
        return Err(LabError::Codec(format!("{} unexpected trailing bytes", r.remaining())));
    }
    let state = State { tick, k, topo, site_stock, entities, ent_stock, genomes, ledger, next_entity_id, births, deaths, initial_total };
    state.validate().map_err(LabError::Codec)?;
    Ok((state, config_hash))
}

/// SHA-256 of the canonical bytes.
pub fn state_hash(state: &State, config_hash: &[u8; 32]) -> [u8; 32] {
    Sha256::digest(encode(state, config_hash)).into()
}

fn put(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_opt(out: &mut Vec<u8>, v: Option<u64>) {
    match v {
        None => out.push(0),
        Some(x) => {
            out.push(1);
            put(out, x);
        }
    }
}

fn put_all(out: &mut Vec<u8>, vs: &[u64]) {
    for &v in vs {
        put(out, v);
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], LabError> {
        if self.remaining() < n {
            return Err(LabError::Codec("checkpoint is truncated".into()));
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
    fn u32(&mut self) -> Result<u32, LabError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4 bytes")))
    }
    fn u64(&mut self) -> Result<u64, LabError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8 bytes")))
    }
    fn opt(&mut self) -> Result<Option<u64>, LabError> {
        match self.take(1)?[0] {
            0 => Ok(None),
            1 => Ok(Some(self.u64()?)),
            other => Err(LabError::Codec(format!("invalid option tag {other}"))),
        }
    }
    fn len(&mut self) -> Result<usize, LabError> {
        usize::try_from(self.u64()?).map_err(|_| LabError::Codec("length does not fit this platform".into()))
    }
    fn u64s(&mut self, n: usize) -> Result<Vec<u64>, LabError> {
        let raw = self.take(n.checked_mul(8).ok_or_else(|| LabError::Codec("declared sizes overflow".into()))?)?;
        Ok(raw.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c)).collect())
    }
}
