use lab_core::config::{initial_state, parse};
use lab_core::{Engine, EntityMeta, LabError, State, Topology};

fn header(w: u32, h: u32, wrap: bool, site: u64, entities: u64, ent: u64) -> String {
    format!(
        "schema_version = 1\nseed = 11\n[world]\nwidth = {w}\nheight = {h}\nwrap = {wrap}\nresources = [\"A\"]\n\
         [init]\nsite_stock = [{site}]\nentities = {entities}\nentity_stock = [{ent}]\n"
    )
}
fn rule(id: &str, stream: u16, params: &str) -> String {
    format!("[[rules]]\nid = \"{id}\"\nversion = 1\nstream = {stream}\nparams = {{ {params} }}\n")
}
fn engine(text: &str) -> Engine {
    let (cfg, _) = parse(text).unwrap();
    Engine::new(lab_rules::build(&cfg).unwrap(), cfg.seed, 2).unwrap()
}
fn run(text: &str, ticks: u32) -> State {
    let (cfg, _) = parse(text).unwrap();
    let mut s = initial_state(&cfg).unwrap();
    let e = engine(text);
    for _ in 0..ticks {
        e.step(&mut s).unwrap();
    }
    s
}
fn err(text: &str) -> String {
    match parse(text).and_then(|(cfg, _)| lab_rules::build(&cfg).map(|_| ())) {
        Err(LabError::Config(m)) => m,
        other => panic!("expected a configuration error, got {other:?}"),
    }
}

#[test]
fn influx_adds_on_schedule() {
    let text = header(3, 3, false, 0, 0, 0) + &rule("influx.constant", 1, "resource = \"A\", amount = 2, every = 2");
    let s = run(&text, 4);
    assert_eq!(s.site_stock, vec![4; 9], "ticks 0 and 2 each add 2");
    assert_eq!(s.ledger.influx, vec![36]);
}

#[test]
fn diffusion_shares_by_integer_division() {
    let text = header(3, 3, false, 0, 0, 0) + &rule("diffusion.integer", 1, "resource = \"A\", divisor = 8");
    let mut site_stock = vec![0u64; 9];
    site_stock[4] = 80;
    let mut s = State::new(Topology::lattice(3, 3, false).unwrap(), 1, site_stock, vec![], vec![]).unwrap();
    engine(&text).step(&mut s).unwrap();
    assert_eq!(s.site_stock, vec![0, 10, 0, 10, 40, 10, 0, 10, 0]);
    let mut small = State::new(Topology::lattice(3, 3, false).unwrap(), 1, vec![7; 9], vec![], vec![]).unwrap();
    engine(&text).step(&mut small).unwrap();
    assert_eq!(small.site_stock, vec![7; 9], "7 / 8 == 0, so nothing moves");
}

#[test]
fn uptake_takes_requested_amount() {
    let text = header(1, 1, false, 10, 2, 0) + &rule("uptake.fixed", 1, "resource = \"A\", amount = 3");
    let s = run(&text, 1);
    assert_eq!(s.site_stock, vec![4]);
    assert_eq!(s.ent_stock, vec![3, 3]);
}

#[test]
fn uptake_priority_is_not_id_order() {
    let text = header(1, 1, false, 0, 2, 0)
        + &rule("influx.constant", 1, "resource = \"A\", amount = 3, every = 1")
        + &rule("uptake.fixed", 2, "resource = \"A\", amount = 3");
    let (cfg, _) = parse(&text).unwrap();
    let mut s = initial_state(&cfg).unwrap();
    let e = engine(&text);
    let mut wins = [0u32; 2];
    for _ in 0..200 {
        let before = s.ent_stock.clone();
        e.step(&mut s).unwrap();
        let gained: Vec<u64> = s.ent_stock.iter().zip(&before).map(|(a, b)| a - b).collect();
        assert!(gained == [3, 0] || gained == [0, 3], "exactly one entity wins all 3 units, got {gained:?}");
        wins[usize::from(gained[1] == 3)] += 1;
    }
    assert!((60..=140).contains(&wins[0]), "entity 0 won {} of 200 contests", wins[0]);
}

#[test]
fn maintenance_dissipates_and_clips() {
    let text = header(1, 1, false, 0, 1, 5) + &rule("maintenance.fixed", 1, "resource = \"A\", amount = 2");
    let s = run(&text, 3);
    assert_eq!(s.ent_stock, vec![0]);
    assert_eq!(s.ledger.dissipated, vec![5]);
}

#[test]
fn move_probability_extremes() {
    let never = header(3, 3, true, 0, 50, 0) + &rule("move.random", 1, "probability_ppm = 0");
    let (cfg, _) = parse(&never).unwrap();
    let start: Vec<u32> = initial_state(&cfg).unwrap().entities.iter().map(|e| e.site).collect();
    let end: Vec<u32> = run(&never, 20).entities.iter().map(|e| e.site).collect();
    assert_eq!(start, end);

    let always = header(3, 3, true, 0, 50, 0) + &rule("move.random", 1, "probability_ppm = 1000000");
    let (cfg, _) = parse(&always).unwrap();
    let mut s = initial_state(&cfg).unwrap();
    let e = engine(&always);
    for _ in 0..20 {
        let before: Vec<u32> = s.entities.iter().map(|e| e.site).collect();
        e.step(&mut s).unwrap();
        assert!(s.entities.iter().zip(&before).all(|(e, &b)| e.site != b), "every entity must move every tick");
    }
}

