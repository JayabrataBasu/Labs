//! DR-014: application semantics that are independent of chunking and thread count,
//! checked against a deliberately naive sequential oracle.
use lab_core::{apply, state_hash, Delta, Engine, EntityMeta, Holder, LabError, Rule, SiteIndex, State, Topology};
use lab_rng::{below, RuleRng};
use std::ops::Range;

fn t(from: Holder, to: Holder, amount: u64) -> Delta {
    Delta::Transfer { from, to, resource: 0, amount }
}

#[test]
fn inter_site_deposits_arrive_after_all_withdrawals() {
    // Site 0 holds 10, site 1 holds 0. Site 1 cannot pass on what it has not yet received.
    let topo = Topology::lattice(3, 1, false).unwrap();
    let mut s = State::new(topo, 1, vec![10, 0, 0], vec![], vec![]).unwrap();
    apply(&mut s, &[t(Holder::Site(0), Holder::Site(1), 10), t(Holder::Site(1), Holder::Site(2), 5)]).unwrap();
    assert_eq!(s.site_stock, vec![0, 10, 0]);
}

#[test]
fn same_site_transfers_are_immediate_and_ordered() {
    let topo = Topology::lattice(1, 1, false).unwrap();
    let e = |id| EntityMeta { id, site: 0, parent: None, mother: None, born: 0 };
    let mut s = State::new(topo, 1, vec![4], vec![e(0), e(1)], vec![0, 0]).unwrap();
    // Entity 0 receives 4 from the site and can hand 3 on to entity 1 within the same rule.
    apply(&mut s, &[t(Holder::Site(0), Holder::Entity(0), 4), t(Holder::Entity(0), Holder::Entity(1), 3)]).unwrap();
    assert_eq!((s.site_stock[0], s.ent_stock[0], s.ent_stock[1]), (0, 1, 3));
}

#[test]
fn entity_to_entity_across_sites_is_deferred() {
    let topo = Topology::lattice(2, 1, false).unwrap();
    let e = |id, site| EntityMeta { id, site, parent: None, mother: None, born: 0 };
    let mut s = State::new(topo, 1, vec![0, 0], vec![e(0, 0), e(1, 1)], vec![6, 0]).unwrap();
    apply(&mut s, &[t(Holder::Entity(0), Holder::Entity(1), 6), t(Holder::Entity(1), Holder::Sink, 6)]).unwrap();
    assert_eq!(s.ent_stock, vec![0, 6], "entity 1 had nothing to dissipate when its turn came");
    assert_eq!(s.ledger.dissipated, vec![0]);
}

// ---- rules used for the differential test -------------------------------------------------

macro_rules! rule {
    ($name:ident, $id:literal, |$state:ident, $index:ident, $rng:ident, $sites:ident, $out:ident| $body:block) => {
        struct $name(u16);
        impl Rule for $name {
            fn id(&self) -> &'static str { $id }
            fn version(&self) -> u32 { 1 }
            fn stream(&self) -> u16 { self.0 }
            #[allow(unused_variables)]
            fn propose(&self, $state: &State, $index: &SiteIndex, $rng: RuleRng, $sites: Range<u32>, $out: &mut Vec<Delta>) $body
        }
    };
}
rule!(Sprinkle, "test.sprinkle", |state, index, rng, sites, out| {
    for s in sites {
        out.push(t(Holder::Source, Holder::Site(s), u64::from(below(rng.draw(state.tick as u32, u64::from(s), 0)[0], 4))));
    }
});
// Offers its whole stock to every neighbour in turn: heavy clipping across sites.
rule!(Relay, "test.relay", |state, index, rng, sites, out| {
    for s in sites {
        for &n in state.topo.neighbours(s) {
            out.push(t(Holder::Site(s), Holder::Site(n), state.site_stock[s as usize]));
        }
    }
});
rule!(Grab, "test.grab", |state, index, rng, sites, out| {
    for s in sites {
        for &e in index.at(s) {
            out.push(t(Holder::Site(s), Holder::Entity(e), 2));
        }
    }
});
// Each entity gives 1 unit to the first entity at its northern neighbour site, else burns it.
rule!(Gift, "test.gift", |state, index, rng, sites, out| {
    for s in sites {
        let north = state.topo.neighbours(s)[0];
        for &e in index.at(s) {
            match index.at(north).first() {
                Some(&other) => out.push(t(Holder::Entity(e), Holder::Entity(other), 1)),
                None => out.push(t(Holder::Entity(e), Holder::Sink, 1)),
            }
        }
    }
});
rule!(Shuffle, "test.shuffle", |state, index, rng, sites, out| {
    for s in sites {
        let nb = state.topo.neighbours(s);
        for &e in index.at(s) {
            let d = rng.draw(state.tick as u32, state.entities[e as usize].id, 0);
            out.push(Delta::Move { entity: e, to: nb[below(d[0], nb.len() as u32) as usize] });
        }
    }
});
// Violates the contract: proposes a withdrawal from a site far outside the range it was given.
rule!(Trespass, "test.trespass", |state, index, rng, sites, out| {
    let far = (sites.start + 2000) % state.topo.n_sites();
    out.push(t(Holder::Site(far), Holder::Sink, 1));
});

