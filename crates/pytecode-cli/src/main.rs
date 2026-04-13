fn main() {
    if let Err(err) = pytecode_cli::run_cli() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
