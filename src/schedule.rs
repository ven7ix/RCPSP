use std::io::Write;

use crate::indices::*;
use crate::job::*;
use crate::time::*;
use crate::worker::*;

/// Задача (операции, партии, ресурсы) вместе с результатом планирования.
///
/// Сами алгоритмы живут в `schedulers`, правила приоритета — в `priority`;
/// здесь только данные и шаги, общие для всех алгоритмов.
#[derive(Clone, Default)]
pub struct Schedule {
    pub(crate) resource_groups: Vec<ResourceGroup>,
    pub batches: Vec<Batch>,
    pub operations: Vec<Operation>,
    /// Зерно для стратегии `Random`: одно и то же зерно даёт одно и то же расписание.
    pub random_seed: u64,
}

impl Schedule {
    pub fn new() -> Self {
        Self::default()
    }

    // ─── Building the problem ─────────────────────────────────────────────────

    pub fn add_resource_group(&mut self, resource_group: ResourceGroup) -> ResourceGroupId {
        debug_assert_eq!(resource_group.id, self.resource_groups.len());
        self.resource_groups.push(resource_group);
        self.resource_groups.len() - 1
    }

    pub fn add_batch(&mut self, batch: Batch) -> BatchId {
        debug_assert_eq!(batch.id, self.batches.len());
        self.batches.push(batch);
        self.batches.len() - 1
    }

    pub fn add_operation(&mut self, operation: Operation) -> OperationId {
        debug_assert_eq!(operation.id, self.operations.len());
        self.operations.push(operation);
        self.operations.len() - 1
    }

    /// Добавляет связь «`predecessor_id` должна закончиться до начала `successor_id`». Дубликаты игнорируются.
    pub fn add_precedence(&mut self, predecessor_id: OperationId, successor_id: OperationId) {
        if self.operations[predecessor_id]
            .successor_ids
            .contains(&successor_id)
        {
            return;
        }
        self.operations[predecessor_id]
            .successor_ids
            .push(successor_id);
        self.operations[successor_id]
            .predecessor_ids
            .push(predecessor_id);
    }

    // ─── Steps shared by all schedulers ───────────────────────────────────────

    /// Сколько у каждой операции ещё не поставленных предшественников.
    pub(crate) fn predecessor_counts(&self) -> Vec<usize> {
        self.operations
            .iter()
            .map(|op| op.predecessor_ids.len())
            .collect()
    }

    /// Раньше этого момента операцию начинать нельзя: старт партии и окончание всех предшественников.
    pub(crate) fn ready_time(&self, id: OperationId) -> Time {
        let op = &self.operations[id];
        op.predecessor_ids
            .iter()
            .filter_map(|&p| self.operations[p].scheduled_span)
            .map(|span| span.end)
            .fold(self.batches[op.assigned_batch_id].start_time, Time::max)
    }

    /// Лучший ресурс группы операции и интервал на нём, начинающийся не раньше `after`.
    pub(crate) fn find_slot(&self, id: OperationId, after: Time) -> Result<(ResourceId, Span), String> {
        let op = &self.operations[id];
        self.resource_groups[op.assigned_resource_group_id]
            .find_best_resource_for_operation(op.duration, after)
            .ok_or_else(|| format!("resource group {} cannot allocate operation {id} at all", op.assigned_resource_group_id))
    }

    /// Закрепляет операцию за ресурсом на интервале `span`.
    pub(crate) fn commit(&mut self, id: OperationId, resource_id: ResourceId, span: Span) {
        let op = &mut self.operations[id];
        self.resource_groups[op.assigned_resource_group_id].allocate_resource(resource_id, span);
        op.assigned_resource_id = Some(resource_id);
        op.scheduled_span = Some(span);
    }

    /// Сбрасывает результаты предыдущего расчёта, чтобы расписание можно было пересчитать.
    pub(crate) fn reset(&mut self) {
        self.resource_groups
            .iter_mut()
            .for_each(ResourceGroup::clear);
        for op in &mut self.operations {
            op.assigned_resource_id = None;
            op.scheduled_span = None;
        }
    }

    /// Проверка после работы алгоритма: поставлены ли все операции (иначе в графе цикл).
    pub(crate) fn ensure_all_scheduled(&self, scheduled: usize) -> Result<(), String> {
        let total = self.operations.len();
        if scheduled == total { Ok(()) } else { Err(format!("only {scheduled} of {total} operations were scheduled (cycle in precedence graph?)")) }
    }

    // ─── Output ───────────────────────────────────────────────────────────────

    pub fn total_execute_time(&self) -> Time {
        self.operations
            .iter()
            .filter_map(|op| op.scheduled_span.map(|span| span.end))
            .max()
            .unwrap_or(0)
    }

    pub fn print(&self, schedule_name: &str) {
        println!("schedule ({schedule_name}):");

        for op in &self.operations {
            match op.scheduled_span {
                Some(span) => println!("operation {}: [{}, {})", op.id, span.start, span.end),
                None => println!("operation {}: not assigned", op.id),
            }
        }

        println!("total execute time: {}", self.total_execute_time());
    }

    pub fn save_to_file(&self, filename: &str, strategy_name: &str) -> std::io::Result<()> {
        let mut file = std::io::BufWriter::new(std::fs::File::create(filename)?);
        writeln!(file, "Schedule generated with strategy: {strategy_name}")?;
        writeln!(file, "Total execute time: {}", self.total_execute_time())?;
        writeln!(file, "Operations:")?;

        for op in &self.operations {
            match op.scheduled_span {
                Some(span) => writeln!(file, "operation id: {:3} | earliest start: {:3} | start: {:4} | end: {:4} | duration: {:3} | batch id: {} | group id: {} | predecessor ids: {:?}", op.id, self.batches[op.assigned_batch_id].start_time, span.start, span.end, op.duration, op.assigned_batch_id, op.assigned_resource_group_id, op.predecessor_ids)?,
                None => writeln!(file, "operation id: {:3} | NOT SCHEDULED", op.id)?,
            }
        }
        file.flush()
    }
}