fn rules() -> Vec<Box<dyn Rule>> {
    vec![Box::new(Sprinkle(1)), Box::new(Relay(2)), Box::new(Grab(3)), Box::new(Gift(4)), Box::new(Shuffle(5))]
}
fn world(seed: u64) -> State {
    let topo = Topology::lattice(70, 40, true).unwrap(); // 2,800 sites: three scheduler chunks
    let n = topo.n_sites();
    let init = RuleRng { seed, stream: 0 };
    let metas = (0..3000u64).map(|i| EntityMeta { id: i, site: below(init.draw(0, i, 0)[0], n), parent: None, mother: None, born: 0 }).collect();
    State::new(topo, 1, vec![3; n as usize], metas, vec![1; 3000]).unwrap()
}

/// The semantics of DR-014 written as plainly as possible, with no chunks and no threads.
fn oracle_apply(state: &mut State, deltas: &[Delta]) {
    let site_of: Vec<u32> = state.entities.iter().map(|e| e.site).collect();
    let home = |h: Holder| match h {
        Holder::Site(s) => Some(s),
        Holder::Entity(e) => Some(site_of[e as usize]),
        _ => None,
    };
    fn slot(state: &mut State, h: Holder) -> &mut u64 {
        match h {
            Holder::Site(s) => &mut state.site_stock[s as usize],
            Holder::Entity(e) => &mut state.ent_stock[e as usize],
            _ => unreachable!(),
        }
    }
    let (mut deposits, mut moves) = (Vec::new(), Vec::new());
    for delta in deltas {
        match *delta {
            Delta::Move { entity, to } => moves.push((entity, to)),
            Delta::Convert { .. } | Delta::Relocate { .. } | Delta::Remove { .. } | Delta::Spawn { .. } => unreachable!("not used by these rules"),
            Delta::Transfer { from, to, amount, .. } => {
                let moved = if from == Holder::Source {
                    state.ledger.influx[0] += amount;
                    amount
                } else {
                    let have = slot(state, from);
                    let moved = amount.min(*have);
                    *have -= moved;
                    moved
                };
                if to == Holder::Sink {
                    state.ledger.dissipated[0] += moved;
                } else if from == Holder::Source || home(from) == home(to) {
                    *slot(state, to) += moved;
                } else {
                    deposits.push((to, moved));
                }
            }
        }
    }
    for (to, moved) in deposits {
        *slot(state, to) += moved;
    }
    for (entity, to) in moves {
        state.entities[entity as usize].site = to;
    }
}

#[test]
fn engine_matches_sequential_oracle() {
    for seed in [1u64, 2, 3] {
        let engine = Engine::new(rules(), seed, 3).unwrap();
        let mut actual = world(seed);
        let mut expected = world(seed);
        let oracle_rules = rules();
        for tick in 0..25 {
            engine.step(&mut actual).unwrap();
            for rule in &oracle_rules {
                let mut deltas = Vec::new();
                let index = expected.site_index();
                rule.propose(&expected, &index, RuleRng { seed, stream: rule.stream() }, 0..expected.topo.n_sites(), &mut deltas);
                oracle_apply(&mut expected, &deltas);
            }
            expected.tick += 1;
            assert_eq!(actual, expected, "seed {seed}: engine and oracle diverged at tick {tick}");
        }
        assert!(actual.ledger.dissipated[0] > 0 && actual.ledger.influx[0] > 0);
    }
}

#[test]
fn engine_result_is_thread_independent_under_heavy_clipping() {
    let run = |threads| {
        let engine = Engine::new(rules(), 9, threads).unwrap();
        let mut s = world(9);
        for _ in 0..25 {
            engine.step(&mut s).unwrap();
        }
        state_hash(&s, &[0; 32])
    };
    let reference = run(1);
    assert_eq!(run(2), reference);
    assert_eq!(run(7), reference);
}

#[test]
fn rule_proposing_outside_its_range_is_rejected() {
    let engine = Engine::new(vec![Box::new(Trespass(1))], 1, 2).unwrap();
    let mut s = world(1);
    match engine.step(&mut s) {
        Err(LabError::Config(m)) => assert!(m.contains("test.trespass"), "{m}"),
        other => panic!("expected the rule to be rejected, got {other:?}"),
    }
}
