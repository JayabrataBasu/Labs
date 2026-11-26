//! One-level containment (DR-022): binding, shared fate, host-only exchange, inheritance, release.
use lab_core::config::{initial_state, parse};
use lab_core::{check_conservation, decode, encode, state_hash, Engine, State};

const NO_MUTATION: &str = "mutation = { point_ppm = 0, point_magnitude = 0, retarget_ppm = 0, duplicate_ppm = 0, delete_ppm = 0, max_genes = 16, kinds = [\"uptake\", \"convert\", \"secrete\"] }";

fn gene(kind: &str, fields: &str, level: u16) -> String {
    format!("{{ kind = \"{kind}\", {fields}, level = {level}, sense = \"always\", threshold = 0 }}")
}
fn bind(tag: &str) -> String {
    gene("bind", &format!("resource = \"{tag}\""), 1)
}
/// Resources E, A. `founders` are (count, stock, genes).
fn world(size: (u32, u32), site: [u64; 2], founders: &[(u64, [u64; 2], String)], rules: &[(&str, u32, String)]) -> String {
    let founders: Vec<String> = founders.iter().map(|(n, s, g)| format!("{{ count = {n}, stock = {s:?}, genome = [{g}] }}")).collect();
    let mut text = format!(
        "schema_version = 2\nseed = 3\n{}[world]\nwidth = {}\nheight = {}\nwrap = {}\nresources = [\"E\", \"A\"]\n[init]\nsite_stock = {site:?}\npatches = []\nfounders = [{}]\n",
        if rules.is_empty() { "rules = []\n" } else { "" },
        size.0,
        size.1,
        size.0 >= 3 && size.1 >= 3,
        founders.join(", ")
    );
    for (i, (id, version, params)) in rules.iter().enumerate() {
        text += &format!("[[rules]]\nid = \"{id}\"\nversion = {version}\nstream = {}\nparams = {{ {params} }}\n", i + 1);
    }
    text
}
fn bind_rule(ppm: u32, max_guests: u32) -> (&'static str, u32, String) {
    ("containment.bind", 1, format!("probability_ppm = {ppm}, max_guests = {max_guests}"))
}
fn metabolism(guests: &str) -> (&'static str, u32, String) {
    ("metabolism.genes", 3, format!("energy = \"E\", base_cost = 0, gene_cost = 0, yield_num = 1, yield_den = 1, reactions = [{{ from = \"A\", to = \"E\", leak_ppm = 500000 }}], release_to = \"site\", guests_trade_with = \"{guests}\""))
}
fn run(text: &str, ticks: u32, threads: usize) -> State {
    let (cfg, _) = parse(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    let engine = Engine::new(lab_rules::build(&cfg).unwrap(), cfg.seed, threads).unwrap();
    let mut s = initial_state(&cfg).unwrap();
    for _ in 0..ticks {
        engine.step(&mut s).unwrap();
        well_formed(&s);
    }
    s
}
/// Every guest has a living, free host at the same site.
fn well_formed(s: &State) {
    check_conservation(s).unwrap();
    for e in &s.entities {
        if let Some(p) = e.parent {
            let host = &s.entities[s.entities.binary_search_by_key(&p, |x| x.id).expect("host is alive")];
            assert_eq!((host.parent, host.site), (None, e.site), "guest {} of host {p}", e.id);
        }
    }
}
fn hosted(s: &State) -> usize {
    s.entities.iter().filter(|e| e.parent.is_some()).count()
}

#[test]
fn binding_needs_matching_bind_genes_on_the_same_site() {
    let pair = |a: String, b: String| run(&world((1, 1), [0, 0], &[(1, [0, 0], a), (1, [0, 0], b)], &[bind_rule(1_000_000, 4)]), 1, 1);
    assert_eq!(hosted(&pair(bind("A"), bind("A"))), 1, "one becomes the guest of the other");
    assert_eq!(hosted(&pair(bind("A"), bind("E"))), 0, "different tags do not bind");
    assert_eq!(hosted(&pair(bind("A"), String::new())), 0, "both must express it");
    assert_eq!(hosted(&run(&world((1, 1), [0, 0], &[(2, [0, 0], bind("A"))], &[bind_rule(0, 4)]), 5, 1)), 0);
}

#[test]
fn hosts_take_at_most_max_guests_and_nesting_is_one_level() {
    let s = run(&world((1, 1), [0, 0], &[(9, [0, 0], bind("A"))], &[bind_rule(1_000_000, 2)]), 12, 1);
    for host in s.entities.iter().filter(|e| e.parent.is_none()) {
        assert!(s.entities.iter().filter(|g| g.parent == Some(host.id)).count() <= 2);
    }
    assert_eq!(hosted(&s), 6, "nine entities settle into three hosts with two guests each");
}

#[test]
fn guests_move_with_their_host() {
    let rules = [bind_rule(1_000_000, 8), ("move.random", 1, "probability_ppm = 1000000".into()), ("mix.relocate", 1, "probability_ppm = 500000".into())];
    let s = run(&world((6, 6), [0, 0], &[(200, [0, 0], bind("A"))], &rules), 30, 3);
    assert!(hosted(&s) > 0);
}

/// (guest index, host index) of the only guest.
fn roles(s: &State) -> (usize, usize) {
    let guest = s.entities.iter().position(|e| e.parent.is_some()).expect("one entity is contained");
    (guest, s.host_of(guest).unwrap())
}

#[test]
fn guests_take_up_from_the_host_or_from_the_site() {
    let genes = [bind("A"), gene("uptake", "resource = \"E\"", 7)].join(", ");
    let founders = [(2, [50, 0], genes)];
    for (mode, host_e, site_e) in [("host", 50, 93), ("site", 57, 86)] {
        let s = run(&world((1, 1), [100, 0], &founders, &[bind_rule(1_000_000, 1), metabolism(mode)]), 1, 1);
        let (g, h) = roles(&s);
        assert_eq!((s.ent_stock[g * 2], s.ent_stock[h * 2], s.site_stock[0]), (57, host_e, site_e), "{mode}");
    }
}

#[test]
fn guests_leak_into_the_host_or_onto_the_site() {
    // Each turns 40 A into 40 E, half of which leaks.
    let genes = [bind("A"), gene("convert", "from = \"A\", to = \"E\"", 100)].join(", ");
    let founders = [(2, [0, 40], genes)];
    for (mode, host_e, site_e) in [("host", 40, 20), ("site", 20, 40)] {
        let s = run(&world((1, 1), [0, 0], &founders, &[bind_rule(1_000_000, 1), metabolism(mode)]), 1, 1);
        let (g, h) = roles(&s);
        assert_eq!((s.ent_stock[g * 2], s.ent_stock[h * 2], s.site_stock[0]), (20, host_e, site_e), "{mode}");
    }
}

#[test]
fn sharing_moves_a_fraction_of_the_difference_toward_the_poorer() {
    let rules = [bind_rule(1_000_000, 1), ("containment.share", 1, "resources = [\"E\"], divisor = 2".into())];
    let s = run(&world((1, 1), [0, 0], &[(1, [100, 7], bind("A")), (1, [0, 0], bind("A"))], &rules), 1, 1);
    let mut e: Vec<u64> = vec![s.ent_stock[0], s.ent_stock[2]];
    e.sort_unstable();
    assert_eq!(e, vec![50, 50]);
    assert_eq!(s.ent_stock[1] + s.ent_stock[3], 7, "unlisted resources stay where they were");
}

#[test]
fn released_guests_are_free() {
    let rules = [bind_rule(1_000_000, 4), ("containment.release", 1, "probability_ppm = 1000000".into())];
    assert_eq!(hosted(&run(&world((1, 1), [0, 0], &[(4, [0, 0], bind("A"))], &rules), 3, 1)), 0);
}

#[test]
fn a_dead_host_frees_its_guests_and_a_dead_guest_returns_its_stocks_to_the_host() {
    // One founder holds no E and starves in the tick it binds; it holds 6 A.
    let founders = [(1, [0, 6], bind("A")), (3, [9, 0], bind("A"))];
    let starve = ("death.starvation", 1, "resource = \"E\", recycle = true".to_string());
    let (mut host_died, mut guest_died) = (0, 0);
    for seed in 0..40u64 {
        let with = |rules: &[(&str, u32, String)]| run(&world((1, 1), [0, 0], &founders, rules).replace("seed = 3", &format!("seed = {seed}")), 1, 1);
        // The same binding draws, without the death: shows who was what.
        let before = with(&[bind_rule(1_000_000, 4)]);
        let after = with(&[bind_rule(1_000_000, 4), starve.clone()]);
        assert_eq!(after.entities.len(), 3);
        let a_held: u64 = after.ent_stock.iter().skip(1).step_by(2).sum();
        if before.entities[0].parent.is_some() {
            guest_died += 1;
            assert_eq!((after.site_stock[1], a_held, hosted(&after)), (0, 6, 2), "seed {seed}: to the host");
        } else {
            host_died += 1;
            assert_eq!((after.site_stock[1], a_held, hosted(&after)), (6, 0, 0), "seed {seed}: to the site, guests freed");
        }
    }
    assert!(host_died > 0 && guest_died > 0, "{host_died} {guest_died}");
}

#[test]
fn offspring_inherit_guests_by_lineage_or_at_random() {
    let divide = ("reproduction.division", 3, format!("requires = [{{ resource = \"E\", threshold = 50, consume = 0 }}], site_capacity = 100, {NO_MUTATION}"));
    let partition = |mode: &str, ppm: u32| ("containment.partition", 1, format!("probability_ppm = {ppm}, mode = \"{mode}\", max_guests = 8"));
    // One rich pair: whoever is host divides; guests of the mother go to the daughter.
    let founders = [(1, [60, 0], bind("A")), (1, [0, 0], bind("A"))];
    let mut moved = 0;
    for seed in 0..40u64 {
        let text = world((1, 1), [0, 0], &founders, &[bind_rule(1_000_000, 8), divide.clone(), partition("lineage", 1_000_000)]).replace("seed = 3", &format!("seed = {seed}"));
        let s = run(&text, 1, 1);
        if s.births == 1 {
            let child = s.entities.last().unwrap();
            let guests_of_child = s.entities.iter().filter(|g| g.parent == Some(child.id)).count();
            if child.parent.is_none() {
                assert_eq!(guests_of_child, 1, "a free daughter receives the guest");
                moved += 1;
            }
        }
        let kept = run(&text.replace("probability_ppm = 1000000, mode", "probability_ppm = 0, mode"), 1, 1);
        assert!(kept.entities.iter().filter(|e| e.born == 0 && e.id == 2).all(|child| kept.entities.iter().all(|g| g.parent != Some(child.id))));
    }
    assert!(moved > 5, "the rich founder is the host in about half of the seeds ({moved})");
    // Random re-assignment ends with the daughter sometimes and with the mother otherwise.
    let (mut with_daughter, mut elsewhere) = (0, 0);
    for seed in 0..60u64 {
        let text = world((1, 1), [0, 0], &founders, &[bind_rule(1_000_000, 8), divide.clone(), partition("random", 1_000_000)]).replace("seed = 3", &format!("seed = {seed}"));
        let s = run(&text, 1, 1);
        if s.births == 1 && s.entities[2].parent.is_none() {
            if s.entities.iter().any(|g| g.parent == Some(s.entities[2].id)) {
                with_daughter += 1;
            } else {
                elsewhere += 1;
            }
        }
    }
    assert!(with_daughter > 0 && elsewhere > 0, "{with_daughter} {elsewhere}");
    let err = parse(&world((1, 1), [0, 0], &founders, &[partition("sideways", 1)])).and_then(|(c, _)| lab_rules::build(&c).map(|_| ())).unwrap_err();
    assert!(err.to_string().contains("mode"), "{err}");
}

#[test]
fn containment_worlds_replay_resume_and_conserve() {
    let genes = [bind("A"), gene("uptake", "resource = \"A\"", 20), gene("convert", "from = \"A\", to = \"E\"", 20), gene("uptake", "resource = \"E\"", 5)].join(", ");
    let rules = [
        ("influx.constant", 1, "resource = \"A\", amount = 12, every = 1".into()),
        metabolism("host"),
        bind_rule(100_000, 3),
        ("containment.share", 1, "resources = [\"E\", \"A\"], divisor = 4".into()),
        ("death.starvation", 1, "resource = \"E\", recycle = true".into()),
        ("death.random", 1, "probability_ppm = 20000, recycle = true".into()),
        ("reproduction.division", 3, "requires = [{ resource = \"E\", threshold = 40, consume = 20 }], site_capacity = 4, mutation = { point_ppm = 30000, point_magnitude = 4, retarget_ppm = 20000, duplicate_ppm = 10000, delete_ppm = 10000, max_genes = 12, kinds = [\"uptake\", \"convert\", \"secrete\", \"bind\"] }".into()),
        ("containment.partition", 1, "probability_ppm = 500000, mode = \"random\", max_guests = 3".into()),
        ("containment.release", 1, "probability_ppm = 10000".into()),
        ("move.random", 1, "probability_ppm = 100000".into()),
    ];
    let text = world((40, 40), [10, 50], &[(1500, [30, 0], genes)], &rules);
    let (cfg, hash) = parse(&text).unwrap();
    let a = run(&text, 120, 1);
    assert!(hosted(&a) > 20 && a.births > 100 && a.deaths > 100, "{} hosted, {} births", hosted(&a), a.births);
    assert_eq!(state_hash(&a, &hash), state_hash(&run(&text, 120, 5), &hash));
    let half = run(&text, 60, 2);
    let (mut resumed, _) = decode(&encode(&half, &hash)).unwrap();
    let engine = Engine::new(lab_rules::build(&cfg).unwrap(), cfg.seed, 3).unwrap();
    for _ in 0..60 {
        engine.step(&mut resumed).unwrap();
    }
    assert_eq!(state_hash(&resumed, &hash), state_hash(&a, &hash));
}
