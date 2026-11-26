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
//! 5. Conversions happen inside one entity and are immediate. A leaking share of the product
//!    goes at once to the entity's own site (or to the sink).
//! 6. Moves and relocations take effect next, in proposal order. Contained entities cannot move
//!    by themselves; they are carried to their host's site. Then attachments and detachments
//!    take effect, in proposal order.
//! 7. Deaths and births take effect last, in proposal order. A removed entity returns what
//!    it holds to its host if it has a living one, else to its site (`recycle`), or dissipates
//!    it; an entity removed earlier in the same step cannot divide. Guests of a removed host
//!    become free.
//!
//! Sites therefore never read each other's stocks while a rule is applied, which is what
//! lets blocks of sites be processed on different threads with identical results.
use crate::{leaked_amount, Birth, Death, Delta, EntityMeta, Flow, Gene, Holder, LabError, Observation, SiteIndex, State};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::ops::Range;

/// What a block of sites hands back for the ordered finishing pass.
struct BlockOut {
    deposits: Vec<(Holder, u16, u64)>,
    /// (entity, destination, must be adjacent)
    moves: Vec<(u32, u32, bool)>,
    life: Vec<Life>,
    influx: Vec<u64>,
    dissipated: Vec<u64>,
    converted_in: Vec<u64>,
    converted_out: Vec<u64>,
    /// What moved, if anyone is watching.
    flows: Vec<Flow>,
    containment: Vec<Containment>,
}

enum Containment {
    Attach { guest: u32, host: u32 },
    Detach(u32),
}

enum Life {
    Remove(u32, bool),
    Spawn(u32, Vec<Gene>),
}

