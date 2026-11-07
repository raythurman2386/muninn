//! GRP-01: measured layout budgets on a synthetic 2000-note vault.
//!
//! Run the release budget with:
//! `cargo test -p muninn-graph --test perf --release -- --nocapture`
//! The settle and real-vault probes are `--ignored` so the normal test pass
//! stays fast; point `MUNINN_BENCH_VAULT` at a read-only copy for the latter.

use std::time::{Duration, Instant};

use muninn_core::testing::TempVault;
use muninn_graph::Graph;

/// 25-note star groups plus a chain: realistic hubs without a single
/// pathological 80-link node.
fn big_vault(n: usize) -> TempVault {
    let mut files = Vec::with_capacity(n);
    for i in 0..n {
        let mut body = String::new();
        if i > 0 {
            body.push_str(&format!("[[Note {:04}]]\n", i - 1));
        }
        let hub = i - i % 25;
        if hub != i {
            body.push_str(&format!("[[Note {hub:04}]]\n"));
        }
        files.push((format!("Note {i:04}.md"), format!("# Note {i:04}\n{body}")));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect();
    TempVault::new(&refs)
}

#[test]
fn step_p95_fits_the_2000_note_budget() {
    let tmp = big_vault(2000);
    let v = tmp.load();
    let mut g = Graph::new(&v);
    for _ in 0..30 {
        g.step();
    }
    let mut samples = Vec::with_capacity(120);
    for _ in 0..120 {
        let start = Instant::now();
        g.step();
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let p95 = samples[samples.len() * 95 / 100];
    println!(
        "2000-note step: p50 {:.2} ms, p95 {:.2} ms, max {:.2} ms",
        samples[samples.len() / 2].as_secs_f64() * 1000.0,
        p95.as_secs_f64() * 1000.0,
        samples[samples.len() - 1].as_secs_f64() * 1000.0,
    );
    // Honest gate for an unoptimised test build. Release is 4.8 ms p95
    // (reported in the PR); the exhaustive O(n²) pass this replaced misses
    // this by seconds.
    assert!(
        p95 < Duration::from_millis(50),
        "2000-note step p95 {:.2} ms exceeds the 50 ms hard gate",
        p95.as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    for _ in 0..120 {
        g.step();
    }
    println!(
        "  release reference: {:.2} ms/step over 120 further frames",
        start.elapsed().as_secs_f64() * 1000.0 / 120.0
    );
}

#[test]
#[ignore = "manual measurement: 2000-note settle is slow in a debug test build"]
fn settle_time_at_2000_notes_is_reported() {
    let tmp = big_vault(2000);
    let v = tmp.load();
    let mut g = Graph::new(&v);
    let start = Instant::now();
    let mut frames = 0;
    let mut quiet_by = None;
    while frames < 4000 {
        let before: Vec<[f32; 2]> = g.nodes.iter().map(|node| node.pos).collect();
        g.step();
        frames += 1;
        let max = g
            .nodes
            .iter()
            .zip(&before)
            .fold(0.0f32, |max, (node, old)| {
                max.max((node.pos[0] - old[0]).abs())
                    .max((node.pos[1] - old[1]).abs())
            });
        if max < 1.0 && quiet_by.is_none() {
            quiet_by = Some((frames, start.elapsed()));
        }
        if frames % 600 == 0 {
            let over = |limit: f32| {
                g.nodes
                    .iter()
                    .zip(&before)
                    .filter(|(node, old)| {
                        (node.pos[0] - old[0]).abs() > limit || (node.pos[1] - old[1]).abs() > limit
                    })
                    .count()
            };
            println!(
                "  2000-note frame {frames}: max motion {max:.3} pt; >1pt {}; >10pt {}",
                over(1.0),
                over(10.0)
            );
        }
        if max < 0.04 {
            break;
        }
    }
    println!(
        "2000-note settle: {frames} frames in {:.0} ms; <1pt at {}",
        start.elapsed().as_secs_f64() * 1000.0,
        quiet_by.map_or_else(
            || "never".to_owned(),
            |(f, t)| format!("{f} frames / {:.0} ms", t.as_secs_f64() * 1000.0)
        )
    );
}

/// Manual measurement against a real vault copy. The vault is only read;
/// point `MUNINN_BENCH_VAULT` at a copy (never edit the original).
#[test]
#[ignore = "set MUNINN_BENCH_VAULT to a read-only vault copy"]
fn real_vault_step_and_settle() {
    let path = std::env::var("MUNINN_BENCH_VAULT").expect("set MUNINN_BENCH_VAULT to a vault path");
    let v = muninn_core::Vault::load(std::path::Path::new(&path)).expect("vault");
    let mut g = Graph::new(&v);
    for _ in 0..30 {
        g.step();
    }
    let mut samples = Vec::with_capacity(120);
    for _ in 0..120 {
        let start = Instant::now();
        g.step();
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let p95 = samples[samples.len() * 95 / 100];
    println!(
        "{} notes: step p50 {:.2} ms, p95 {:.2} ms",
        v.notes.len(),
        samples[samples.len() / 2].as_secs_f64() * 1000.0,
        p95.as_secs_f64() * 1000.0,
    );
    let mut fresh = Graph::new(&v);
    let start = Instant::now();
    let mut frames = 0;
    let mut max = f32::INFINITY;
    while frames < 4000 && max >= 0.04 {
        let before: Vec<[f32; 2]> = fresh.nodes.iter().map(|node| node.pos).collect();
        fresh.step();
        frames += 1;
        max = fresh
            .nodes
            .iter()
            .zip(&before)
            .fold(0.0f32, |max, (node, old)| {
                max.max((node.pos[0] - old[0]).abs())
                    .max((node.pos[1] - old[1]).abs())
            });
    }
    println!(
        "{} notes: settle {frames} frames in {:.0} ms (max motion now {max:.3} pt)",
        v.notes.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );
}
