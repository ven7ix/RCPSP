//! Двусторонний проход (forward-backward improvement): улучшает уже построенное расписание
//! любым алгоритмом, потому что использует только трейт `Scheduler`.

use crate::job::Operation;
use crate::priority::PriorityKey;
use crate::schedule::Schedule;
use crate::schedulers::Scheduler;

impl Schedule {
    /// Зеркальная задача: связи развёрнуты, времена готовности партий сброшены в 0.
    /// Расписание в ней, прочитанное с конца, — это «сдвиг всех операций как можно позже».
    fn mirrored(&self) -> Schedule {
        let mut mirror = self.clone();
        mirror.reset();
        for op in &mut mirror.operations {
            std::mem::swap(&mut op.predecessor_ids, &mut op.successor_ids);
        }
        for batch in &mut mirror.batches {
            batch.start_time = 0;
        }
        mirror
    }

    /// Один двусторонний проход: возвращает новое корректное расписание (не обязательно лучшее).
    ///
    /// 1. Назад: операции по убыванию окончания строятся тем же алгоритмом в зеркальной задаче,
    ///    так получаются самые поздние старты при неизменной длительности.
    /// 2. Вперёд: порядок самых поздних стартов становится приоритетом, и расписание
    ///    строится заново тем же алгоритмом с самым ранним временем начала.
    fn justified(&self, scheduler: &dyn Scheduler) -> Result<Schedule, String> {
        let span = |op: &Operation| {
            op.scheduled_span
                .expect("justified is called on a computed schedule")
        };
        let horizon = i64::from(self.total_execute_time());

        let backward_keys: Vec<PriorityKey> = self
            .operations
            .iter()
            .map(|op| (-i64::from(span(op).end), -i64::from(span(op).start)))
            .collect();
        let mut mirror = self.mirrored();
        mirror.run(scheduler, &backward_keys)?;

        let forward_keys: Vec<PriorityKey> = self
            .operations
            .iter()
            .zip(&mirror.operations)
            .map(|(op, mirrored_op)| {
                // В обратном времени операция занимает [s, e), в исходном — [horizon - e, horizon - s).
                let latest_start = horizon - i64::from(span(mirrored_op).end);
                (latest_start, i64::from(span(op).start))
            })
            .collect();
        let mut candidate = self.clone();
        candidate.run(scheduler, &forward_keys)?;
        Ok(candidate)
    }

    /// Улучшает уже построенное расписание повторными двусторонними проходами, пока длительность
    /// уменьшается, но не более `max_rounds` раз. Принимаются только строго лучшие результаты,
    /// поэтому длительность никогда не растёт. Возвращает число принятых проходов.
    pub fn improve(&mut self, scheduler: &dyn Scheduler, max_rounds: usize) -> Result<usize, String> {
        if self
            .operations
            .iter()
            .any(|op| !op.is_scheduled())
        {
            return Err("schedule must be computed before improving it".to_string());
        }

        let mut accepted = 0;
        while accepted < max_rounds {
            let candidate = self.justified(scheduler)?;
            if candidate.total_execute_time() >= self.total_execute_time() {
                break;
            }
            *self = candidate;
            accepted += 1;
        }
        Ok(accepted)
    }
}