/// Read-only facts every block needs.
struct Context<'a> {
    k: usize,
    n_sites: u32,
    entities: &'a [EntityMeta],
    index: &'a SiteIndex,
    /// Whether entity stocks were gathered into site order for this rule.
    entities_gathered: bool,
    observe: bool,
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
    matches!(delta, Delta::Transfer { from: Holder::Entity(_), .. } | Delta::Transfer { to: Holder::Entity(_), .. } | Delta::Convert { .. })
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
    let zero = || vec![0u64; ctx.k];
    let mut out = BlockOut {
        deposits: Vec::new(),
        moves: Vec::new(),
        life: Vec::new(),
        influx: zero(),
        dissipated: zero(),
        converted_in: zero(),
        converted_out: zero(),
        flows: Vec::new(),
        containment: Vec::new(),
    };
    let outside = || LabError::Config("a delta's home site lies outside the site range the rule was asked to cover".into());
    // An entity-directed delta is at home in this block if the entity currently sits at one of its sites.
    let at_home = |entity: u32| -> Result<(), LabError> {
        match ctx.entities.get(entity as usize) {
            None => Err(LabError::Config(format!("delta refers to entity index {entity}, which does not exist"))),
            Some(meta) if !block.sites.contains(&meta.site) => Err(outside()),
            Some(_) => Ok(()),
        }
    };
    for delta in block.deltas {
        match *delta {
            Delta::Move { entity, to } => {
                at_home(entity)?;
                out.moves.push((entity, to, true));
            }
            Delta::Relocate { entity, to } => {
                at_home(entity)?;
                out.moves.push((entity, to, false));
            }
            Delta::Remove { entity, recycle } => {
                at_home(entity)?;
                out.life.push(Life::Remove(entity, recycle));
            }
            Delta::Attach { guest, host } => {
                at_home(guest)?;
                at_home(host)?;
                if ctx.entities[guest as usize].site != ctx.entities[host as usize].site || guest == host {
                    return Err(LabError::Config("an entity can only be contained in another one at the same site".into()));
                }
                out.containment.push(Containment::Attach { guest, host });
            }
            Delta::Detach { guest } => {
                at_home(guest)?;
                out.containment.push(Containment::Detach(guest));
            }
            Delta::Spawn { parent, ref genome } => {
                at_home(parent)?;
                out.life.push(Life::Spawn(parent, genome.clone()));
            }
            Delta::Convert { entity, from, to, amount, yield_num, yield_den, leak_ppm, leak_draw, leak_to } => {
                at_home(entity)?;
                let (rf, rt) = (from as usize, to as usize);
                if rf >= ctx.k || rt >= ctx.k || rf == rt {
                    return Err(LabError::Config(format!("conversion {rf} -> {rt} is not between two distinct existing resources")));
                }
                if yield_den == 0 || yield_num > yield_den {
                    return Err(LabError::Config(format!("conversion yield {yield_num}/{yield_den} must be between 0 and 1")));
                }
                if leak_ppm > 1_000_000 {
                    return Err(LabError::Config(format!("conversion leak {leak_ppm} ppm exceeds 1000000")));
                }
                let base = (ctx.index.position(entity) - block.ent_base) * ctx.k;
                let consumed = amount.min(block.ent_stock[base + rf]);
                let produced = (u128::from(consumed) * u128::from(yield_num) / u128::from(yield_den)) as u64;
                let leaked = leaked_amount(produced, leak_ppm, leak_draw);
                let credited = block.ent_stock[base + rt].checked_add(produced - leaked).ok_or(LabError::Overflow("resource stock"))?;
                if ctx.observe && produced > 0 {
                    let meta = &ctx.entities[entity as usize];
                    let released = if matches!(leak_to, Holder::Site(_)) { leaked } else { 0 };
                    out.flows.push(Flow { entity: meta.id, site: meta.site, resource: to, taken: 0, released, produced });
                }
                // Resolve where the leak goes before changing anything; it must be at this entity's site.
                let leak_target = if leaked == 0 {
                    None
                } else {
                    let site = ctx.entities[entity as usize].site;
                    let local = match leak_to {
                        Holder::Sink => true,
                        Holder::Site(s) => s == site,
                        Holder::Entity(e) => e != entity && ctx.entities.get(e as usize).is_some_and(|m| m.site == site),
                        Holder::Source => false,
                    };
                    if !local {
                        return Err(LabError::Config(format!("a leak must go to the entity's own site, a holder there, or the sink, not {leak_to:?}")));
                    }
                    Some(place(ctx, &block, leak_to, rt)?)
                };
                out.converted_out[rf] = out.converted_out[rf].checked_add(consumed).ok_or(LabError::Overflow("conversion ledger"))?;
                out.converted_in[rt] = out.converted_in[rt].checked_add(produced).ok_or(LabError::Overflow("conversion ledger"))?;
                block.ent_stock[base + rf] -= consumed;
                block.ent_stock[base + rt] = credited;
                match leak_target {
                    None => {}
                    Some(Place::Boundary) => out.dissipated[rt] = out.dissipated[rt].checked_add(leaked).ok_or(LabError::Overflow("dissipation ledger"))?,
                    Some(Place::Local { entity: to_entity, slot, .. }) => {
                        let stock = if to_entity { &mut block.ent_stock[slot] } else { &mut block.site_stock[slot] };
                        *stock = stock.checked_add(leaked).ok_or(LabError::Overflow("resource stock"))?;
                    }
                    Some(Place::Remote) => return Err(outside()),
                }
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
                        if ctx.observe && moved > 0 {
                            let record = |entity: u32, site: u32, taken, released| Flow { entity: ctx.entities[entity as usize].id, site, resource, taken, released, produced: 0 };
                            match (from, to) {
                                (Holder::Site(s), Holder::Entity(e)) => out.flows.push(record(e, s, moved, 0)),
                                (Holder::Entity(e), Holder::Site(s)) => out.flows.push(record(e, s, 0, moved)),
                                _ => {}
                            }
                        }
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
/// Returns whether entity positions or the set of entities changed. On error the state must be discarded.
pub(crate) fn apply_blocks(
    state: &mut State,
    index: &SiteIndex,
    blocks: &[Range<u32>],
    proposals: &[Vec<Delta>],
    gathered: &mut Vec<u64>,
    pool: Option<&rayon::ThreadPool>,
    mut observation: Option<&mut Observation>,
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

    let ctx = Context { k, n_sites: state.topo.n_sites(), entities: &state.entities, index, entities_gathered, observe: observation.is_some() };
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

    // Ordered finishing pass: ledgers, deferred credits, moves, then deaths and births.
    let add = |ledger: &mut [u64], part: &[u64], what: &'static str| -> Result<(), LabError> {
        for (total, &x) in ledger.iter_mut().zip(part) {
            *total = total.checked_add(x).ok_or(LabError::Overflow(what))?;
        }
        Ok(())
    };
    for out in &outs {
        add(&mut state.ledger.influx, &out.influx, "influx ledger")?;
        add(&mut state.ledger.dissipated, &out.dissipated, "dissipation ledger")?;
        add(&mut state.ledger.converted_in, &out.converted_in, "conversion ledger")?;
        add(&mut state.ledger.converted_out, &out.converted_out, "conversion ledger")?;
    }
    if let Some(seen) = observation.as_deref_mut() {
        for out in &outs {
            seen.extend(&out.flows);
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
    let mut changed = false;
    let n_sites = state.topo.n_sites();
    for out in &outs {
        for &(entity, to, adjacent) in &out.moves {
            let meta = &state.entities[entity as usize];
            if meta.parent.is_some() {
                return Err(LabError::Config(format!("entity {} is contained and cannot move on its own", meta.id)));
            }
            if to >= n_sites || (adjacent && !state.topo.neighbours(meta.site).contains(&to)) {
                return Err(LabError::Config(format!("entity {} cannot move from site {} to site {to}", meta.id, meta.site)));
            }
            state.entities[entity as usize].site = to;
            changed = true;
        }
    }
    if changed {
        carry_guests(state);
    }
    if outs.iter().any(|out| !out.containment.is_empty()) {
        changed = true;
        apply_containment(state, &outs)?;
    }
    if outs.iter().any(|out| !out.life.is_empty()) {
        changed = true;
        let mut dead = vec![false; n_entities];
        for out in outs {
            for event in out.life {
                match event {
                    Life::Remove(entity, recycle) => {
                        let e = entity as usize;
                        if dead[e] {
                            continue;
                        }
                        dead[e] = true;
                        let site = state.entities[e].site as usize;
                        // `host_of` searches by id, which still works: nothing is removed from the list until later.
                        let host = state.host_of(e).filter(|&h| h < n_entities);
                        if let Some(seen) = observation.as_deref_mut() {
                            let id = state.entities[e].id;
                            seen.deaths.push(Death { tick: state.tick, id });
                            if recycle && !host.is_some_and(|h| !dead[h]) {
                                let held: Vec<Flow> = (0..k)
                                    .filter(|r| state.ent_stock[e * k + r] > 0)
                                    .map(|r| Flow { entity: id, site: site as u32, resource: r as u16, taken: 0, released: state.ent_stock[e * k + r], produced: 0 })
                                    .collect();
                                seen.extend(&held);
                            }
                        }
                        let host = host.filter(|&h| !dead[h]);
                        for r in 0..k {
                            let held = std::mem::take(&mut state.ent_stock[e * k + r]);
                            let target = match (recycle, host) {
                                (false, _) => &mut state.ledger.dissipated[r],
                                (true, Some(h)) => &mut state.ent_stock[h * k + r],
                                (true, None) => &mut state.site_stock[site * k + r],
                            };
                            *target = target.checked_add(held).ok_or(LabError::Overflow("resource stock"))?;
                        }
                        state.deaths = state.deaths.checked_add(1).ok_or(LabError::Overflow("death counter"))?;
                    }
                    Life::Spawn(parent, genome) => {
                        let p = parent as usize;
                        if dead[p] {
                            continue;
                        }
                        if state.entities.len() >= u32::MAX as usize {
                            return Err(LabError::Overflow("number of entities"));
                        }
                        let id = state.next_entity_id;
                        state.next_entity_id = id.checked_add(1).ok_or(LabError::Overflow("entity id"))?;
                        for r in 0..k {
                            let share = state.ent_stock[p * k + r] / 2;
                            state.ent_stock[p * k + r] -= share;
                            state.ent_stock.push(share);
                        }
                        let host = state.host_of(p).filter(|&h| h < n_entities && !dead[h]);
                        let parent = host.map(|h| state.entities[h].id);
                        let mother = &state.entities[p];
                        if let Some(seen) = observation.as_deref_mut() {
                            seen.births.push(Birth { tick: state.tick, child: id, mother: mother.id });
                        }
                        let child = EntityMeta { id, site: mother.site, parent, mother: Some(mother.id), born: state.tick };
                        state.entities.push(child);
                        state.genomes.push(genome);
                        state.births = state.births.checked_add(1).ok_or(LabError::Overflow("birth counter"))?;
                    }
                }
            }
        }
        if dead.contains(&true) {
            let gone: Vec<u64> = (0..n_entities).filter(|&i| dead[i]).map(|i| state.entities[i].id).collect();
            for e in &mut state.entities {
                if e.parent.is_some_and(|p| gone.binary_search(&p).is_ok()) {
                    e.parent = None;
                }
            }
            // Offspring were appended after index `n_entities` and are never dead.
            let alive = |i: usize| i >= n_entities || !dead[i];
            let mut i = 0;
            state.entities.retain(|_| {
                i += 1;
                alive(i - 1)
            });
            let mut i = 0;
            state.genomes.retain(|_| {
                i += 1;
                alive(i - 1)
            });
            let mut i = 0;
            state.ent_stock.retain(|_| {
                i += 1;
                alive((i - 1) / k)
            });
        }
    }
    Ok(changed)
}

/// Puts every contained entity at its host's site.
fn carry_guests(state: &mut State) {
    for i in 0..state.entities.len() {
        if let Some(h) = state.host_of(i) {
            state.entities[i].site = state.entities[h].site;
        }
    }
}

fn apply_containment(state: &mut State, outs: &[BlockOut]) -> Result<(), LabError> {
    // Number of guests per host id, kept up to date so that one level of nesting is enforced.
    let mut guests: BTreeMap<u64, u32> = BTreeMap::new();
    for parent in state.entities.iter().filter_map(|e| e.parent) {
        *guests.entry(parent).or_default() += 1;
    }
    for event in outs.iter().flat_map(|out| &out.containment) {
        let (guest, host) = match *event {
            Containment::Attach { guest, host } => (guest as usize, Some(host as usize)),
            Containment::Detach(guest) => (guest as usize, None),
        };
        let new_parent = match host {
            Some(h) => {
                let (g_id, h_meta) = (state.entities[guest].id, &state.entities[h]);
                if h_meta.parent.is_some() || guests.get(&g_id).is_some_and(|&n| n > 0) {
                    return Err(LabError::Config(format!("entity {g_id} cannot be contained in {}: containment is one level deep", h_meta.id)));
                }
                Some(h_meta.id)
            }
            None => None,
        };
        if let Some(old) = state.entities[guest].parent {
            *guests.entry(old).or_default() -= 1;
        }
        if let Some(new) = new_parent {
            *guests.entry(new).or_default() += 1;
        }
        state.entities[guest].parent = new_parent;
    }
    Ok(())
}

/// Applies a list of deltas as one rule step, single-threaded, treating the whole world as one block.
pub fn apply(state: &mut State, deltas: &[Delta]) -> Result<(), LabError> {
    let index = state.site_index();
    let n_sites = state.topo.n_sites();
    apply_blocks(state, &index, std::slice::from_ref(&(0..n_sites)), std::slice::from_ref(&deltas.to_vec()), &mut Vec::new(), None, None).map(|_| ())
}
