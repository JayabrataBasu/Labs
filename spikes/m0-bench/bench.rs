// THROWAWAY M0 spike: throughput + thread-count invariance + exact conservation. Not engine code.
use std::time::Instant;
const M0: u32 = 0xD2511F53; const M1: u32 = 0xCD9E8D57;
const W0: u32 = 0x9E3779B9; const W1: u32 = 0xBB67AE85;
#[inline(always)]
fn philox(mut c: [u32; 4], mut k: [u32; 2]) -> [u32; 4] {
    for r in 0..10 {
        if r > 0 { k[0] = k[0].wrapping_add(W0); k[1] = k[1].wrapping_add(W1); }
        let p0 = (M0 as u64) * (c[0] as u64);
        let p1 = (M1 as u64) * (c[2] as u64);
        c = [(p1 >> 32) as u32 ^ c[1] ^ k[0], p1 as u32, (p0 >> 32) as u32 ^ c[3] ^ k[1], p0 as u32];
    }
    c
}
#[derive(Clone, Copy)]
struct Gene { kind: u8, a: u8, b: u8, rate: u16 }
#[derive(Clone, Copy)]
struct Ent { id: u32, stock: [u32; 4], genes: [Gene; 8] }
const E: usize = 4; // entities per site
fn init(w: usize, seed: u32) -> (Vec<[u32; 4]>, Vec<Ent>) {
    let n = w * w;
    let sites = vec![[50u32, 0, 0, 0]; n];
    let mut ents = Vec::with_capacity(n * E);
    for i in 0..(n * E) as u32 {
        let mut genes = [Gene { kind: 0, a: 0, b: 0, rate: 0 }; 8];
        for g in 0..8u32 {
            let r = philox([i, g, 0, 0xFFFF_FFFF], [seed, 0]);
            genes[g as usize] = Gene { kind: (r[0] % 3) as u8, a: (r[1] % 4) as u8, b: (r[2] % 4) as u8, rate: (r[3] >> 16) as u16 };
        }
        ents.push(Ent { id: i, stock: [5, 0, 0, 0], genes });
    }
    (sites, ents)
}
// returns (influx, dissipated) for this chunk
fn step_chunk(sites: &mut [[u32; 4]], ents: &mut [Ent], tick: u32, seed: u32) -> (u64, u64) {
    let (mut influx, mut diss) = (0u64, 0u64);
    for (s, site) in sites.iter_mut().enumerate() {
        site[0] += 2; influx += 2;
        for e in &mut ents[s * E..(s + 1) * E] {
            for g in 0..8usize {
                let ge = e.genes[g];
                let r = philox([tick, g as u32, e.id, 0], [seed, 0]);
                if ((r[0] >> 16) as u16) < ge.rate {
                    let (a, b) = (ge.a as usize, ge.b as usize);
                    match ge.kind {
                        0 => { let t = site[a].min(1 + (r[1] & 1)); site[a] -= t; e.stock[a] += t; }
                        1 => if e.stock[a] >= 2 { e.stock[a] -= 2; e.stock[b] += 1; diss += 1; },
                        _ => if e.stock[a] > 0 { e.stock[a] -= 1; site[a] += 1; },
                    }
                }
            }
            if e.stock[0] > 0 { e.stock[0] -= 1; diss += 1; } // maintenance
        }
    }
    (influx, diss)
}
fn total(sites: &[[u32; 4]], ents: &[Ent]) -> u64 {
    sites.iter().flatten().map(|&x| x as u64).sum::<u64>() + ents.iter().flat_map(|e| e.stock).map(|x| x as u64).sum::<u64>()
}
fn hash(sites: &[[u32; 4]], ents: &[Ent]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut f = |x: u32| for b in x.to_le_bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); };
    for s in sites { for &x in s { f(x) } }
    for e in ents { for &x in &e.stock { f(x) } }
    h
}
fn run(w: usize, ticks: u32, threads: usize, seed: u32) -> (f64, u64, bool) {
    let (mut sites, mut ents) = init(w, seed);
    let t0 = total(&sites, &ents);
    let (mut influx, mut diss) = (0u64, 0u64);
    let per = sites.len().div_ceil(threads);
    let start = Instant::now();
    for tick in 0..ticks {
        let res: Vec<(u64, u64)> = std::thread::scope(|sc| {
            let hs: Vec<_> = sites.chunks_mut(per).zip(ents.chunks_mut(per * E))
                .map(|(s, e)| sc.spawn(move || step_chunk(s, e, tick, seed))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (i, d) in res { influx += i; diss += d; }
    }
    let dt = start.elapsed().as_secs_f64();
    let ok = total(&sites, &ents) == t0 + influx - diss;
    (dt, hash(&sites, &ents), ok)
}
fn main() {
    // Known-answer tests (Random123 kat_vectors, philox4x32-10) -- recalled from memory; a match is strong evidence.
    let k1 = philox([0; 4], [0; 2]);
    let k2 = philox([0xffffffff; 4], [0xffffffff; 2]);
    let k3 = philox([0x243f6a88, 0x85a308d3, 0x13198a2e, 0x03707344], [0xa4093822, 0x299f31d0]);
    println!("KAT zero: {:08x?} expect [6627e8d5, e169c58d, bc57ac4c, 9b00dbd8]", k1);
    println!("KAT ones: {:08x?} expect [408f276d, 41c83b0e, a20bc7c6, 6d5451fd]", k2);
    println!("KAT pi:   {:08x?} expect [d16cfe09, 94fdcceb, 5001e420, 24126ea1]", k3);
    let (w, ticks) = (512usize, 100u32);
    let n_ent = w * w * E;
    println!("world {}x{}, {} entities x 8 genes, {} ticks", w, w, n_ent, ticks);
    let mut h0 = None;
    for &th in &[1usize, 2, 4, 8, 12] {
        let (dt, h, ok) = run(w, ticks, th, 42);
        let rate = (n_ent as f64 * ticks as f64) / dt;
        let same = *h0.get_or_insert(h) == h;
        println!("threads {:2}: {:6.2}s  {:.3e} entity-ticks/s  hash {:016x} same_as_1thread={} conservation_exact={}", th, dt, rate, h, same, ok);
    }
    let (_, h2, _) = run(w, ticks, 4, 43);
    println!("different seed hash {:016x} (differs: {})", h2, Some(h2) != h0);
}
