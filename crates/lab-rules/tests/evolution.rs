//! M2 mechanisms: gene-driven metabolism, conversion accounting, birth, death, mutation,
//! null-world switches and scheduled shocks. Expected values are computed by hand.
use lab_core::config::{initial_state, parse};
use lab_core::{decode, encode, state_hash, Engine, Gene, GeneKind, State, SENSE_ALWAYS};
use lab_rng::RuleRng;
use lab_rules::mutation::MutationParams;

const NO_MUTATION: &str = "mutation = { point_ppm = 0, point_magnitude = 0, retarget_ppm = 0, duplicate_ppm = 0, delete_ppm = 0, max_genes = 16 }";

/// A 1x1 (or given) world with resources E, A, B and S; `genome` and `rules` are TOML fragments.
fn world(size: (u32, u32), site: [u64; 4], entities: u64, ent: [u64; 4], genome: &str, rules: &str) -> String {
    format!(
        "schema_version = 1\nseed = 5\n[world]\nwidth = {}\nheight = {}\nwrap = false\nresources = [\"E\", \"A\", \"B\", \"S\"]\n\
         [init]\nsite_stock = {site:?}\npatches = []\nentities = {entities}\nentity_stock = {ent:?}\ngenome = [{genome}]\n{rules}",
        size.0, size.1
    )
}
fn rule(id: &str, stream: u16, params: &str) -> String {
    format!("[[rules]]\nid = \"{id}\"\nversion = 1\nstream = {stream}\nparams = {{ {params} }}\n")
}
fn gene(kind: &str, fields: &str, level: u16, sense: &str, threshold: u16) -> String {
    format!("{{ kind = \"{kind}\", {fields}, level = {level}, sense = \"{sense}\", threshold = {threshold} }}")
}
fn metabolism(base: u64, per_gene: u64, secretion: bool) -> String {
    rule("metabolism.genes", 1, &format!("energy = \"E\", base_cost = {base}, gene_cost = {per_gene}, yield_num = 1, yield_den = 2, reactions = [[\"A\", \"B\"], [\"B\", \"E\"]], secretion = {secretion}"))
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
fn secretion_switch_is_the_no_interaction_null() {
    let genome = gene("secrete", "resource = \"B\"", 2, "always", 0);
    let s = run(&world((1, 1), [0; 4], 1, [9, 0, 5, 0], &genome, &metabolism(0, 1, false)), 1, 1);
    assert_eq!(s.site_stock, vec![0; 4], "nothing reaches the site");
    assert_eq!(s.ent_stock, vec![8, 0, 5, 0], "but the silent gene is still paid for");
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
    let division = rule("reproduction.division", 2, &format!("resource = \"E\", threshold = 10, site_capacity = 3, {NO_MUTATION}"));
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

fn founder() -> Vec<Gene> {
    vec![
        Gene { kind: GeneKind::Uptake, a: 1, b: 1, sense: SENSE_ALWAYS, level: 100, threshold: 0 },
        Gene { kind: GeneKind::Convert, a: 1, b: 2, sense: 3, level: 100, threshold: 50 },
        Gene { kind: GeneKind::Secrete, a: 2, b: 2, sense: SENSE_ALWAYS, level: 100, threshold: 0 },
    ]
}
fn mutation(point: u32, magnitude: u16, retarget: u32, duplicate: u32, delete: u32, max_genes: u32) -> MutationParams {
    MutationParams { point_ppm: point, point_magnitude: magnitude, retarget_ppm: retarget, duplicate_ppm: duplicate, delete_ppm: delete, max_genes }
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
    assert!(a.births > cfg.init.entities && a.deaths > 0, "there is turnover: {} births, {} deaths", a.births, a.deaths);
    type Signature = Vec<(u8, u8, u8, u8, u16, u16)>;
    let distinct: std::collections::BTreeSet<Signature> =
        a.genomes.iter().map(|g| g.iter().map(|x| (x.kind as u8, x.a, x.b, x.sense, x.level, x.threshold)).collect()).collect();
    assert!(distinct.len() > 1, "mutation produced heritable variation");
    // Checkpoints carry genomes and lineage.
    let (restored, stored) = decode(&encode(&a, &hash)).unwrap();
    assert_eq!((restored, stored), (a, hash));
}
