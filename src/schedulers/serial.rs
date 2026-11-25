use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::Scheduler;
use crate::indices::OperationId;
use crate::priority::PriorityKey;
use crate::schedule::Schedule;

/// Последовательная схема: операции выбираются по одной в порядке приоритета и ставятся
/// на самое раннее возможное время (в том числе в «окна» между уже поставленными).
#[derive(Clone, Copy, Debug, Default)]
pub struct SerialScheduler;

impl Scheduler for SerialScheduler {
    fn name(&self) -> &'static str {
        "Serial"
    }

    fn build(&self, schedule: &mut Schedule, keys: &[PriorityKey]) -> Result<(), String> {
        let mut remaining = schedule.predecessor_counts();
        // Доступные операции: (ключ, порядковый номер появления, id) — номер даёт FIFO при равных ключах.
        let mut eligible: BinaryHeap<Reverse<(PriorityKey, usize, OperationId)>> = BinaryHeap::new();
        let mut arrivals = 0;
        for id in (0..remaining.len()).filter(|&id| remaining[id] == 0) {
            eligible.push(Reverse((keys[id], arrivals, id)));
            arrivals += 1;
        }
        let mut scheduled = 0;

        while let Some(Reverse((_, _, id))) = eligible.pop() {
            let (resource_id, span) = schedule.find_slot(id, schedule.ready_time(id))?;
            schedule.commit(id, resource_id, span);
            scheduled += 1;

            for &successor in &schedule.operations[id].successor_ids {
                remaining[successor] -= 1;
                if remaining[successor] == 0 {
                    eligible.push(Reverse((keys[successor], arrivals, successor)));
                    arrivals += 1;
                }
            }
        }

        schedule.ensure_all_scheduled(scheduled)
    }
}
