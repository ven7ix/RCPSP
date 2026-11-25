use rand::{RngExt, SeedableRng, rngs::StdRng};

use crate::schedule::Schedule;

/// Ключ сортировки операции: меньше — планируется раньше.
pub type PriorityKey = (i64, i64);

/// Правило приоритета: какую из доступных операций планировать первой.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortStrategy {
    PriorityThenDueTime,
    DueTimeThenPriority,
    ShortestDurationFirst,
    LongestDurationFirst,
    MostSuccessorsFirst,
    FewestSuccessorsFirst,
    LongestSuccessorDuration,
    ShortestSuccessorDuration,
    EarliestDueDate,
    LongestCriticalPathFirst,
    Random,
}

impl SortStrategy {
    const ALL: [SortStrategy; 11] = [SortStrategy::PriorityThenDueTime, SortStrategy::DueTimeThenPriority, SortStrategy::ShortestDurationFirst, SortStrategy::LongestDurationFirst, SortStrategy::MostSuccessorsFirst, SortStrategy::FewestSuccessorsFirst, SortStrategy::LongestSuccessorDuration, SortStrategy::ShortestSuccessorDuration, SortStrategy::EarliestDueDate, SortStrategy::LongestCriticalPathFirst, SortStrategy::Random];

    pub fn all() -> &'static [SortStrategy] {
        &Self::ALL
    }
}

impl Schedule {
    /// Ключ приоритета каждой операции по выбранной стратегии: чем меньше, тем раньше операция планируется.
    /// Все правила статические, поэтому ключи считаются один раз. При равных ключах побеждает та операция,
    /// которая раньше стала доступной (FIFO) — это перемешивает цепочки, а не планирует их по одной.
    pub(crate) fn priority_keys(&self, strategy: SortStrategy) -> Result<Vec<PriorityKey>, String> {
        let path_lengths = match strategy {
            SortStrategy::LongestCriticalPathFirst => self.compute_critical_path_data()?.path_lengths,
            _ => Vec::new(),
        };
        let mut rng = StdRng::seed_from_u64(self.random_seed);

        let keys = self
            .operations
            .iter()
            .enumerate()
            .map(|(id, op)| {
                let batch = &self.batches[op.assigned_batch_id];
                let (priority, due) = (batch.priority as i64, i64::from(batch.due_time));
                let successors_duration: i64 = op
                    .successor_ids
                    .iter()
                    .map(|&s| i64::from(self.operations[s].duration))
                    .sum();
                let successors_count = op.successor_ids.len() as i64;
                let duration = i64::from(op.duration);

                match strategy {
                    SortStrategy::PriorityThenDueTime => (priority, due),
                    SortStrategy::DueTimeThenPriority => (due, priority),
                    SortStrategy::ShortestDurationFirst => (duration, 0),
                    SortStrategy::LongestDurationFirst => (-duration, 0),
                    SortStrategy::MostSuccessorsFirst => (-successors_count, 0),
                    SortStrategy::FewestSuccessorsFirst => (successors_count, 0),
                    SortStrategy::LongestSuccessorDuration => (-successors_duration, 0),
                    SortStrategy::ShortestSuccessorDuration => (successors_duration, 0),
                    SortStrategy::EarliestDueDate => (due, 0),
                    SortStrategy::LongestCriticalPathFirst => (-i64::from(path_lengths[id]), 0),
                    SortStrategy::Random => (i64::from(rng.random::<u32>()), 0),
                }
            })
            .collect();
        Ok(keys)
    }
}
