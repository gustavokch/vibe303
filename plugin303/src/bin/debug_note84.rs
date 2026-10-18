use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

fn main() {
    let sr = 48_000.0;
    let note = 84u8;
    let mut v = Voice::new(sr);
    v.set_tier_immediate(Tier::Best);
    v.set_cutoff(4000.0);
    v.set_reso(0.0);
    v.note_on(note);
    v.set_gate(true, true, false, false);

    println!("First 300 samples:");
    for i in 0..300 {
        let y = v.tick();
        if i < 300 {
            println!("  sample {}: {:.6}", i, y);
        }
    }
}
