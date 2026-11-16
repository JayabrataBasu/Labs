use lab_core::{state_hash, Delta, Engine, EntityMeta, Holder, LabError, Rule, SiteIndex, State, Topology};
use lab_rng::{below, RuleRng};
use std::ops::Range;

/// Adds 0..=2 units of resource 0 to every site.
struct Sprinkle(u16);
impl Rule for Sprinkle {
    fn id(&self) -> &'static str { "test.sprinkle" }
    fn version(&self) -> u32 { 1 }
    fn stream(&self) -> u16 { self.0 }
    fn propose(&self, state: &State, _: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for s in sites {
            let amount = u64::from(below(rng.draw(state.tick as u32, u64::from(s), 0)[0], 3));
            out.push(Delta::Transfer { from: Holder::Source, to: Holder::Site(s), resource: 0, amount });
        }
    }
}
/// Every entity moves to a random neighbour.
struct Shuffle(u16);
impl Rule for Shuffle {
    fn id(&self) -> &'static str { "test.shuffle" }
    fn version(&self) -> u32 { 1 }
    fn stream(&self) -> u16 { self.0 }
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for s in sites {
            let nb = state.topo.neighbours(s);
            for &e in index.at(s) {
                let d = rng.draw(state.tick as u32, state.entities[e as usize].id, 0);
                out.push(Delta::Move { entity: e, to: nb[below(d[0], nb.len() as u32) as usize] });
            }
        }
    }
}
/// Every entity takes one unit from its site.
struct Grab(u16);
impl Rule for Grab {
    fn id(&self) -> &'static str { "test.grab" }
    fn version(&self) -> u32 { 1 }
    fn stream(&self) -> u16 { self.0 }
    fn propose(&self, _: &State, index: &SiteIndex, _: RuleRng, sites: Range<u32>, out: &mut Vec<Delta>) {
        for s in sites {
            for &e in index.at(s) {
                out.push(Delta::Transfer { from: Holder::Site(s), to: Holder::Entity(e), resource: 0, amount: 1 });
            }
        }
    }
}

fn world(entities: u32) -> State {
    let topo = Topology::lattice(64, 64, true).unwrap();
    let init = RuleRng { seed: 99, stream: 0 };
    let metas = (0..entities)
        .map(|i| EntityMeta { id: u64::from(i), site: below(init.draw(0, u64::from(i), 0)[0], 4096), parent: None })
        .collect();
    State::new(topo, 1, vec![1; 4096], metas, vec![0; entities as usize]).unwrap()
}
fn rules(order: &[&str]) -> Vec<Box<dyn Rule>> {
    order
        .iter()
        .map(|&name| -> Box<dyn Rule> {
            match name {
                "sprinkle" => Box::new(Sprinkle(1)),
                "shuffle" => Box::new(Shuffle(2)),
                _ => Box::new(Grab(3)),
            }
        })
        .collect()
}
fn run(order: &[&str], seed: u64, threads: usize, entities: u32, ticks: u32) -> State {
    let engine = Engine::new(rules(order), seed, threads).unwrap();
    let mut s = world(entities);
    for _ in 0..ticks {
        engine.step(&mut s).unwrap();
    }
    s
}
const ALL: &[&str] = &["sprinkle", "grab", "shuffle"];
const H: [u8; 32] = [0; 32];

#[test]
fn thread_invariance() {
    let reference = state_hash(&run(ALL, 7, 1, 5000, 50), &H);
    for threads in [2, 5, 12] {
        assert_eq!(state_hash(&run(ALL, 7, threads, 5000, 50), &H), reference, "{threads} threads");
    }
}

#[test]
fn seed_sensitivity() {
    assert_ne!(state_hash(&run(ALL, 1, 2, 500, 10), &H), state_hash(&run(ALL, 2, 2, 500, 10), &H));
}

#[test]
fn rule_order_is_config_order() {
    let a = run(&["sprinkle", "grab", "shuffle"], 7, 2, 500, 10);
    let b = run(&["sprinkle", "shuffle", "grab"], 7, 2, 500, 10);
    assert_ne!(state_hash(&a, &H), state_hash(&b, &H));
}

#[test]
fn duplicate_or_reserved_stream_rejected() {
    let dup: Vec<Box<dyn Rule>> = vec![Box::new(Sprinkle(4)), Box::new(Grab(4))];
    assert!(matches!(Engine::new(dup, 1, 1), Err(LabError::Config(_))));
    let reserved: Vec<Box<dyn Rule>> = vec![Box::new(Sprinkle(0))];
    assert!(matches!(Engine::new(reserved, 1, 1), Err(LabError::Config(_))));
    assert!(matches!(Engine::new(rules(ALL), 1, 0), Err(LabError::Config(_))), "zero threads");
}

#[test]
fn ledger_matches_totals() {
    let s = run(ALL, 7, 3, 500, 50);
    assert_eq!(s.tick, 50);
    assert!(s.ledger.influx[0] > 0);
    assert_eq!(s.total(0).unwrap(), s.initial_total[0] + s.ledger.influx[0] - s.ledger.dissipated[0]);
}

#[test]
fn zero_entities_runs() {
    let s = run(ALL, 7, 2, 0, 5);
    assert_eq!(s.tick, 5);
    assert!(s.entities.is_empty());
}

#[test]
fn tick_limit_is_an_error() {
    let engine = Engine::new(rules(ALL), 1, 1).unwrap();
    let mut s = world(0);
    s.tick = u64::from(u32::MAX);
    assert!(matches!(engine.step(&mut s), Err(LabError::Config(_))));
}
