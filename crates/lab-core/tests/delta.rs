use lab_core::{apply, check_conservation, Delta, EntityMeta, Holder, LabError, State, Topology};

/// 3x3 non-wrapping lattice, 2 resources; site 4 holds 10 of resource 0; two entities at site 4.
fn world() -> State {
    let topo = Topology::lattice(3, 3, false).unwrap();
    let mut site_stock = vec![0u64; 18];
    site_stock[4 * 2] = 10;
    let entities = vec![EntityMeta { id: 0, site: 4, parent: None, mother: None, born: 0 }, EntityMeta { id: 1, site: 4, parent: None, mother: None, born: 0 }];
    State::new(topo, 2, site_stock, entities, vec![0, 0, 5, 0]).unwrap()
}
fn t(from: Holder, to: Holder, amount: u64) -> Delta {
    Delta::Transfer { from, to, resource: 0, amount }
}

#[test]
fn transfer_moves_exact() {
    let mut s = world();
    apply(&mut s, &[t(Holder::Site(4), Holder::Entity(0), 4)]).unwrap();
    assert_eq!(s.site_stock[8], 6);
    assert_eq!(s.ent_stock[0], 4);
    check_conservation(&s).unwrap();
}

#[test]
fn transfer_clips() {
    let mut s = world();
    apply(&mut s, &[t(Holder::Site(4), Holder::Entity(0), 15), t(Holder::Site(4), Holder::Entity(1), 3)]).unwrap();
    assert_eq!(s.site_stock[8], 0);
    assert_eq!(s.ent_stock[0], 10);
    assert_eq!(s.ent_stock[2], 5, "second request found the site empty");
}

#[test]
fn order_matters_and_is_slice_order() {
    let mut s = world();
    apply(&mut s, &[t(Holder::Site(4), Holder::Entity(1), 7), t(Holder::Site(4), Holder::Entity(0), 7)]).unwrap();
    assert_eq!(s.ent_stock[2], 5 + 7);
    assert_eq!(s.ent_stock[0], 3);
}

#[test]
fn source_and_sink_update_ledger() {
    let mut s = world();
    apply(&mut s, &[t(Holder::Source, Holder::Site(0), 5), t(Holder::Entity(1), Holder::Sink, 2), t(Holder::Entity(0), Holder::Sink, 9)]).unwrap();
    assert_eq!(s.ledger.influx, vec![5, 0]);
    assert_eq!(s.ledger.dissipated, vec![2, 0], "entity 0 had nothing, so its dissipation clipped to 0");
    assert_eq!(s.total(0).unwrap(), 15 + 5 - 2);
    check_conservation(&s).unwrap();
}

#[test]
fn conservation_detects_tamper() {
    let mut s = world();
    s.site_stock[0] += 1;
    assert_eq!(check_conservation(&s), Err(LabError::Conservation { resource: 0, expected: 15, actual: 16 }));
}

#[test]
fn overflow_is_error() {
    let topo = Topology::lattice(1, 1, false).unwrap();
    let mut s = State::new(topo, 1, vec![u64::MAX], vec![], vec![]).unwrap();
    assert!(matches!(apply(&mut s, &[t(Holder::Source, Holder::Site(0), 1)]), Err(LabError::Overflow(_))));
    assert_eq!(s.site_stock[0], u64::MAX, "a failed transfer leaves stocks untouched");
    assert_eq!(s.ledger.influx[0], 0);
}

#[test]
fn move_changes_site() {
    let mut s = world();
    apply(&mut s, &[Delta::Move { entity: 0, to: 1 }]).unwrap();
    assert_eq!(s.entities[0].site, 1);
    assert!(matches!(apply(&mut s, &[Delta::Move { entity: 0, to: 8 }]), Err(LabError::Config(_))), "site 8 is not adjacent to site 1");
    assert!(matches!(apply(&mut s, &[Delta::Move { entity: 0, to: 99 }]), Err(LabError::Config(_))));
    assert!(matches!(apply(&mut s, &[Delta::Move { entity: 7, to: 1 }]), Err(LabError::Config(_))));
}

#[test]
fn invalid_holders_rejected() {
    let bad = [
        t(Holder::Source, Holder::Sink, 1),
        t(Holder::Sink, Holder::Site(0), 1),
        t(Holder::Site(0), Holder::Source, 1),
        t(Holder::Site(9), Holder::Site(0), 1),
        t(Holder::Site(0), Holder::Entity(2), 1),
        Delta::Transfer { from: Holder::Site(0), to: Holder::Site(1), resource: 2, amount: 1 },
    ];
    for d in bad {
        let mut s = world();
        assert!(matches!(apply(&mut s, std::slice::from_ref(&d)), Err(LabError::Config(_))), "{d:?} should be rejected");
    }
}
