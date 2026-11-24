use rcpsp::schedule::*;
use rcpsp::schedule_generator::*;
use rcpsp::time::Time;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
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

// ─── Leaderboard ─────────────────────────────────────────────────────────

struct StrategyStats {
    strategy: SortStrategy,
    total_time: u64,
    /// Сумма мест по всем запускам (меньше — лучше).
    rank_sum: usize,
    run_count: usize,
}

impl StrategyStats {
    fn new(strategy: SortStrategy) -> Self {
        Self { strategy, total_time: 0, rank_sum: 0, run_count: 0 }
    }

    fn avg_time(&self) -> f64 {
        if self.run_count == 0 { 0.0 } else { self.total_time as f64 / self.run_count as f64 }
    }
}

fn run_leaderboard(config: &GenerationConfig) {
    // Статистика по каждому алгоритму; порядок совпадает с `Algorithm::ALL`.
    let mut stats: Vec<Vec<StrategyStats>> = Algorithm::ALL
        .iter()
        .map(|_| {
            SortStrategy::all()
                .iter()
                .map(|&strategy| StrategyStats::new(strategy))
                .collect()
        })
        .collect();

    let seeds = config.random_seed..config.random_seed + config.seed_count;
    for (index, seed) in seeds.enumerate() {
        let mut seeded_config = config.clone();
        seeded_config.random_seed = seed;
        let mut base_schedule = generate_schedule(&seeded_config);
        base_schedule.random_seed = seed; // воспроизводимая стратегия Random

        println!("Seed {:>4}/{} ", index + 1, config.seed_count);

        for (&algorithm, algorithm_stats) in Algorithm::ALL.iter().zip(&mut stats) {
            match run_seed(&base_schedule, algorithm) {
                Ok(times) => accumulate(&times, algorithm_stats),
                // Сид пропускается целиком, чтобы сбой не искажал места остальных стратегий.
                Err(e) => eprintln!("Seed {seed}: {algorithm:?} failed: {e}"),
            }
        }
    }

    for (algorithm, algorithm_stats) in Algorithm::ALL.iter().zip(&mut stats) {
        let label = format!("{algorithm:?}");
        let filename = format!("leaderboard_{}.txt", label.to_lowercase());
        if let Err(e) = save_leaderboard_to_file(&filename, &label, algorithm_stats) {
            eprintln!("Failed to save leaderboard {filename}: {e}");
        }
    }
}

/// Длительность расписания для каждой стратегии (в порядке `SortStrategy::all()`).
fn run_seed(base_schedule: &Schedule, algorithm: Algorithm) -> Result<Vec<Time>, String> {
    SortStrategy::all()
        .iter()
        .map(|&strategy| {
            let mut schedule = base_schedule.clone();
            schedule.compute(algorithm, strategy)?;
            Ok(schedule.total_execute_time())
        })
        .collect()
}

/// Места раздаются по длительности расписания; при равенстве стратегии делят одно место
/// (иначе выигрывала бы та, что стоит раньше в списке).
fn accumulate(times: &[Time], stats: &mut [StrategyStats]) {
    for (stat, &time) in stats.iter_mut().zip(times) {
        stat.rank_sum += 1 + times
            .iter()
            .filter(|&&other| other < time)
            .count();
        stat.total_time += u64::from(time);
        stat.run_count += 1;
    }
}

fn save_leaderboard_to_file(filename: &str, label: &str, stats: &mut [StrategyStats]) -> std::io::Result<()> {
    const NAME_WIDTH: usize = 30;
    const COL: usize = 14;

    stats.sort_by(|a, b| {
        a.rank_sum
            .cmp(&b.rank_sum)
            .then_with(|| a.avg_time().total_cmp(&b.avg_time()))
    });

    let mut file = BufWriter::new(File::create(filename)?);
    writeln!(file, "{label} Leaderboard {}", "-".repeat(53))?;
    writeln!(file, "{:<10}  {:<NAME_WIDTH$}  {:<COL$}  {:<COL$}", "Placement", "Strategy", "Avg time", "Total points")?;
    writeln!(file, "{}", "-".repeat(NAME_WIDTH + COL * 2 + 16))?;

    for (place, s) in stats.iter().enumerate() {
        let medal = if place < 3 { (place + 1).to_string() } else { "-".to_string() };
        writeln!(file, "{:<10}  {:<NAME_WIDTH$}  {:<COL$.1}  {:<COL$}", medal, format!("{:?}", s.strategy), s.avg_time(), s.rank_sum)?;
    }
    file.flush()?;

    println!("Leaderboard saved to {filename}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_times_share_the_same_place() {
        let mut stats: Vec<StrategyStats> = SortStrategy::all()
            .iter()
            .take(3)
            .map(|&s| StrategyStats::new(s))
            .collect();

        accumulate(&[10, 10, 20], &mut stats);

        let places: Vec<usize> = stats.iter().map(|s| s.rank_sum).collect();
        assert_eq!(places, vec![1, 1, 3]);
        assert_eq!(stats[2].total_time, 20);
    }
}
