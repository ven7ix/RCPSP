use rcpsp::schedule::*;
use rcpsp::schedule_generator::*;
use std::env;
use std::path::PathBuf;

fn main() {
    let config: GenerationConfig = if let Some(path) = env::args().nth(1) {
        let path: PathBuf = PathBuf::from(path);
        match load_config_from_json(&path) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Ошибка загрузки конфигурации из {}: {}", path.display(), e);
                std::process::exit(1);
            }
        }
    } else {
        GenerationConfig::default()
    };

    let base_schedule: Schedule = generate_schedule(&config);

    compute_schedule_serial(&base_schedule);

    compute_schedule_parallel(&base_schedule);
}

fn compute_schedule_serial(base_schedule: &Schedule) {
    let (schedule_parallel, strategy, execute_time) = Schedule::find_best_schedule_serial(&base_schedule);
    println!("Best strategy: {:?}, execute time: {}", strategy, execute_time);

    let filename: &'static str = "best_schedule_serial.txt";
    match schedule_parallel.save_to_file(filename, &format!("{:?}", strategy)) {
        Ok(()) => println!("Saved to {}", filename),
        Err(e) => eprintln!("Failed to save file: {}", e),
    }
}

fn compute_schedule_parallel(base_schedule: &Schedule) {
    let (schedule_parallel, strategy, execute_time) = Schedule::find_best_schedule_parallel(&base_schedule);
    println!("Best strategy: {:?}, execute time: {}", strategy, execute_time);

    let filename: &'static str = "best_schedule_parallel.txt";
    match schedule_parallel.save_to_file(filename, &format!("{:?}", strategy)) {
        Ok(()) => println!("Saved to {}", filename),
        Err(e) => eprintln!("Failed to save file: {}", e),
    }
}
