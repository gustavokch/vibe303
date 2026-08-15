use plugin303::dsp::generator::{AcidGenerator, GeneratorParams, MusicalScale};
use plugin303::dsp::sequencer::TimeEntry;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut params = GeneratorParams::default();
    let mut output_json = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--key" if i + 1 < args.len() => {
                let k = &args[i + 1];
                params.root_note = match k.to_uppercase().as_str() {
                    "C" => 0,
                    "C#" | "DB" => 1,
                    "D" => 2,
                    "D#" | "EB" => 3,
                    "E" => 4,
                    "F" => 5,
                    "F#" | "GB" => 6,
                    "G" => 7,
                    "G#" | "AB" => 8,
                    "A" => 9,
                    "A#" | "BB" => 10,
                    "B" => 11,
                    _ => k.parse().unwrap_or(0),
                };
                i += 2;
            }
            "--scale" if i + 1 < args.len() => {
                params.scale = match args[i + 1].to_lowercase().as_str() {
                    "minor-pentatonic" | "pentatonic" => MusicalScale::MinorPentatonic,
                    "dorian" => MusicalScale::Dorian,
                    "phrygian" => MusicalScale::Phrygian,
                    "aeolian" | "minor" => MusicalScale::Aeolian,
                    "harmonic-minor" => MusicalScale::HarmonicMinor,
                    "chromatic" => MusicalScale::Chromatic,
                    _ => MusicalScale::MinorPentatonic,
                };
                i += 2;
            }
            "--pulses" if i + 1 < args.len() => {
                params.rhythm_pulses = args[i + 1].parse().unwrap_or(5);
                i += 2;
            }
            "--steps" if i + 1 < args.len() => {
                params.rhythm_length = args[i + 1].parse().unwrap_or(16);
                i += 2;
            }
            "--shift" if i + 1 < args.len() => {
                params.rhythm_shift = args[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            "--seed" if i + 1 < args.len() => {
                params.seed = args[i + 1].parse().unwrap_or(42);
                i += 2;
            }
            "--accent-prob" if i + 1 < args.len() => {
                params.accent_probability = args[i + 1].parse().unwrap_or(0.35);
                i += 2;
            }
            "--slide-prob" if i + 1 < args.len() => {
                params.slide_probability = args[i + 1].parse().unwrap_or(0.25);
                i += 2;
            }
            "--json" => {
                output_json = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let pattern = AcidGenerator::generate(&params);

    if output_json {
        if let Ok(json_str) = serde_json::to_string_pretty(&pattern) {
            println!("{}", json_str);
            return;
        }
    }

    const NOTE_NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];

    println!("=== 303 ACID GENERATIVE PATTERN ===");
    println!(
        "Key: {} | Scale: {:?} | Pulses: {}/{} | Seed: {}",
        NOTE_NAMES[(params.root_note % 12) as usize],
        params.scale,
        params.rhythm_pulses,
        params.rhythm_length,
        params.seed
    );
    println!("------------------------------------------------------------");

    // Line 1: Step Numbers
    print!("STEP  |");
    for s in 0..16 {
        print!(" {:02} ", s + 1);
        if (s + 1) % 4 == 0 && s < 15 {
            print!("|");
        }
    }
    println!();

    // Line 2: Gate / Timing
    print!("GATE  |");
    for s in 0..16 {
        let symbol = match pattern.timing[s] {
            TimeEntry::Note => " N  ",
            TimeEntry::Tie => " S  ",
            TimeEntry::Rest => " -  ",
        };
        print!("{}", symbol);
        if (s + 1) % 4 == 0 && s < 15 {
            print!("|");
        }
    }
    println!();

    // Line 3: Note Pitch
    print!("PITCH |");
    for s in 0..16 {
        if pattern.timing[s] == TimeEntry::Rest {
            print!(" ---");
        } else {
            let k = (pattern.pitches[s].key as usize) % 12;
            let oct = pattern.pitches[s].octave + 2; // Display as octave 1..3
            let name = format!("{}{}", NOTE_NAMES[k], oct);
            print!("{:>4}", name);
        }
        if (s + 1) % 4 == 0 && s < 15 {
            print!("|");
        }
    }
    println!();

    // Line 4: Accent
    print!("ACC   |");
    for s in 0..16 {
        let symbol = if pattern.pitches[s].accent && pattern.timing[s] != TimeEntry::Rest {
            " ACC"
        } else {
            " ---"
        };
        print!("{:>4}", symbol);
        if (s + 1) % 4 == 0 && s < 15 {
            print!("|");
        }
    }
    println!();

    // Line 5: Slide
    print!("SLD   |");
    for s in 0..16 {
        let symbol = if pattern.pitches[s].slide && pattern.timing[s] != TimeEntry::Rest {
            " SLD"
        } else {
            " ---"
        };
        print!("{:>4}", symbol);
        if (s + 1) % 4 == 0 && s < 15 {
            print!("|");
        }
    }
    println!();
    println!("------------------------------------------------------------");
}
