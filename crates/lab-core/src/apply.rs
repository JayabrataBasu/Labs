//! How one rule's deltas change the state (DR-014).
//!
//! Semantics, defined per site so that they cannot depend on chunking or thread count:
//!
//! 1. Every delta has a *home site*: the site of its `from` holder (for `Source`, of its `to`
//!    holder). A site's deltas are processed in the order the rule proposed them.
//! 2. A withdrawal is clipped to what the holder has at that moment.
//! 3. If giver and receiver are at the same site (or the giver is `Source`, or the receiver
//!    is `Sink`), the receiver is credited immediately.
//! 4. Otherwise the amount is withdrawn immediately but credited only after every site has
//!    been processed. Nothing can be passed on in the same rule step in which it arrived
//!    from another site.
//! 5. Moves take effect last, in proposal order.
//!
//! Sites therefore never read each other's stocks while a rule is applied, which is what
//! lets blocks of sites be processed on different threads with identical results.
use crate::{Delta, EntityMeta, Holder, LabError, SiteIndex, State};
use rayon::prelude::*;
use std::ops::Range;

/// What a block of sites hands back for the ordered finishing pass.
struct BlockOut {
    deposits: Vec<(Holder, u16, u64)>,
    moves: Vec<(u32, u32)>,
    influx: Vec<u64>,
    dissipated: Vec<u64>,
}

/// Read-only facts every block needs.
struct Context<'a> {
    k: usize,
    n_sites: u32,
    entities: &'a [EntityMeta],
    index: &'a SiteIndex,
    /// Whether entity stocks were gathered into site order for this rule.
    entities_gathered: bool,
}

/// One block's exclusive working set.
struct Block<'a> {
    sites: Range<u32>,
    site_stock: &'a mut [u64],
    /// Entity stocks of this block's entities, in site order; empty if not gathered.
    ent_stock: &'a mut [u64],
    /// Position in site order of this block's first entity.
    ent_base: usize,
    deltas: &'a [Delta],
}

/// Where a holder's quantity lives, seen from one block.
#[derive(Clone, Copy)]
enum Place {
    /// Inside this block: its site, and its slot in the block's site or entity slice.
    Local { site: u32, entity: bool, slot: usize },
    /// A real holder at a site owned by another block.
    Remote,
    /// `Source` or `Sink`.
    Boundary,
}

fn touches_entity(delta: &Delta) -> bool {
    matches!(delta, Delta::Transfer { from: Holder::Entity(_), .. } | Delta::Transfer { to: Holder::Entity(_), .. })
}

fn place(ctx: &Context, block: &Block, holder: Holder, r: usize) -> Result<Place, LabError> {
    match holder {
        Holder::Source | Holder::Sink => Ok(Place::Boundary),
        Holder::Site(s) if s >= ctx.n_sites => Err(LabError::Config(format!("transfer refers to site {s}, which does not exist"))),
        Holder::Site(s) if block.sites.contains(&s) => {
            Ok(Place::Local { site: s, entity: false, slot: (s - block.sites.start) as usize * ctx.k + r })
        }
        Holder::Site(_) => Ok(Place::Remote),
        Holder::Entity(e) => {
            let Some(meta) = ctx.entities.get(e as usize) else {
                return Err(LabError::Config(format!("transfer refers to entity index {e}, which does not exist")));
            };
            if block.sites.contains(&meta.site) {
                debug_assert!(ctx.entities_gathered);
                Ok(Place::Local { site: meta.site, entity: true, slot: (ctx.index.position(e) - block.ent_base) * ctx.k + r })
            } else {
                Ok(Place::Remote)
            }
        }
    }
}

