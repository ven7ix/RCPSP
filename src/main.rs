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

    // let base_schedule: Schedule = generate_schedule(&config);
    // _ = Schedule::find_best_schedule_serial(&base_schedule);
    // _ = Schedule::find_best_schedule_parallel(&base_schedule);

    run_leaderboard(&config, 10);
}

fn save_schedule_to_file(filename: &str, schedule: &Schedule, strategy: SortStrategy) {
    match schedule.save_to_file(filename, &format!("{:?}", strategy)) {
        Ok(()) => println!("Saved to {}", filename),
        Err(e) => eprintln!("Failed to save file: {}", e),
    }
}

// ─── Таблица лидеров ─────────────────────────────────────────────────────────

struct StrategyStats {
    strategy: SortStrategy,
    total_time: u64,
    rank_sum: usize,
    run_count: usize,
}

impl StrategyStats {
    fn new(strategy: SortStrategy) -> Self {
        Self { strategy, total_time: 0, rank_sum: 0, run_count: 0 }
    }

    fn avg_time(&self) -> f64 {
        if self.run_count == 0 {
            return 0.0;
        }
        self.total_time as f64 / self.run_count as f64
    }
}

fn run_leaderboard(config: &GenerationConfig, seed_count: u64) {
    let mut serial_stats: Vec<StrategyStats> = SortStrategy::all_except_random()
        .iter()
        .map(|&s| StrategyStats::new(s))
        .collect();
    let mut parallel_stats: Vec<StrategyStats> = SortStrategy::all_except_random()
        .iter()
        .map(|&s| StrategyStats::new(s))
        .collect();

    for seed in 0..seed_count {
        let mut seeded_config = config.clone();
        seeded_config.random_seed = seed;
        let base_schedule = generate_schedule(&seeded_config);

        print!("Сид {:>4}/{} ", seed + 1, seed_count);

        run_seed_serial(&base_schedule, &mut serial_stats);
        run_seed_parallel(&base_schedule, &mut parallel_stats);

        println!("✓");
    }

    if let Err(e) = print_leaderboard("SERIAL", &mut serial_stats) {
        eprintln!("Failed to save leaderboard: {}", e);
    }
    if let Err(e) = print_leaderboard("PARALLEL", &mut parallel_stats) {
        eprintln!("Failed to save leaderboard: {}", e);
    }
}

fn run_seed_serial(base_schedule: &Schedule, stats: &mut Vec<StrategyStats>) {
    let mut results: Vec<(usize, u32)> = SortStrategy::all_except_random()
        .iter()
        .enumerate()
        .filter_map(|(i, &strategy)| {
            let mut schedule = base_schedule.clone();
            schedule.compute_serial(strategy).ok()?;
            Some((i, schedule.total_execute_time()))
        })
        .collect();

    accumulate(&mut results, stats);
}

fn run_seed_parallel(base_schedule: &Schedule, stats: &mut Vec<StrategyStats>) {
    let mut results: Vec<(usize, u32)> = SortStrategy::all_except_random()
        .iter()
        .enumerate()
        .filter_map(|(i, &strategy)| {
            let mut schedule = base_schedule.clone();
            schedule.compute_parallel(strategy).ok()?;
            Some((i, schedule.total_execute_time()))
        })
        .collect();

    accumulate(&mut results, stats);
}

fn accumulate(results: &mut Vec<(usize, u32)>, stats: &mut Vec<StrategyStats>) {
    results.sort_by_key(|&(_, time)| time);

    for (rank, &(strategy_idx, time)) in results.iter().enumerate() {
        stats[strategy_idx].total_time += time as u64;
        stats[strategy_idx].rank_sum += rank + 1; // места с 1
        stats[strategy_idx].run_count += 1;
    }
}

fn print_leaderboard(label: &str, stats: &mut Vec<StrategyStats>) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    stats.sort_by(|a, b| {
        a.rank_sum
            .cmp(&b.rank_sum)
            .then_with(|| a.avg_time().partial_cmp(&b.avg_time()).unwrap())
    });

    let filename = format!("leaderboard_{}.txt", label.to_lowercase());
    let mut file = File::create(&filename)?;

    let name_width = 30usize;
    let col = 14usize;

    writeln!(file, "┌─ {} LEADERBOARD {}", label, "─".repeat(50))?;
    writeln!(file, "│ {:>3}  {:<name_width$}  {:>col$}  {:>col$}", "МЕС", "СТРАТЕГИЯ", "СР. ВРЕМЯ", "СУММА МЕСТ", name_width = name_width, col = col)?;
    writeln!(file, "│ {}", "─".repeat(name_width + col * 2 + 12))?;

    for (place, s) in stats.iter().enumerate() {
        let medal = match place {
            0 => "1",
            1 => "2",
            2 => "3",
            _ => "-",
        };
        writeln!(file, "│ {:>1}  {:<name_width$}  {:>col$.1}  {:>col$}", medal, format!("{:?}", s.strategy), s.avg_time(), s.rank_sum, name_width = name_width, col = col,)?;
    }
    writeln!(file, "└{}", "─".repeat(name_width + col * 2 + 14))?;

    println!("Leaderboard saved to {}", filename);
    return Ok(());
}
