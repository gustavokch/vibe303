//! Headless 303 voice renderer for the ESR harness. Emits raw `f32le` (no wav dep).
//! Args: --note --sr --secs --cutoff --reso --tier --out
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;
use std::io::Write;

fn arg(flag: &str, default: f32) -> f32 {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|s| s == flag)
        .and_then(|i| a.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn arg_s(flag: &str, default: &str) -> String {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|s| s == flag)
        .and_then(|i| a.get(i + 1))
        .cloned()
        .unwrap_or_else(|| default.into())
}

fn main() {
    let sr = arg("--sr", 48_000.0);
    let note = arg("--note", 57.0) as u8; // A2
    let secs = arg("--secs", 0.5);
    let cutoff = arg("--cutoff", 1_000.0);
    let reso = arg("--reso", 0.0);
    let tier = arg("--tier", 0.0) as i32; // 0=Low 1=High 2=Ultra; default keeps F2 golds valid
    let out = arg_s("--out", "render.f32");

    let mut v = Voice::new(sr);
    v.set_tier(match tier {
        1 => Tier::High,
        2 => Tier::Ultra,
        _ => Tier::Low,
    });
    v.set_cutoff(cutoff);
    v.set_reso(reso / 100.0); // --reso is a percentage; the voice wants 0..1
    v.note_on(note);
    let n = (secs * sr) as usize;
    let mut buf = Vec::with_capacity(n);
    for _ in 0..n {
        buf.push(v.tick());
    }

    let mut f = std::fs::File::create(&out).unwrap();
    for s in &buf {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
}
