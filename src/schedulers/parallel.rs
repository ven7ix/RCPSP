use std::collections::BTreeMap;

use super::Scheduler;
use crate::indices::OperationId;
use crate::priority::PriorityKey;
use crate::schedule::Schedule;
use crate::time::Time;

/// Параллельная схема: время движется вперёд, в каждый момент свободные ресурсы
/// отдаются доступным операциям по приоритету.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParallelScheduler;

impl Scheduler for ParallelScheduler {
    fn name(&self) -> &'static str {
        "Parallel"
    }

    fn build(&self, schedule: &mut Schedule, keys: &[PriorityKey]) -> Result<(), String> {
        let mut remaining = schedule.predecessor_counts();
        let mut released = vec![false; remaining.len()];
        // Время, когда операция может начаться → операции, ожидающие этого момента (в порядке появления).
        let mut pending: BTreeMap<Time, Vec<OperationId>> = BTreeMap::new();
        for id in (0..remaining.len()).filter(|&id| remaining[id] == 0) {
            released[id] = true;
            pending
                .entry(schedule.ready_time(id))
                .or_default()
                .push(id);
        }
        let mut scheduled = 0;

        while let Some((time, mut queue)) = pending.pop_first() {
            queue.sort_by_key(|&id| keys[id]); // сортировка устойчивая: при равных ключах порядок появления сохраняется

            let mut completed = Vec::new();
            for id in queue {
                let (resource_id, span) = schedule.find_slot(id, time)?;
                if span.start > time {
                    // Ресурсы заняты: операция ждёт, пока освободится самый ранний из них.
                    pending.entry(span.start).or_default().push(id);
                    continue;
                }

                schedule.commit(id, resource_id, span);
                completed.push(id);
                for &successor in &schedule.operations[id].successor_ids {
                    remaining[successor] -= 1;
                }
            }
            scheduled += completed.len();

            // Последователи становятся доступными, когда поставлены все их предшественники.
            // Если операция нулевой длины, она попадёт в очередь на тот же `time` и будет обработана следующей.
            for id in completed {
                for &successor in &schedule.operations[id].successor_ids {
                    if remaining[successor] == 0 && !released[successor] {
                        released[successor] = true;
                        pending
                            .entry(schedule.ready_time(successor))
                            .or_default()
                            .push(successor);
                    }
                }
            }
        }

        schedule.ensure_all_scheduled(scheduled)
    }
}
