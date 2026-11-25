use crate::priority::{PriorityKey, SortStrategy};
use crate::schedule::Schedule;
use crate::schedulers::{ParallelScheduler, Scheduler, SerialScheduler};

/// Сколько двусторонних проходов `find_best` и лидерборд делают по умолчанию, пока есть улучшение.
pub const DEFAULT_IMPROVE_ROUNDS: usize = 10;

impl Schedule {
    /// Строит расписание по готовым ключам приоритета (прошлый результат сбрасывается).
    pub(crate) fn run(&mut self, scheduler: &dyn Scheduler, keys: &[PriorityKey]) -> Result<(), String> {
        self.reset();
        scheduler.build(self, keys)
    }

    /// Строит расписание выбранным алгоритмом и правилом приоритета (прошлый результат сбрасывается).
    pub fn compute(&mut self, scheduler: &dyn Scheduler, strategy: SortStrategy) -> Result<(), String> {
        let keys = self.priority_keys(strategy)?;
        self.run(scheduler, &keys)
    }

    /// То же, что `compute`, но затем расписание улучшается двусторонним проходом (см. `improve`).
    pub fn compute_improved(&mut self, scheduler: &dyn Scheduler, strategy: SortStrategy, max_rounds: usize) -> Result<(), String> {
        self.compute(scheduler, strategy)?;
        self.improve(scheduler, max_rounds).map(|_| ())
    }

    pub fn compute_serial(&mut self, strategy: SortStrategy) -> Result<(), String> {
        self.compute(&SerialScheduler, strategy)
    }

    pub fn compute_parallel(&mut self, strategy: SortStrategy) -> Result<(), String> {
        self.compute(&ParallelScheduler, strategy)
    }

    /// Перебирает все стратегии и возвращает расписание с наименьшей длительностью.
    /// При `improve_rounds > 0` каждое расписание дополнительно улучшается двусторонним проходом.
    /// `None`, если ни одна стратегия не смогла построить расписание.
    pub fn find_best(original: &Schedule, scheduler: &dyn Scheduler, improve_rounds: usize) -> Option<(Schedule, SortStrategy)> {
        SortStrategy::all()
            .iter()
            .filter_map(|&strategy| {
                let mut schedule = original.clone();
                match schedule.compute_improved(scheduler, strategy, improve_rounds) {
                    Ok(()) => Some((schedule, strategy)),
                    Err(e) => {
                        eprintln!("{scheduler:?} with strategy {strategy:?} failed: {e}");
                        None
                    }
                }
            })
            .min_by_key(|(schedule, _)| schedule.total_execute_time())
    }
}
