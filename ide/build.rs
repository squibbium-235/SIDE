fn main() {
    println!("cargo:rerun-if-changed=syntax");
    println!("cargo:rerun-if-changed=assets/fonts/JetBrainsMono-Regular.ttf");
}