#[test]
fn move_on_isolated_site_is_a_no_op() {
    let text = header(1, 1, false, 0, 3, 0) + &rule("move.random", 1, "probability_ppm = 1000000");
    assert!(run(&text, 5).entities.iter().all(|e| e.site == 0));
}

#[test]
fn initial_placement_is_seeded_and_complete() {
    let text = header(8, 8, true, 1, 100, 2).replace("[world]", "rules = []\n[world]");
    let s = run(&text, 0);
    assert_eq!(s.entities.len(), 100);
    assert_eq!(s.entities, run(&text, 0).entities);
    assert!(s.entities.iter().enumerate().all(|(i, e): (usize, &EntityMeta)| e.id == i as u64 && e.parent.is_none()));
    let other = run(&text.replace("seed = 11", "seed = 12"), 0);
    assert_ne!(s.entities, other.entities);
    assert_eq!(s.initial_total, vec![64 + 200]);
}

#[test]
fn parse_rejects_unknown_field() {
    let text = header(3, 3, false, 0, 0, 0).replace("seed = 11", "seed = 11\nsed = 1");
    assert!(err(&text).contains("sed"), "{}", err(&text));
    let text = header(3, 3, false, 0, 0, 0) + &rule("influx.constant", 1, "resource = \"A\", amount = 2, every = 1, evry = 3");
    assert!(err(&text).contains("evry"), "{}", err(&text));
}

#[test]
fn parse_rejects_missing_field() {
    let text = header(3, 3, false, 0, 0, 0).replace("seed = 11\n", "");
    assert!(err(&text).contains("seed"), "{}", err(&text));
    let text = header(3, 3, false, 0, 0, 0) + &rule("influx.constant", 1, "resource = \"A\", amount = 2");
    assert!(err(&text).contains("every"), "{}", err(&text));
    let no_rules = header(3, 3, false, 0, 0, 0).replace("[world]", "rules = []\n[world]");
    assert!(parse(&no_rules).is_ok(), "an explicit empty rule list is allowed");
    assert!(err(&header(3, 3, false, 0, 0, 0)).contains("rules"), "but omitting `rules` is not");
}

#[test]
fn parse_rejects_bad_values() {
    let base = header(3, 3, false, 0, 0, 0);
    let influx = |stream| rule("influx.constant", stream, "resource = \"A\", amount = 1, every = 1");
    let cases: Vec<(&str, String)> = vec![
        ("schema_version", base.replace("schema_version = 1", "schema_version = 2") + &influx(1)),
        ("resource", base.replace("[\"A\"]", "[\"A\", \"A\"]").replace("[0]", "[0, 0]") + &influx(1)),
        ("site_stock", base.replace("site_stock = [0]", "site_stock = [0, 0]") + &influx(1)),
        ("entity_stock", base.replace("entity_stock = [0]", "entity_stock = []") + &influx(1)),
        ("stream", base.clone() + &influx(1) + &influx(1)),
        ("stream", base.clone() + &influx(0)),
        ("no.such.rule", base.clone() + &rule("no.such.rule", 1, "")),
        ("version", (base.clone() + &influx(1)).replace("version = 1\nstream", "version = 2\nstream")),
        ("divisor", base.clone() + &rule("diffusion.integer", 1, "resource = \"A\", divisor = 4")),
        ("probability_ppm", base.clone() + &rule("move.random", 1, "probability_ppm = 1000001")),
        ("every", base.clone() + &rule("influx.constant", 1, "resource = \"A\", amount = 1, every = 0")),
        ("Z", base.clone() + &rule("uptake.fixed", 1, "resource = \"Z\", amount = 1")),
        ("width", header(2, 3, true, 0, 0, 0) + &influx(1)),
    ];
    for (needle, text) in cases {
        let message = err(&text);
        assert!(message.contains(needle), "expected {needle:?} in error, got: {message}");
    }
}

#[test]
fn config_hash_is_of_exact_text() {
    let text = header(3, 3, false, 0, 0, 0).replace("[world]", "rules = []\n[world]");
    let (_, h1) = parse(&text).unwrap();
    let (_, h2) = parse(&(text.clone() + "# comment\n")).unwrap();
    assert_eq!(h1, parse(&text).unwrap().1);
    assert_ne!(h1, h2, "any byte change in the config changes its hash");
}

#[test]
fn reference_config_parses_and_runs_10_ticks() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../configs/reference.toml")).unwrap();
    let s = run(&text, 10);
    assert_eq!(s.tick, 10);
    assert_eq!(s.entities.len(), 2000);
    assert_eq!(s.ledger.influx, vec![10 * 2 * 4096, 0]);
}

#[test]
fn no_floats_in_kernel() {
    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                scan(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let floaty = text.split(|c: char| !c.is_alphanumeric() && c != '_').any(|tok| tok == "f32" || tok == "f64");
                if floaty {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    let mut hits = Vec::new();
    for krate in ["lab-rng", "lab-core", "lab-rules"] {
        scan(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(krate).join("src"), &mut hits);
    }
    assert!(hits.is_empty(), "floating-point types found in kernel sources (DR-002): {hits:?}");
}
