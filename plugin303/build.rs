fn main() {
    println!("cargo:rerun-if-env-changed=VIBE303_NAME");
    println!("cargo:rerun-if-env-changed=VIBE303_CLAP_ID");
    println!("cargo:rerun-if-env-changed=VIBE303_VST3_CLASS_ID");
}
