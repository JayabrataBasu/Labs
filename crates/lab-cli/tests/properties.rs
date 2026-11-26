//! Property tests for the core invariants: thread-count invariance, exact conservation,
//! RNG stream isolation, and checkpoint/resume equivalence, over randomly generated worlds.
use lab_core::config::{initial_state, parse, Config};
use lab_core::{check_conservation, decode, encode, state_hash, Delta, Engine, Observation, Rule, SiteIndex, State};
use lab_rng::RuleRng;
use proptest::prelude::*;
use std::ops::Range;

#[derive(Clone, Debug)]
struct World {
    text: String,
    ticks: u32,
    /// Position at which the isolation test inserts its extra rule.
    insert_at: usize,
    midpoint: u32,
}

fn rule_text(kind: usize, stream: usize, resource: usize, a: u64, b: u64, k: usize) -> String {
    let r = format!("R{resource}");
    let (id, params) = match kind {
        0 => ("influx.constant", format!("resource = \"{r}\", amount = {}, every = {}", a % 6, 1 + b % 3)),
        1 => ("diffusion.integer", format!("resource = \"{r}\", divisor = {}", 5 + a % 8)),
        2 => ("uptake.fixed", format!("resource = \"{r}\", amount = {}", a % 6)),
        3 => ("maintenance.fixed", format!("resource = \"{r}\", amount = {}", a % 4)),
        4 => ("move.random", format!("probability_ppm = {}", a % 1_000_001)),
        5 => (
            "metabolism.genes",
            format!(
                "energy = \"{r}\", base_cost = {}, gene_cost = {}, yield_num = {}, yield_den = 4, reactions = [{}], release_to = \"{}\"",
                a % 3,
                b % 2,
                a % 5,
                if k > 1 { format!("{{ from = \"R0\", to = \"R1\", leak_ppm = {} }}", b % 1_000_001) } else { String::new() },
                if b.is_multiple_of(5) { "sink" } else { "site" }
            ),
        ),
        6 => (
            "reproduction.division",
            format!(
                "requires = [{{ resource = \"{r}\", threshold = {} }}], site_capacity = {}, mutation = {{ point_ppm = {}, point_magnitude = 3, retarget_ppm = {}, duplicate_ppm = {}, delete_ppm = {}, max_genes = 12 }}",
                1 + a % 12, 1 + b % 6, a % 400_000, b % 300_000, a % 200_000, b % 200_000
            ),
        ),
        7 => ("death.starvation", format!("resource = \"{r}\", recycle = {}", !a.is_multiple_of(3))),
        8 => ("mix.relocate", format!("probability_ppm = {}", a % 1_000_001)),
        _ => ("decay.fraction", format!("resource = \"{r}\", divisor = {}", 1 + a % 9)),
    };
    let version = if id == "metabolism.genes" || id == "reproduction.division" { 2 } else { 1 };
    format!("[[rules]]\nid = \"{id}\"\nversion = {version}\nstream = {stream}\nparams = {{ {params} }}\n")
}

fn world() -> impl Strategy<Value = World> {
    let kinds = proptest::sample::subsequence(vec![0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9], 1..=10).prop_shuffle();
    (
        (3u32..=40, 3u32..=40, any::<bool>(), 1usize..=3, 0u64..=300),
        (kinds, proptest::collection::vec((0usize..3, any::<u64>(), any::<u64>()), 10)),
        (0u64..=i64::MAX as u64, 1u32..=40, any::<usize>(), any::<u32>()),
        (proptest::collection::vec(0u64..=20, 3), proptest::collection::vec(0u64..=5, 3)),
    )
        .prop_map(|((w, h, wrap, k, entities), (kinds, params), (seed, ticks, insert, mid), (site, ent))| {
            let resources: Vec<String> = (0..k).map(|i| format!("\"R{i}\"")).collect();
            let list = |v: &[u64]| v[..k].iter().map(u64::to_string).collect::<Vec<_>>().join(", ");
            // Founders can take up, convert (when there are two resources) and secrete.
            let mut genome = String::from("{ kind = \"uptake\", resource = \"R0\", level = 3, sense = \"always\", threshold = 0 }");
            if k > 1 {
                genome += ", { kind = \"convert\", from = \"R0\", to = \"R1\", level = 2, sense = \"always\", threshold = 0 }";
                genome += ", { kind = \"secrete\", resource = \"R1\", level = 1, sense = \"R0\", threshold = 2 }";
            }
            let mut text = format!(
                "schema_version = 2\nseed = {seed}\n[world]\nwidth = {w}\nheight = {h}\nwrap = {wrap}\nresources = [{}]\n\
                 [init]\nsite_stock = [{}]\npatches = []\nfounders = [{{ count = {entities}, stock = [{}], genome = [{genome}] }}, {{ count = {}, stock = [{}], genome = [] }}]\n",
                resources.join(", "),
                list(&site),
                list(&ent),
                entities / 3,
                list(&site)
            );
            for (i, &kind) in kinds.iter().enumerate() {
                let (resource, a, b) = params[i];
                text += &rule_text(kind, i + 1, resource % k, a, b, k);
            }
            World { text, ticks, insert_at: insert % (kinds.len() + 1), midpoint: mid % (ticks + 1) }
        })
}

