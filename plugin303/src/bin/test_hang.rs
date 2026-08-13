use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::Voice;

fn main() {
    println!("Creating voice...");
    let mut voice = Voice::new(48_000.0);
    println!("Setting tier immediately to Ultra...");
    voice.set_tier_immediate(Tier::Ultra);
    println!("Setting cutoff and resonance...");
    voice.set_cutoff(1_000.0);
    voice.set_reso(0.5);
    println!("Triggering note_on...");
    voice.note_on(57);
    
    println!("Warming up (128 ticks)...");
    for i in 0..128 {
        print!("warm tick {}... ", i);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        voice.tick();
        println!("ok");
    }
    
    println!("Rendering hot path (4096 ticks)...");
    for i in 0..4096 {
        print!("hot tick {}... ", i);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        voice.tick();
        println!("ok");
    }
    println!("All ticks completed successfully!");
}