fn process_block(ctx: &Context, block: Block) -> Result<BlockOut, LabError> {
    let mut out = BlockOut { deposits: Vec::new(), moves: Vec::new(), influx: vec![0; ctx.k], dissipated: vec![0; ctx.k] };
    let outside = || LabError::Config("a delta's home site lies outside the site range the rule was asked to cover".into());
    for &delta in block.deltas {
        match delta {
            Delta::Move { entity, to } => {
                let Some(meta) = ctx.entities.get(entity as usize) else {
                    return Err(LabError::Config(format!("move refers to entity index {entity}, which does not exist")));
                };
                if !block.sites.contains(&meta.site) {
                    return Err(outside());
                }
                out.moves.push((entity, to));
            }
            Delta::Transfer { from, to, resource, amount } => {
                let r = resource as usize;
                if r >= ctx.k {
                    return Err(LabError::Config(format!("transfer refers to resource {r}, but there are only {}", ctx.k)));
                }
                if from == Holder::Sink || to == Holder::Source || (from == Holder::Source && to == Holder::Sink) {
                    return Err(LabError::Config(format!("invalid transfer direction {from:?} -> {to:?}")));
                }
                let from_place = place(ctx, &block, from, r)?;
                let to_place = place(ctx, &block, to, r)?;
                if from == to {
                    continue;
                }
                match (from_place, to_place) {
                    (Place::Remote, _) | (Place::Boundary, Place::Remote) => return Err(outside()),
                    (Place::Boundary, Place::Boundary) => unreachable!("direction was validated above"),
                    (Place::Boundary, Place::Local { entity, slot, .. }) => {
                        let stock = if entity { &mut block.ent_stock[slot] } else { &mut block.site_stock[slot] };
                        let credited = stock.checked_add(amount).ok_or(LabError::Overflow("resource stock"))?;
                        let influx = out.influx[r].checked_add(amount).ok_or(LabError::Overflow("influx ledger"))?;
                        *stock = credited;
                        out.influx[r] = influx;
                    }
                    (Place::Local { site: from_site, entity: from_entity, slot: from_slot }, to_place) => {
                        let have = if from_entity { block.ent_stock[from_slot] } else { block.site_stock[from_slot] };
                        let moved = amount.min(have);
                        match to_place {
                            Place::Boundary => {
                                out.dissipated[r] = out.dissipated[r].checked_add(moved).ok_or(LabError::Overflow("dissipation ledger"))?;
                            }
                            Place::Local { site, entity, slot } if site == from_site => {
                                let stock = if entity { &mut block.ent_stock[slot] } else { &mut block.site_stock[slot] };
                                *stock = stock.checked_add(moved).ok_or(LabError::Overflow("resource stock"))?;
                            }
                            _ => {
                                if moved > 0 {
                                    out.deposits.push((to, resource, moved));
                                }
                            }
                        }
                        let stock = if from_entity { &mut block.ent_stock[from_slot] } else { &mut block.site_stock[from_slot] };
                        *stock = have - moved;
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Applies one rule's proposals. `proposals[i]` must hold the deltas whose home sites lie in
/// `blocks[i]`; the blocks must be contiguous, ascending and cover every site.
///
/// Returns whether any entity changed site. On error the state must be discarded.
pub(crate) fn apply_blocks(
    state: &mut State,
    index: &SiteIndex,
    blocks: &[Range<u32>],
    proposals: &[Vec<Delta>],
    gathered: &mut Vec<u64>,
    pool: Option<&rayon::ThreadPool>,
) -> Result<bool, LabError> {
    let k = state.k;
    let n_entities = state.entities.len();
    let entities_gathered = proposals.iter().any(|p| p.iter().any(touches_entity));

    // Entity stocks are stored by id; gather them into site order so each block owns a contiguous slice.
    // `gathered` is scratch space reused across calls, so large worlds do not reallocate every rule.
    gathered.resize(if entities_gathered { n_entities * k } else { 0 }, 0);
    if entities_gathered {
        let ent_stock = &state.ent_stock;
        let copy = |(dst, &entity): (&mut [u64], &u32)| dst.copy_from_slice(&ent_stock[entity as usize * k..][..k]);
        match pool {
            Some(pool) => pool.install(|| gathered.par_chunks_mut(k).zip(index.items().par_iter()).for_each(copy)),
            None => gathered.chunks_mut(k).zip(index.items()).for_each(copy),
        }
    }

    let ctx = Context { k, n_sites: state.topo.n_sites(), entities: &state.entities, index, entities_gathered };
    let mut jobs = Vec::with_capacity(blocks.len());
    let mut sites_rest: &mut [u64] = &mut state.site_stock;
    let mut ents_rest: &mut [u64] = gathered;
    for (sites, deltas) in blocks.iter().zip(proposals) {
        let span = index.span(sites.clone());
        let (site_stock, rest) = sites_rest.split_at_mut((sites.end - sites.start) as usize * k);
        sites_rest = rest;
        let (ent_stock, rest) = ents_rest.split_at_mut(if entities_gathered { (span.end - span.start) * k } else { 0 });
        ents_rest = rest;
        jobs.push(Block { sites: sites.clone(), site_stock, ent_stock, ent_base: span.start, deltas });
    }
    let results: Vec<Result<BlockOut, LabError>> = match pool {
        Some(pool) => pool.install(|| jobs.into_par_iter().map(|job| process_block(&ctx, job)).collect()),
        None => jobs.into_iter().map(|job| process_block(&ctx, job)).collect(),
    };
    let outs = results.into_iter().collect::<Result<Vec<BlockOut>, LabError>>()?;

    if entities_gathered {
        let gathered = &*gathered;
        let copy = |(entity, dst): (usize, &mut [u64])| dst.copy_from_slice(&gathered[index.position(entity as u32) * k..][..k]);
        match pool {
            Some(pool) => pool.install(|| state.ent_stock.par_chunks_mut(k).enumerate().for_each(copy)),
            None => state.ent_stock.chunks_mut(k).enumerate().for_each(copy),
        }
    }

    // Ordered finishing pass: ledgers, deferred credits, then moves.
    let mut moved_any = false;
    for out in &outs {
        for r in 0..k {
            state.ledger.influx[r] = state.ledger.influx[r].checked_add(out.influx[r]).ok_or(LabError::Overflow("influx ledger"))?;
            state.ledger.dissipated[r] =
                state.ledger.dissipated[r].checked_add(out.dissipated[r]).ok_or(LabError::Overflow("dissipation ledger"))?;
        }
    }
    for out in &outs {
        for &(to, resource, amount) in &out.deposits {
            let stock = match to {
                Holder::Site(s) => &mut state.site_stock[s as usize * k + resource as usize],
                Holder::Entity(e) => &mut state.ent_stock[e as usize * k + resource as usize],
                Holder::Source | Holder::Sink => unreachable!("deferred credits always target a real holder"),
            };
            *stock = stock.checked_add(amount).ok_or(LabError::Overflow("resource stock"))?;
        }
    }
    let n_sites = state.topo.n_sites();
    for out in &outs {
        for &(entity, to) in &out.moves {
            let meta = &state.entities[entity as usize];
            if to >= n_sites || !state.topo.neighbours(meta.site).contains(&to) {
                return Err(LabError::Config(format!("entity {} cannot move from site {} to non-adjacent site {to}", meta.id, meta.site)));
            }
            state.entities[entity as usize].site = to;
            moved_any = true;
        }
    }
    Ok(moved_any)
}

/// Applies a list of deltas as one rule step, single-threaded, treating the whole world as one block.
pub fn apply(state: &mut State, deltas: &[Delta]) -> Result<(), LabError> {
    let index = state.site_index();
    let n_sites = state.topo.n_sites();
    apply_blocks(state, &index, std::slice::from_ref(&(0..n_sites)), std::slice::from_ref(&deltas.to_vec()), &mut Vec::new(), None).map(|_| ())
}
