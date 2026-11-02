fn main() {
    let command = std::env::args().nth(1).unwrap_or_default();
    if let Err(error) = open_meteo_checks::desktop::run(&command) {
        eprintln!("Desktop check failed ({command}): {error:#}");
        std::process::exit(1);
    }
}
