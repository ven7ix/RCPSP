//! Сравнение стратегий и алгоритмов на множестве сидов: чем лучше расписание, тем выше место.

use std::fs::File;
use std::io::{BufWriter, Write};

use crate::priority::SortStrategy;
use crate::schedule::Schedule;
use crate::schedule_generator::{GenerationConfig, generate_schedule};
use crate::schedulers::{ParallelScheduler, Scheduler, SerialScheduler};
use crate::solver::DEFAULT_IMPROVE_ROUNDS;
use crate::time::Time;

/// Что именно сравнивается в лидерборде: алгоритм с улучшением или без.
#[derive(Clone, Copy)]
struct Mode {
    scheduler: &'static dyn Scheduler,
    improve_rounds: usize,
}

impl Mode {
    const ALL: [Mode; 4] = [Mode { scheduler: &SerialScheduler, improve_rounds: 0 }, Mode { scheduler: &ParallelScheduler, improve_rounds: 0 }, Mode { scheduler: &SerialScheduler, improve_rounds: DEFAULT_IMPROVE_ROUNDS }, Mode { scheduler: &ParallelScheduler, improve_rounds: DEFAULT_IMPROVE_ROUNDS }];

    fn label(&self) -> String {
        let suffix = if self.improve_rounds > 0 { "Improved" } else { "" };
        format!("{}{suffix}", self.scheduler.name())
    }

    fn file_name(&self) -> String {
        let suffix = if self.improve_rounds > 0 { "_improved" } else { "" };
        format!("leaderboard_{}{suffix}.txt", self.scheduler.name()).to_lowercase()
    }
}

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

pub fn run_leaderboard(config: &GenerationConfig) {
    // Статистика по каждому режиму; порядок совпадает с `Mode::ALL`.
    let mut stats: Vec<Vec<StrategyStats>> = Mode::ALL
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

        for (mode, mode_stats) in Mode::ALL.iter().zip(&mut stats) {
            match run_seed(&base_schedule, *mode) {
                Ok(times) => accumulate(&times, mode_stats),
                // Сид пропускается целиком, чтобы сбой не искажал места остальных стратегий.
                Err(e) => eprintln!("Seed {seed}: {} failed: {e}", mode.label()),
            }
        }
    }

    for (mode, mode_stats) in Mode::ALL.iter().zip(&mut stats) {
        let filename = mode.file_name();
        if let Err(e) = save_leaderboard_to_file(&filename, &mode.label(), mode_stats) {
            eprintln!("Failed to save leaderboard {filename}: {e}");
        }
    }
}

/// Длительность расписания для каждой стратегии (в порядке `SortStrategy::all()`).
fn run_seed(base_schedule: &Schedule, mode: Mode) -> Result<Vec<Time>, String> {
    SortStrategy::all()
        .iter()
        .map(|&strategy| {
            let mut schedule = base_schedule.clone();
            schedule.compute_improved(mode.scheduler, strategy, mode.improve_rounds)?;
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
