use plugin303::dsp::voice::Voice;
use plugin303::dsp::filter::Tier;
use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    println!("Instantiating cold Voice::new(48000.0)...");
    let mut voice1 = Voice::new(48000.0);
    println!("Voice 1 instantiated in {:.3} ms", t0.elapsed().as_secs_f64() * 1000.0);

    let t1 = Instant::now();
    println!("Instantiating warm Voice::new(48000.0)...");
    let mut _voice2 = Voice::new(48000.0);
    println!("Voice 2 instantiated in {:.3} ms", t1.elapsed().as_secs_f64() * 1000.0);

    voice1.set_tier_immediate(Tier::Ultra);
    voice1.note_on(57);
    let t2 = Instant::now();
    let mut acc = 0.0f32;
    for _ in 0..4096 {
        acc += voice1.tick();
    }
    println!("Rendered 4096 Ultra samples in {:.3} ms (acc={:.3})", t2.elapsed().as_secs_f64() * 1000.0, acc);
}
