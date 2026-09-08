fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("slc {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    eprintln!("slc: no input file");
    std::process::exit(1);
}
