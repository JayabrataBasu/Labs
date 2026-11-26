//! M2 mechanisms: gene-driven metabolism, conversion accounting, birth, death, mutation,
//! null-world switches and scheduled shocks. Expected values are computed by hand.
use lab_core::config::{initial_state, parse};
use lab_core::{decode, encode, state_hash, Engine, Gene, GeneKind, State, SENSE_ALWAYS};
use lab_rng::RuleRng;
use lab_rules::mutation::MutationParams;

const NO_MUTATION: &str = "mutation = { point_ppm = 0, point_magnitude = 0, retarget_ppm = 0, duplicate_ppm = 0, delete_ppm = 0, max_genes = 16, kinds = [\"uptake\", \"convert\", \"secrete\"] }";

/// A 1x1 (or given) world with resources E, A, B and S; `genome` and `rules` are TOML fragments.
fn world(size: (u32, u32), site: [u64; 4], entities: u64, ent: [u64; 4], genome: &str, rules: &str) -> String {
    format!(
        "schema_version = 2\nseed = 5\n[world]\nwidth = {}\nheight = {}\nwrap = false\nresources = [\"E\", \"A\", \"B\", \"S\"]\n\
         [init]\nsite_stock = {site:?}\npatches = []\nfounders = [{{ count = {entities}, stock = {ent:?}, genome = [{genome}] }}]\n{rules}",
        size.0, size.1
    )
}
fn rule(id: &str, stream: u16, params: &str) -> String {
    let version = match id {
        "metabolism.genes" => 3,
        "reproduction.division" => 2,
        _ => 1,
    };
    format!("[[rules]]\nid = \"{id}\"\nversion = {version}\nstream = {stream}\nparams = {{ {params} }}\n")
}
fn gene(kind: &str, fields: &str, level: u16, sense: &str, threshold: u16) -> String {
    format!("{{ kind = \"{kind}\", {fields}, level = {level}, sense = \"{sense}\", threshold = {threshold} }}")
}
fn reaction(from: &str, to: &str, leak_ppm: u32) -> String {
    format!("{{ from = \"{from}\", to = \"{to}\", leak_ppm = {leak_ppm} }}")
}
fn chemistry(base: u64, per_gene: u64, yield_den: u32, leak_ppm: u32, release_to: &str) -> String {
    let reactions = [reaction("A", "B", leak_ppm), reaction("B", "E", 0)].join(", ");
    rule("metabolism.genes", 1, &format!("energy = \"E\", base_cost = {base}, gene_cost = {per_gene}, yield_num = 1, yield_den = {yield_den}, reactions = [{reactions}], release_to = \"{release_to}\", guests_trade_with = \"site\""))
}
fn metabolism(base: u64, per_gene: u64, transfer: bool) -> String {
    chemistry(base, per_gene, 2, 0, if transfer { "site" } else { "sink" })
}
fn run(text: &str, ticks: u32, threads: usize) -> State {
    let (cfg, _) = parse(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    let engine = Engine::new(lab_rules::build(&cfg).unwrap(), cfg.seed, threads).unwrap();
    let mut s = initial_state(&cfg).unwrap();
    for _ in 0..ticks {
        engine.step(&mut s).unwrap();
    }
    s
}

#[test]
fn conversion_is_accounted_exactly() {
    let genome = gene("convert", "from = \"A\", to = \"B\"", 4, "always", 0);
    let s = run(&world((1, 1), [0; 4], 1, [0, 10, 0, 0], &genome, &metabolism(0, 0, true)), 1, 1);
    assert_eq!(s.ent_stock, vec![0, 6, 2, 0], "4 of A consumed, 2 of B produced at yield 1/2");
    assert_eq!(s.ledger.converted_out, vec![0, 4, 0, 0]);
    assert_eq!(s.ledger.converted_in, vec![0, 0, 2, 0]);
    let s = run(&world((1, 1), [0; 4], 1, [0, 3, 0, 0], &genome, &metabolism(0, 0, true)), 1, 1);
    assert_eq!(s.ent_stock, vec![0, 0, 1, 0], "only 3 available: floor(3 / 2) = 1 produced");
}

#[test]
fn chemistry_is_a_world_rule_not_a_mutable_trait() {
    // A -> E is not in the reaction list: the gene is carried and paid for, but does nothing.
    let genome = gene("convert", "from = \"A\", to = \"E\"", 4, "always", 0);
    let s = run(&world((1, 1), [0; 4], 1, [5, 10, 0, 0], &genome, &metabolism(0, 1, true)), 1, 1);
    assert_eq!(s.ent_stock, vec![4, 10, 0, 0]);
    assert_eq!(s.ledger.converted_out, vec![0; 4]);
}

#[test]
fn genes_act_in_order_within_one_tick_and_cost_energy() {
    // Take 6 A from the site, convert 6 A -> 3 B, release 2 B; pay 1 + 3 genes x 2 = 7 E.
    let genome = [
        gene("uptake", "resource = \"A\"", 6, "always", 0),
        gene("convert", "from = \"A\", to = \"B\"", 6, "always", 0),
        gene("secrete", "resource = \"B\"", 2, "always", 0),
    ]
    .join(", ");
    let s = run(&world((1, 1), [0, 20, 0, 0], 1, [10, 0, 0, 0], &genome, &metabolism(1, 2, true)), 1, 1);
    assert_eq!(s.site_stock, vec![0, 14, 2, 0]);
    assert_eq!(s.ent_stock, vec![3, 0, 1, 0]);
    assert_eq!(s.ledger.dissipated, vec![7, 0, 0, 0]);
}

#[test]
fn releasing_to_the_sink_removes_transfer_but_keeps_the_producers_budget() {
    let genome = gene("secrete", "resource = \"B\"", 2, "always", 0);
    let intact = run(&world((1, 1), [0; 4], 1, [9, 0, 5, 0], &genome, &metabolism(0, 1, true)), 1, 1);
    let null = run(&world((1, 1), [0; 4], 1, [9, 0, 5, 0], &genome, &metabolism(0, 1, false)), 1, 1);
    assert_eq!(intact.ent_stock, null.ent_stock, "the secretor pays and loses exactly the same");
    assert_eq!((intact.site_stock[2], null.site_stock[2]), (2, 0), "only the intact world lets others have it");
    assert_eq!(null.ledger.dissipated, vec![1, 0, 2, 0]);
}

#[test]
fn leaky_reactions_release_an_exact_share_of_the_product() {
    let genome = gene("convert", "from = \"A\", to = \"B\"", 100, "always", 0);
    let start = |release: &str| run(&world((1, 1), [0; 4], 1, [0, 100, 0, 0], &genome, &chemistry(0, 0, 1, 250_000, release)), 1, 1);
    let intact = start("site");
    assert_eq!((intact.ent_stock[2], intact.site_stock[2]), (75, 25), "a quarter of the 100 produced leaks out");
    assert_eq!(intact.ledger.converted_in[2], 100, "the ledger records everything produced");
    let null = start("sink");
    assert_eq!((null.ent_stock[2], null.site_stock[2], null.ledger.dissipated[2]), (75, 0, 25));
}

#[test]
fn fractional_leaks_are_rounded_by_chance_without_bias() {
    // Each of 400 entities produces 50; a quarter of 50 is 12.5, so each leaks 12 or 13.
    let genome = gene("convert", "from = \"A\", to = \"B\"", 50, "always", 0);
    let s = run(&world((1, 1), [0; 4], 400, [0, 50, 0, 0], &genome, &chemistry(0, 0, 1, 250_000, "site")), 1, 1);
    let leaked: Vec<u64> = (0..400).map(|i| 50 - s.ent_stock[i * 4 + 2]).collect();
    assert!(leaked.iter().all(|&x| x == 12 || x == 13), "{leaked:?}");
    let total: u64 = leaked.iter().sum();
    assert_eq!(total, s.site_stock[2]);
    assert!((4900..=5100).contains(&total), "mean leak should be near 12.5, total {total}");
    let t = run(&world((1, 1), [0; 4], 400, [0, 50, 0, 0], &genome, &chemistry(0, 0, 1, 250_000, "site")), 1, 4);
    assert_eq!(s, t, "rounding draws do not depend on threads");
}

#[test]
fn reaction_and_release_settings_are_validated() {
    let bad = |rules: String| parse(&world((1, 1), [0; 4], 0, [0; 4], "", &rules)).and_then(|(c, _)| lab_rules::build(&c).map(|_| ())).unwrap_err().to_string();
    assert!(bad(chemistry(0, 0, 1, 1_000_001, "site")).contains("leak_ppm"));
    assert!(bad(chemistry(0, 0, 1, 0, "nowhere")).contains("release_to"));
}

#[test]
fn sensing_gates_expression() {
    let genome = gene("uptake", "resource = \"A\"", 3, "S", 5);
    let quiet = run(&world((1, 1), [0, 9, 0, 4], 1, [0; 4], &genome, &metabolism(0, 0, true)), 1, 1);
    assert_eq!(quiet.ent_stock, vec![0; 4], "signal 4 < threshold 5: gene silent");
    let active = run(&world((1, 1), [0, 9, 0, 5], 1, [0; 4], &genome, &metabolism(0, 0, true)), 1, 1);
    assert_eq!(active.ent_stock, vec![0, 3, 0, 0]);
}

#[test]
fn division_splits_stocks_and_records_lineage() {
    let division = rule("reproduction.division", 2, &format!("requires = [{{ resource = \"E\", threshold = 10 }}], site_capacity = 3, {NO_MUTATION}"));
    let genome = gene("uptake", "resource = \"A\"", 1, "always", 0);
    let text = world((1, 1), [0; 4], 1, [11, 5, 0, 0], &genome, &division);
    let s = run(&text, 1, 1);
    assert_eq!(s.entities.len(), 2);
    assert_eq!(s.ent_stock, vec![6, 3, 0, 0, 5, 2, 0, 0], "offspring gets floor(half); parent keeps the rest");
    let child = &s.entities[1];
    assert_eq!((child.id, child.site, child.mother, child.born, child.parent), (1, 0, Some(0), 0, None));
    assert_eq!(s.genomes[1], s.genomes[0], "with all mutation rates at zero the genome is copied exactly");
    assert_eq!((s.births, s.deaths, s.next_entity_id), (1, 0, 2));
    // Tick 2: neither holds 10 E any more. Capacity: start again with plenty and watch it cap at 3.
    let rich = run(&world((1, 1), [0; 4], 1, [1000, 0, 0, 0], &genome, &division), 5, 1);
    assert_eq!(rich.entities.len(), 3, "site_capacity = 3 stops further division");
    assert_eq!(rich.total(0).unwrap(), 1000, "division conserves every resource");
}

#[test]
fn starvation_removes_and_returns_stocks() {
    let rules = rule("death.starvation", 1, "resource = \"E\", recycle = true");
    let s = run(&world((1, 1), [0; 4], 2, [0, 7, 0, 0], "", &rules), 1, 1);
    assert!(s.entities.is_empty());
    assert_eq!(s.site_stock, vec![0, 14, 0, 0]);
    assert_eq!(s.deaths, 2);
    let alive = run(&world((1, 1), [0; 4], 2, [1, 7, 0, 0], "", &rules), 3, 1);
    assert_eq!(alive.entities.len(), 2, "holding any energy keeps an entity alive");
}

#[test]
fn death_without_recycling_dissipates_stocks_and_still_conserves() {
    let silent = rule("death.starvation", 1, "resource = \"E\", recycle = false");
    let s = run(&world((1, 1), [0; 4], 2, [0, 7, 3, 0], "", &silent), 1, 1);
    assert!(s.entities.is_empty());
    assert_eq!(s.site_stock, vec![0; 4], "nothing returns to the site");
    assert_eq!(s.ledger.dissipated, vec![0, 14, 6, 0], "what the dead held leaves the system as dissipation");
    let culled = run(&world((1, 1), [0; 4], 4, [1, 1, 0, 0], "", &rule("shock.cull", 1, "tick = 0, probability_ppm = 1000000, recycle = false")), 1, 1);
    assert_eq!((culled.entities.len(), culled.site_stock.iter().sum::<u64>(), culled.ledger.dissipated[0]), (0, 0, 4));
}

#[test]
fn random_birth_and_death_ignore_stocks() {
    let rules = rule("reproduction.random", 1, &format!("probability_ppm = 1000000, site_capacity = 8, {NO_MUTATION}")) + &rule("death.random", 2, "probability_ppm = 0, recycle = true");
    let s = run(&world((1, 1), [0; 4], 1, [0; 4], "", &rules), 3, 1);
    assert_eq!(s.entities.len(), 8, "1 -> 2 -> 4 -> 8 with no resources at all");
    let rules = rule("death.random", 2, "probability_ppm = 1000000, recycle = true");
    assert!(run(&world((1, 1), [0; 4], 5, [9; 4], "", &rules), 1, 1).entities.is_empty());
}

#[test]
fn division_needs_every_required_resource() {
    let both = rule("reproduction.division", 2, &format!("requires = [{{ resource = \"E\", threshold = 10 }}, {{ resource = \"B\", threshold = 4 }}], site_capacity = 9, {NO_MUTATION}"));
    let births = |stock: [u64; 4]| run(&world((1, 1), [0; 4], 1, stock, "", &both), 1, 1).births;
    assert_eq!(births([100, 0, 3, 0]), 0, "rich in energy but short of B");
    assert_eq!(births([9, 0, 100, 0]), 0, "rich in B but short of energy");
    assert_eq!(births([10, 0, 4, 0]), 1);
    let none = rule("reproduction.division", 2, &format!("requires = [], site_capacity = 9, {NO_MUTATION}"));
    let e = parse(&world((1, 1), [0; 4], 0, [0; 4], "", &none)).and_then(|(c, _)| lab_rules::build(&c).map(|_| ())).unwrap_err();
    assert!(e.to_string().contains("requires"), "{e}");
}

#[test]
fn several_founder_types_are_placed_with_their_own_stock_and_genome() {
    let first = gene("uptake", "resource = \"A\"", 3, "always", 0);
    let second = gene("secrete", "resource = \"B\"", 1, "always", 0);
    let text = world((4, 4), [0; 4], 2, [5, 0, 0, 0], &first, "rules = []\n")
        .replace("}] }]\n", &format!("}}] }}, {{ count = 3, stock = [0, 7, 0, 0], genome = [{second}] }}]\n"))
        .replace("\nrules = []\n", "\n")
        .replace("[world]", "rules = []\n[world]");
    let s = run(&text, 0, 1);
    assert_eq!(s.entities.iter().map(|e| e.id).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
    assert_eq!(s.genomes.iter().map(|g| g[0].kind).collect::<Vec<_>>(), [vec![GeneKind::Uptake; 2], vec![GeneKind::Secrete; 3]].concat());
    assert_eq!(s.ent_stock, [[5, 0, 0, 0].repeat(2), [0, 7, 0, 0].repeat(3)].concat());
    assert_eq!((s.next_entity_id, s.total(0).unwrap(), s.total(1).unwrap()), (5, 10, 21));
    let wrong = text.replace("stock = [0, 7, 0, 0]", "stock = [0, 7]");
    assert!(parse(&wrong).unwrap_err().to_string().contains("founders[1]"));
}

fn founder() -> Vec<Gene> {
    vec![
        Gene { kind: GeneKind::Uptake, a: 1, b: 1, sense: SENSE_ALWAYS, level: 100, threshold: 0 },
        Gene { kind: GeneKind::Convert, a: 1, b: 2, sense: 3, level: 100, threshold: 50 },
        Gene { kind: GeneKind::Secrete, a: 2, b: 2, sense: SENSE_ALWAYS, level: 100, threshold: 0 },
    ]
}
fn mutation(point: u32, magnitude: u16, retarget: u32, duplicate: u32, delete: u32, max_genes: u32) -> MutationParams {
    MutationParams { point_ppm: point, point_magnitude: magnitude, retarget_ppm: retarget, duplicate_ppm: duplicate, delete_ppm: delete, max_genes, kinds: ["uptake", "convert", "secrete"].map(String::from).to_vec() }
}
const RNG: RuleRng = RuleRng { seed: 77, stream: 4 };

#[test]
fn mutation_operators_are_independent() {
    let parent = founder();
    assert_eq!(mutation(0, 0, 0, 0, 0, 8).offspring(&parent, RNG, 3, 9, 4), parent, "all rates zero: exact copy (null world N5)");
    assert!(mutation(0, 0, 0, 0, 1_000_000, 8).offspring(&parent, RNG, 3, 9, 4).is_empty(), "certain deletion");
    let doubled = mutation(0, 0, 0, 1_000_000, 0, 8).offspring(&parent, RNG, 3, 9, 4);
    assert_eq!(doubled, vec![parent[0], parent[0], parent[1], parent[1], parent[2], parent[2]]);
    assert_eq!(mutation(0, 0, 0, 1_000_000, 0, 4).offspring(&parent, RNG, 3, 9, 4).len(), 4, "max_genes caps duplication");
    assert_eq!(mutation(0, 0, 0, 0, 0, 8).offspring(&parent, RNG, 3, 9, 4), mutation(0, 0, 0, 0, 0, 8).offspring(&parent, RNG, 3, 9, 4));
}

#[test]
fn point_mutation_changes_only_magnitudes_within_bounds() {
    let parent = founder();
    let mut changed = 0;
    for id in 0..500u64 {
        let child = mutation(1_000_000, 7, 0, 0, 0, 8).offspring(&parent, RNG, 1, id, 4);
        assert_eq!(child.len(), 3);
        for (c, p) in child.iter().zip(&parent) {
            assert_eq!((c.kind, c.a, c.b, c.sense), (p.kind, p.a, p.b, p.sense), "point mutation never changes identity");
            let step = (i32::from(c.level) - i32::from(p.level)).abs() + (i32::from(c.threshold) - i32::from(p.threshold)).abs();
            assert!(step <= 7, "step {step} exceeds magnitude");
            changed += usize::from(step > 0);
        }
    }
    assert!(changed > 1000, "most point mutations should change something ({changed} of 1500)");
    assert_ne!(
        mutation(1_000_000, 7, 0, 0, 0, 8).offspring(&parent, RNG, 1, 1, 4),
        mutation(1_000_000, 7, 0, 0, 0, 8).offspring(&parent, RNG, 2, 1, 4),
        "outcomes depend on the tick"
    );
}

#[test]
fn retargeting_keeps_genes_well_formed() {
    let mut kinds = std::collections::BTreeSet::new();
    for k in [1u8, 2, 4] {
        for id in 0..2000u64 {
            let seed_gene = [Gene { kind: GeneKind::Uptake, a: 0, b: 0, sense: SENSE_ALWAYS, level: 10, threshold: 0 }];
            for g in mutation(0, 0, 1_000_000, 0, 0, 8).offspring(&seed_gene, RNG, 1, id, k) {
                assert!(g.a < k && g.b < k && (g.sense == SENSE_ALWAYS || g.sense < k), "resource out of range for k = {k}: {g:?}");
                assert_eq!(g.kind == GeneKind::Convert, g.a != g.b, "only conversions join two resources: {g:?}");
                assert!(g.sense != SENSE_ALWAYS || g.threshold == 0);
                if k == 4 {
                    kinds.insert(g.kind);
                }
            }
        }
    }
    assert_eq!(kinds.len(), 3, "retargeting can reach every gene kind, so new functions can arise");
}

#[test]
fn decay_relocate_shock_and_cull() {
    let decay = run(&world((1, 1), [0, 0, 0, 40], 0, [0; 4], "", &rule("decay.fraction", 1, "resource = \"S\", divisor = 4")), 2, 1);
    assert_eq!(decay.site_stock[3], 23, "40 -> 30 -> 23 (integer division)");

    let mixed = run(&world((9, 9), [0; 4], 200, [0; 4], "", &rule("mix.relocate", 1, "probability_ppm = 1000000")), 1, 1);
    let start = run(&world((9, 9), [0; 4], 200, [0; 4], "", &rule("mix.relocate", 1, "probability_ppm = 1000000")), 0, 1);
    let far = mixed.entities.iter().zip(&start.entities).filter(|(a, b)| !start.topo.neighbours(b.site).contains(&a.site) && a.site != b.site).count();
    assert!(far > 100, "relocation is not limited to neighbouring sites ({far} of 200 jumped further)");

    let shocks = rule("shock.scheduled", 1, "tick = 2, action = \"add\", resource = \"A\", amount = 9, x = 1, y = 0, width = 2, height = 1")
        + &rule("shock.scheduled", 2, "tick = 3, action = \"remove\", resource = \"B\", amount = 4, x = 0, y = 0, width = 1, height = 1");
    let text = world((3, 1), [0, 0, 6, 0], 0, [0; 4], "", &shocks);
    assert_eq!(run(&text, 2, 1).site_stock[1], 0, "nothing before tick 2 has been applied");
    let after = run(&text, 4, 1);
    assert_eq!([after.site_stock[1], after.site_stock[5], after.site_stock[9]], [0, 9, 9]);
    assert_eq!([after.site_stock[2], after.site_stock[6]], [2, 6]);

    let cull = rule("shock.cull", 1, "tick = 1, probability_ppm = 1000000, recycle = true");
    assert_eq!(run(&world((2, 2), [0; 4], 30, [1; 4], "", &cull), 1, 1).entities.len(), 30);
    let culled = run(&world((2, 2), [0; 4], 30, [1; 4], "", &cull), 2, 1);
    assert!(culled.entities.is_empty());
    assert_eq!(culled.total(1).unwrap(), 30, "the culled entities' stocks stay in the world");
}

#[test]
fn patches_shape_the_initial_world() {
    let text = world((4, 3), [1, 0, 0, 0], 0, [0; 4], "", "rules = []\n")
        .replace("patches = []", "patches = [{ x = 1, y = 1, width = 2, height = 2, resource = \"A\", amount = 7 }]")
        .replace("\nrules = []\n", "\n")
        .replace("[world]", "rules = []\n[world]");
    let s = run(&text, 0, 1);
    let a: Vec<u64> = (0..12).map(|site| s.site_stock[site * 4 + 1]).collect();
    assert_eq!(a, vec![0, 0, 0, 0, 0, 7, 7, 0, 0, 7, 7, 0]);
    let outside = text.replace("width = 2, height = 2", "width = 4, height = 2");
    assert!(parse(&outside).unwrap_err().to_string().contains("patches"));
}

#[test]
fn crossfeed_world_lives_and_is_reproducible() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../configs/crossfeed.toml")).unwrap();
    let (cfg, hash) = parse(&text).unwrap();
    let a = run(&text, 400, 1);
    let b = run(&text, 400, 4);
    assert_eq!(state_hash(&a, &hash), state_hash(&b, &hash), "births, deaths and mutation are thread-independent");
    assert!(!a.entities.is_empty(), "the population should persist for 400 ticks");
    assert!(a.births > cfg.init.founders.iter().map(|f| f.count).sum::<u64>() && a.deaths > 0, "there is turnover: {} births, {} deaths", a.births, a.deaths);
    type Signature = Vec<(u8, u8, u8, u8, u16, u16)>;
    let distinct: std::collections::BTreeSet<Signature> =
        a.genomes.iter().map(|g| g.iter().map(|x| (x.kind as u8, x.a, x.b, x.sense, x.level, x.threshold)).collect()).collect();
    assert!(distinct.len() > 1, "mutation produced heritable variation");
    // Checkpoints carry genomes and lineage.
    let (restored, stored) = decode(&encode(&a, &hash)).unwrap();
    assert_eq!((restored, stored), (a, hash));
}

/// The construct-check worlds on a small lattice (same density), so they run quickly.
fn small_control(name: &str) -> String {
    let text = std::fs::read_to_string(format!("{}/../../configs/{name}.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    text.replace("width = 48\nheight = 48", "width = 16\nheight = 16").replace("count = 1500", "count = 167").replace("count = 800", "count = 89")
}

#[test]
fn positive_control_depends_on_transfer_and_negative_control_does_not() {
    let positive = small_control("positive-control");
    let intact = run(&positive, 600, 2);
    let kinds: std::collections::BTreeSet<usize> = intact.genomes.iter().map(Vec::len).collect();
    assert!(intact.entities.len() > 300 && intact.births > 500, "specialists persist together: {} alive, {} births", intact.entities.len(), intact.births);
    assert_eq!(kinds.len(), 1);
    let types: std::collections::BTreeSet<u8> = intact.genomes.iter().map(|g| g[1].b).collect();
    assert_eq!(types.len(), 2, "both specialist types survive");
    let cut = run(&positive.replace("release_to = \"site\"", "release_to = \"sink\"").replace("recycle = true", "recycle = false"), 600, 2);
    assert!(cut.births < intact.births / 10, "without transfer the specialists cannot reproduce: {} vs {}", cut.births, intact.births);

    let negative = small_control("negative-control");
    let alone = run(&negative, 600, 2);
    assert!(alone.entities.len() > 300, "self-sufficient entities need no one: {}", alone.entities.len());
    assert_eq!(alone.site_stock.iter().skip(3).step_by(5).sum::<u64>() + alone.site_stock.iter().skip(4).step_by(5).sum::<u64>(), 0, "no intermediate ever reaches a site");
}

#[test]
fn black_queen_world_lives_and_loses_genes() {
    let text = small_control("blackqueen");
    let s = run(&text, 800, 2);
    assert!(s.entities.len() > 200, "{}", s.entities.len());
    let first_steps = |g: &Vec<Gene>| [3u8, 4].iter().filter(|&&product| g.iter().any(|x| x.kind == GeneKind::Convert && x.a == 2 && x.b == product && x.level > 0)).count();
    assert!(s.genomes.iter().any(|g| first_steps(g) < 2), "some lineage has lost a leaky step");
}