/// Draws random numbers for every site and entity but changes nothing.
struct DrawOnly;
impl Rule for DrawOnly {
    fn id(&self) -> &'static str {
        "test.draw_only"
    }
    fn version(&self) -> u32 {
        1
    }
    fn stream(&self) -> u16 {
        999
    }
    fn propose(&self, state: &State, index: &SiteIndex, rng: RuleRng, sites: Range<u32>, _: &mut Vec<Delta>) {
        for site in sites {
            std::hint::black_box(rng.draw(state.tick as u32, u64::from(site), 0));
            for &e in index.at(site) {
                std::hint::black_box(rng.draw(state.tick as u32, state.entities[e as usize].id, 1));
            }
        }
    }
}

fn config(w: &World) -> (Config, [u8; 32]) {
    parse(&w.text).unwrap_or_else(|e| panic!("generated config is invalid: {e}\n{}", w.text))
}
fn engine(cfg: &Config, threads: usize) -> Engine {
    Engine::new(lab_rules::build(cfg).unwrap(), cfg.seed, threads).unwrap()
}
fn run(w: &World, threads: usize) -> [u8; 32] {
    let (cfg, hash) = config(w);
    let e = engine(&cfg, threads);
    let mut s = initial_state(&cfg).unwrap();
    for _ in 0..w.ticks {
        e.step(&mut s).unwrap();
    }
    state_hash(&s, &hash)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn prop_thread_invariance(w in world()) {
        let reference = run(&w, 1);
        prop_assert_eq!(run(&w, 3), reference);
        prop_assert_eq!(run(&w, 8), reference);
    }

    #[test]
    fn prop_conservation(w in world()) {
        let (cfg, _) = config(&w);
        let e = engine(&cfg, 2);
        let mut s = initial_state(&cfg).unwrap();
        for _ in 0..w.ticks {
            e.step(&mut s).unwrap();
            prop_assert!(check_conservation(&s).is_ok());
            for r in 0..s.k {
                prop_assert_eq!(
                    s.total(r).unwrap() + s.ledger.dissipated[r] + s.ledger.converted_out[r],
                    s.initial_total[r] + s.ledger.influx[r] + s.ledger.converted_in[r]
                );
            }
        }
    }

    #[test]
    fn prop_stream_isolation(w in world()) {
        let (cfg, hash) = config(&w);
        let mut rules = lab_rules::build(&cfg).unwrap();
        rules.insert(w.insert_at, Box::new(DrawOnly));
        let e = Engine::new(rules, cfg.seed, 2).unwrap();
        let mut s = initial_state(&cfg).unwrap();
        for _ in 0..w.ticks {
            e.step(&mut s).unwrap();
        }
        prop_assert_eq!(state_hash(&s, &hash), run(&w, 2));
    }

    #[test]
    fn prop_observation_changes_nothing_and_reconciles(w in world()) {
        let (cfg, hash) = config(&w);
        let observe = |threads: usize| {
            let e = engine(&cfg, threads);
            let mut s = initial_state(&cfg).unwrap();
            let mut seen = Observation::default();
            for _ in 0..w.ticks {
                e.step_observed(&mut s, &mut seen).unwrap();
            }
            (s, seen.flows().to_vec(), seen.births.len() as u64, seen.deaths.len() as u64)
        };
        let (s, flows, births, deaths) = observe(1);
        prop_assert_eq!(state_hash(&s, &hash), run(&w, 2));
        prop_assert_eq!((births, deaths), (s.births, s.deaths));
        for r in 0..s.k {
            let produced: u64 = flows.iter().filter(|f| f.resource as usize == r).map(|f| f.produced).sum();
            prop_assert_eq!(produced, s.ledger.converted_in[r]);
        }
        prop_assert_eq!(observe(5).1, flows);
    }

    #[test]
    fn prop_resume_equivalence(w in world()) {
        let (cfg, hash) = config(&w);
        let e = engine(&cfg, 2);
        let mut s = initial_state(&cfg).unwrap();
        for _ in 0..w.midpoint {
            e.step(&mut s).unwrap();
        }
        let (mut resumed, stored) = decode(&encode(&s, &hash)).unwrap();
        prop_assert_eq!(stored, hash);
        prop_assert_eq!(&resumed, &s);
        let e2 = engine(&cfg, 3);
        for _ in w.midpoint..w.ticks {
            e2.step(&mut resumed).unwrap();
        }
        prop_assert_eq!(state_hash(&resumed, &hash), run(&w, 2));
    }

    #[test]
    fn prop_entities_sorted_and_parent_none(w in world()) {
        let (cfg, _) = config(&w);
        let e = engine(&cfg, 2);
        let mut s = initial_state(&cfg).unwrap();
        for _ in 0..w.ticks {
            e.step(&mut s).unwrap();
            prop_assert!(s.entities.windows(2).all(|p| p[0].id < p[1].id));
            prop_assert!(s.entities.iter().all(|x| x.parent.is_none() && x.site < s.topo.n_sites()));
            prop_assert_eq!(s.genomes.len(), s.entities.len());
            prop_assert_eq!(s.entities.len() as u64, cfg.init.founders.iter().map(|f| f.count).sum::<u64>() + s.births - s.deaths);
        }
    }
}
