//! Observation records what actually moved, and never changes what happens (DR-021).
use lab_core::{apply, state_hash, Delta, Engine, EntityMeta, Flow, Holder, Observation, Rule, SiteIndex, State, Topology};
use lab_rng::RuleRng;
use std::ops::Range;

/// One step of fixed deltas, proposed for the site block that contains each delta's home.
struct Fixed(u16);
impl Rule for Fixed {
    fn id(&self) -> &'static str { "test.fixed" }
    fn version(&self) -> u32 { 1 }
    fn stream(&self) -> u16 { self.0 }
    fn propose(&self, state: &State, index: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for site in sites {
            for &e in index.at(site) {
                // Take up to 5 of resource 0, turn 4 of it into resource 1 (a quarter leaks), give 2 of 0 back.
                out.push(Delta::Transfer { from: Holder::Site(site), to: Holder::Entity(e), resource: 0, amount: 5 });
                out.push(Delta::Convert { entity: e, from: 0, to: 1, amount: 4, yield_num: 1, yield_den: 1, leak_ppm: 250_000, leak_draw: 0, leak_to: Holder::Site(state.entities[e as usize].site) });
                out.push(Delta::Transfer { from: Holder::Entity(e), to: Holder::Site(site), resource: 0, amount: 2 });
                if state.entities[e as usize].id == 1 && state.tick == 1 {
                    out.push(Delta::Spawn { parent: e, genome: vec![] });
                }
                if state.entities[e as usize].id == 0 && state.tick == 1 {
                    out.push(Delta::Remove { entity: e, recycle: true });
                }
            }
        }
    }
}

fn world() -> State {
    let e = |id, site| EntityMeta { id, site, parent: None, mother: None, born: 0 };
    State::new(Topology::lattice(2, 1, false).unwrap(), 2, vec![100, 0, 3, 0], vec![e(0, 0), e(1, 1)], vec![0; 4]).unwrap()
}

#[test]
fn flows_record_the_amounts_that_really_moved() {
    let engine = Engine::new(vec![Box::new(Fixed(1))], 1, 1).unwrap();
    let mut s = world();
    let mut obs = Observation::default();
    engine.step_observed(&mut s, &mut obs).unwrap();
    let flow = |entity, site, resource, taken, released, produced| Flow { entity, site, resource, taken, released, produced };
    // Entity 1 finds only 3 at its site: takes 3, converts 3 (leak 0 of 3: 0.75 rounds up with draw 0), returns 0.
    assert_eq!(
        obs.flows(),
        &[
            flow(0, 0, 0, 5, 1, 0),
            flow(0, 0, 1, 0, 1, 4),
            flow(1, 1, 0, 3, 0, 0),
            flow(1, 1, 1, 0, 1, 3),
        ]
    );
    assert!(obs.births.is_empty() && obs.deaths.is_empty());
}

#[test]
fn births_deaths_and_recycled_stocks_are_recorded() {
    let engine = Engine::new(vec![Box::new(Fixed(1))], 1, 1).unwrap();
    let mut s = world();
    engine.step(&mut s).unwrap();
    let held: Vec<u64> = s.ent_stock[0..2].to_vec();
    let mut obs = Observation::default();
    engine.step_observed(&mut s, &mut obs).unwrap();
    assert_eq!(obs.births.iter().map(|b| (b.tick, b.child, b.mother)).collect::<Vec<_>>(), vec![(1, 2, 1)]);
    assert_eq!(obs.deaths.iter().map(|d| (d.tick, d.id)).collect::<Vec<_>>(), vec![(1, 0)]);
    let released_b = obs.flows().iter().find(|f| f.entity == 0 && f.resource == 1).unwrap().released;
    assert!(released_b >= held[1], "what the dead entity held counts as released: {released_b} vs {}", held[1]);
}

#[test]
fn observing_changes_nothing_and_windows_accumulate() {
    let run = |observe: bool, threads: usize| {
        let engine = Engine::new(vec![Box::new(Fixed(1))], 1, threads).unwrap();
        let mut s = world();
        let mut obs = Observation::default();
        for _ in 0..6 {
            if observe { engine.step_observed(&mut s, &mut obs).unwrap() } else { engine.step(&mut s).unwrap() }
        }
        (state_hash(&s, &[0; 32]), obs.flows().to_vec(), s.ledger.converted_in[1])
    };
    let (plain, none, _) = run(false, 1);
    let (watched, flows, converted) = run(true, 1);
    assert_eq!(plain, watched);
    assert!(none.is_empty());
    assert_eq!(flows.iter().map(|f| f.produced).sum::<u64>(), converted, "everything produced is observed");
    assert!(flows.windows(2).all(|w| (w[0].entity, w[0].site, w[0].resource) < (w[1].entity, w[1].site, w[1].resource)), "coalesced and sorted");
    assert_eq!(run(true, 4).1, flows);
    let _ = apply;
}

#[test]
fn removing_entities_for_a_fork_conserves_everything() {
    let mut s = world();
    s.ent_stock = vec![0; 4];
    let engine = Engine::new(vec![Box::new(Fixed(1))], 1, 1).unwrap();
    engine.step(&mut s).unwrap();
    let lost: u64 = s.ent_stock[0..2].iter().sum();
    let before = s.ledger.dissipated.iter().sum::<u64>();
    assert_eq!(s.retain_entities(&[1]), 1);
    assert_eq!(s.entities.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1]);
    assert_eq!((s.ent_stock.len(), s.genomes.len(), s.deaths), (2, 1, 1));
    assert_eq!(s.ledger.dissipated.iter().sum::<u64>(), before + lost);
    lab_core::check_conservation(&s).unwrap();
}
