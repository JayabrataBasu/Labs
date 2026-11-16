use lab_core::{decode, encode, state_hash, EntityMeta, LabError, State, Topology};

fn sample() -> State {
    let topo = Topology::lattice(3, 3, true).unwrap();
    let mut site_stock = vec![0u64; 9 * 2];
    site_stock[0] = 10;
    site_stock[17] = 7;
    let entities = vec![
        EntityMeta { id: 0, site: 2, parent: None },
        EntityMeta { id: 1, site: 0, parent: None },
        EntityMeta { id: 5, site: 2, parent: None },
    ];
    State::new(topo, 2, site_stock, entities, vec![1, 2, 3, 4, 5, 6]).unwrap()
}

#[test]
fn lattice_neighbours() {
    let t = Topology::lattice(3, 3, true).unwrap();
    assert_eq!(t.n_sites(), 9);
    assert_eq!(t.neighbours(0), &[6, 1, 3, 2]);
    let t = Topology::lattice(3, 3, false).unwrap();
    assert_eq!(t.neighbours(0), &[1, 3]);
    assert_eq!(t.neighbours(4), &[1, 5, 7, 3]);
}

#[test]
fn lattice_rejects_degenerate() {
    assert!(matches!(Topology::lattice(2, 5, true), Err(LabError::Config(_))));
    assert!(matches!(Topology::lattice(5, 2, true), Err(LabError::Config(_))));
    assert!(matches!(Topology::lattice(0, 5, false), Err(LabError::Config(_))));
    let t = Topology::lattice(1, 1, false).unwrap();
    assert!(t.neighbours(0).is_empty());
}

#[test]
fn new_computes_totals_and_next_id() {
    let s = sample();
    assert_eq!(s.initial_total, vec![10 + 1 + 3 + 5, 7 + 2 + 4 + 6]);
    assert_eq!(s.total(0).unwrap(), 19);
    assert_eq!(s.next_entity_id, 6);
    assert_eq!(s.ledger.influx, vec![0, 0]);
}

#[test]
fn new_rejects_inconsistent_input() {
    let topo = || Topology::lattice(3, 3, true).unwrap();
    let e = |id, site| EntityMeta { id, site, parent: None };
    assert!(State::new(topo(), 2, vec![0; 17], vec![], vec![]).is_err(), "site_stock length");
    assert!(State::new(topo(), 2, vec![0; 18], vec![e(0, 0)], vec![1]).is_err(), "ent_stock length");
    assert!(State::new(topo(), 2, vec![0; 18], vec![e(1, 0), e(0, 0)], vec![0; 4]).is_err(), "unsorted ids");
    assert!(State::new(topo(), 2, vec![0; 18], vec![e(0, 9)], vec![0; 2]).is_err(), "site out of range");
    assert!(State::new(topo(), 0, vec![], vec![], vec![]).is_err(), "k = 0");
}

#[test]
fn roundtrip() {
    let s = sample();
    let h = [7u8; 32];
    let bytes = encode(&s, &h);
    assert_eq!(&bytes[..4], b"LABS");
    let (s2, h2) = decode(&bytes).unwrap();
    assert_eq!(s2, s);
    assert_eq!(h2, h);
}

#[test]
fn hash_changes_on_any_field() {
    let base = sample();
    let h = [0u8; 32];
    let h0 = state_hash(&base, &h);
    assert_eq!(h0, state_hash(&sample(), &h), "hash is a pure function of state");
    let variants: Vec<Box<dyn Fn(&mut State)>> = vec![
        Box::new(|s| s.tick += 1),
        Box::new(|s| s.site_stock[3] += 1),
        Box::new(|s| s.entities[0].site = 1),
        Box::new(|s| s.entities[0].parent = Some(1)),
        Box::new(|s| s.ent_stock[0] += 1),
        Box::new(|s| s.ledger.influx[0] += 1),
        Box::new(|s| s.ledger.dissipated[1] += 1),
        Box::new(|s| s.next_entity_id += 1),
        Box::new(|s| s.initial_total[0] += 1),
    ];
    for (i, f) in variants.iter().enumerate() {
        let mut s = sample();
        f(&mut s);
        assert_ne!(state_hash(&s, &h), h0, "variant {i} did not change the hash");
    }
    assert_ne!(state_hash(&base, &[1u8; 32]), h0, "config hash is part of the hash");
}

#[test]
fn decode_rejects() {
    let bytes = encode(&sample(), &[0u8; 32]);
    let mut bad_magic = bytes.clone();
    bad_magic[0] = b'X';
    assert!(matches!(decode(&bad_magic), Err(LabError::Codec(_))));
    let mut bad_version = bytes.clone();
    bad_version[4] = 2;
    assert!(matches!(decode(&bad_version), Err(LabError::Codec(_))));
    assert!(matches!(decode(&bytes[..bytes.len() - 1]), Err(LabError::Codec(_))));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(decode(&trailing), Err(LabError::Codec(_))));
    assert!(matches!(decode(&[]), Err(LabError::Codec(_))));
}

#[test]
fn site_index_groups() {
    let s = sample();
    let idx = s.site_index();
    assert_eq!(idx.at(2), &[0, 2]);
    assert_eq!(idx.at(0), &[1]);
    assert!(idx.at(1).is_empty());
}
