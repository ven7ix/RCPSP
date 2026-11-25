use rcpsp::leaderboard::run_leaderboard;
use rcpsp::schedule_generator::{GenerationConfig, load_config_from_json};
use std::env;
use std::path::PathBuf;

fn main() {
    let config: GenerationConfig = match env::args().nth(1) {
        Some(path) => {
            let path = PathBuf::from(path);
            load_config_from_json(&path).unwrap_or_else(|e| {
                eprintln!("Error while loading config {}: {}", path.display(), e);
                std::process::exit(1);
            })
        }
        None => GenerationConfig::default(),
    };

    run_leaderboard(&config);
}
