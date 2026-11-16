//! The tick loop. Proposals are computed in parallel; application is sequential and ordered,
//! so the result is independent of the number of threads.
use crate::{apply, check_conservation, Delta, LabError, Rule, State};
use lab_rng::{RuleRng, STREAM_INIT};
use rayon::prelude::*;
use std::ops::Range;

/// Sites per work unit. A constant: chunk boundaries must not depend on the thread count.
pub const CHUNK_SITES: u32 = 1024;

pub struct Engine {
    rules: Vec<Box<dyn Rule>>,
    seed: u64,
    pool: rayon::ThreadPool,
}

impl Engine {
    pub fn new(rules: Vec<Box<dyn Rule>>, seed: u64, threads: usize) -> Result<Self, LabError> {
        if threads == 0 {
            return Err(LabError::Config("threads must be at least 1".into()));
        }
        let mut streams: Vec<u16> = rules.iter().map(|r| r.stream()).collect();
        streams.sort_unstable();
        if streams.contains(&STREAM_INIT) {
            return Err(LabError::Config(format!("stream {STREAM_INIT} is reserved for initial placement")));
        }
        if let Some(w) = streams.windows(2).find(|w| w[0] == w[1]) {
            return Err(LabError::Config(format!("stream {} is used by more than one rule", w[0])));
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| LabError::Io(format!("cannot start worker threads: {e}")))?;
        Ok(Engine { rules, seed, pool })
    }

    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    /// Advances one tick: each rule in order proposes against the state left by the previous
    /// rule, then its deltas are applied. Conservation is verified before the tick counter moves.
    ///
    /// If this returns an error the state may be partly updated and must be discarded.
    pub fn step(&self, state: &mut State) -> Result<(), LabError> {
        if state.tick >= u64::from(u32::MAX) {
            return Err(LabError::Config("tick limit of 2^32 - 1 reached".into()));
        }
        let n_sites = state.topo.n_sites();
        let chunks: Vec<Range<u32>> = (0..n_sites)
            .step_by(CHUNK_SITES as usize)
            .map(|start| start..start.saturating_add(CHUNK_SITES).min(n_sites))
            .collect();
        for rule in &self.rules {
            let rng = RuleRng { seed: self.seed, stream: rule.stream() };
            let index = state.site_index();
            let snapshot: &State = state;
            let proposals: Vec<Vec<Delta>> = self.pool.install(|| {
                chunks
                    .par_iter()
                    .map(|sites| {
                        let mut out = Vec::new();
                        rule.propose(snapshot, &index, rng, sites.clone(), &mut out);
                        out
                    })
                    .collect()
            });
            for deltas in &proposals {
                apply(state, deltas)?;
            }
        }
        check_conservation(state)?;
        state.tick += 1;
        Ok(())
    }
}
