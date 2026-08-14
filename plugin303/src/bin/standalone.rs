use nih_plug::prelude::*;
use plugin303::Plugin303;

fn main() {
    eprintln!("[STANDALONE_DEBUG] Starting standalone binary...");
    let res = nih_export_standalone::<Plugin303>();
    eprintln!("[STANDALONE_DEBUG] Finished standalone binary with result: {res}");
}
