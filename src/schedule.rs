use std::collections::{BTreeMap, VecDeque};

use crate::indices::*;
use crate::job::*;
use crate::time::*;
use crate::worker::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortStrategy {
    PriorityThenDueTime,
    DueTimeThenPriority,
    ShortestDurationFirst,
    LongestDurationFirst,
    MostSuccessorsFirst,
    FewestSuccessorsFirst,
    MostTotalSuccessorDuration,
    FewestTotalSuccessorDuration,
    EarliestDueDate,
    LongestCriticalPathFirst,
    Random,
}

fn sort_queue_by_strategy(
    operations: &Vec<Operation>,
    batches: &Vec<Batch>,
    queue: &mut VecDeque<OperationId>,
    strategy: SortStrategy,
    critical_paths: &[Time],
) {
    let mut vec: Vec<OperationId> = queue.drain(..).collect();

    match strategy {
        SortStrategy::PriorityThenDueTime => {
            vec.sort_by_key(|id: &OperationId| {
                let op: &Operation = &operations[*id];
                let batch: &Batch = &batches[op.assigned_batch_id];
                (batch.priority, batch.due_time)
            });
        }
        SortStrategy::DueTimeThenPriority => {
            vec.sort_by_key(|id: &OperationId| {
                let op: &Operation = &operations[*id];
                let batch: &Batch = &batches[op.assigned_batch_id];
                (batch.due_time, batch.priority)
            });
        }
        SortStrategy::ShortestDurationFirst => {
            vec.sort_by_key(|id: &OperationId| operations[*id].duration);
        }
        SortStrategy::LongestDurationFirst => {
            vec.sort_by_key(|id: &OperationId| std::cmp::Reverse(operations[*id].duration));
        }
        SortStrategy::MostSuccessorsFirst => {
            vec.sort_by_key(|id: &OperationId| {
                std::cmp::Reverse(operations[*id].successor_ids.len())
            });
        }
        SortStrategy::FewestSuccessorsFirst => {
            vec.sort_by_key(|id: &OperationId| operations[*id].successor_ids.len());
        }
        SortStrategy::MostTotalSuccessorDuration => {
            vec.sort_by_key(|&id| {
                let total: Time = operations[id]
                    .successor_ids
                    .iter()
                    .map(|&sid| operations[sid].duration)
                    .sum();
                std::cmp::Reverse(total)
            });
        }
        SortStrategy::FewestTotalSuccessorDuration => {
            vec.sort_by_key(|&id| {
                operations[id]
                    .successor_ids
                    .iter()
                    .map(|&sid| operations[sid].duration)
                    .sum::<Time>()
            });
        }
        SortStrategy::EarliestDueDate => {
            vec.sort_by_key(|&id| batches[operations[id].assigned_batch_id].due_time);
        }
        SortStrategy::LongestCriticalPathFirst => {
            vec.sort_by_key(|&id| std::cmp::Reverse(critical_paths[id]));
        }
        SortStrategy::Random => {
            // недетерминированная сортировка (перемешивание)
            use rand::seq::SliceRandom;
            vec.shuffle(&mut rand::rng());
        }
    }

    queue.extend(vec);
}

// итеративность за счет сортировки фронта
// генератор
// сравнить методы
// транспортировка между ресурсами ?

#[derive(Clone)]
pub struct Schedule {
    resource_groups: Vec<ResourceGroup>,
    pub batches: Vec<Batch>,
    pub operations: Vec<Operation>,
    pending_operation_ids: BTreeMap<Time, VecDeque<OperationId>>,
}

impl Schedule {
    pub fn new() -> Self {
        return Self {
            resource_groups: Vec::new(),
            batches: Vec::new(),
            operations: Vec::new(),
            pending_operation_ids: BTreeMap::new(),
        };
    }

    pub fn add_resource_group(&mut self, resource_group: ResourceGroup) -> ResourceGroupId {
        self.resource_groups.push(resource_group);
        return self.resource_groups.len() - 1;
    }

    pub fn add_batch(&mut self, batch: Batch) -> BatchId {
        self.batches.push(batch);
        return self.batches.len() - 1;
    }

    pub fn add_operation(&mut self, operation: Operation) -> OperationId {
        self.operations.push(operation);
        return self.operations.len() - 1;
    }

    pub fn add_precedence(&mut self, predecessor_id: OperationId, successor_id: OperationId) {
        self.operations[predecessor_id]
            .successor_ids
            .push(successor_id);
        self.operations[successor_id]
            .predecessor_ids
            .push(predecessor_id);
    }

    fn init_pending_opeations(&mut self, strategy: SortStrategy, critical_paths: &[Time]) {
        for operation in &self.operations {
            if operation.predecessor_ids.len() > 0 {
                continue;
            }

            let start_time: Time = self.batches[operation.assigned_batch_id].start_time;
            self.pending_operation_ids
                .entry(start_time)
                .or_default()
                .push_back(operation.id);
        }

        for queue in self.pending_operation_ids.values_mut() {
            sort_queue_by_strategy(
                &self.operations,
                &self.batches,
                queue,
                strategy,
                critical_paths,
            );
        }
    }

    fn find_next_event_time(&self, current_time: Time) -> Option<Time> {
        let mut next_event_time: Option<Time> = self
            .resource_groups
            .iter()
            .map(|resource_group: &ResourceGroup| resource_group.next_available_time)
            .filter(|time: &Time| *time > current_time)
            .min();

        if let Some((&time, _)) = self
            .pending_operation_ids
            .range((current_time + 1)..)
            .next()
        {
            match next_event_time {
                None => next_event_time = Some(time),
                Some(existing) if time < existing => next_event_time = Some(time),
                _ => {}
            }
        }

        return next_event_time;
    }

    fn add_successors_to_pending_operations(
        &mut self,
        current_time: Time,
        current_completed_operation_ids: Vec<OperationId>,
        strategy: SortStrategy,
        critical_paths: &[Time],
    ) {
        for operation_id in current_completed_operation_ids {
            let operation = &self.operations[operation_id];
            let successor_ids = &operation.successor_ids;
            for successor_id in successor_ids {
                let successor = &self.operations[*successor_id];
                if successor.is_scheduled()
                    || successor
                        .predecessor_ids
                        .iter()
                        .any(|id: &OperationId| !self.operations[*id].is_scheduled())
                {
                    continue;
                }

                let max_predecessor_end: Time = successor
                    .predecessor_ids
                    .iter()
                    .map(|&pid| {
                        self.operations[pid]
                            .scheduled_span
                            .expect("predecessor must be scheduled")
                            .end
                    })
                    .max()
                    .unwrap_or(0);

                let mut start_time: Time = self.batches[successor.assigned_batch_id]
                    .start_time
                    .max(max_predecessor_end);
                if start_time < current_time {
                    start_time = current_time;
                }

                let queue = self.pending_operation_ids.entry(start_time).or_default();

                if !queue.contains(successor_id) {
                    queue.push_back(*successor_id);
                    sort_queue_by_strategy(
                        &self.operations,
                        &self.batches,
                        queue,
                        strategy,
                        critical_paths,
                    );
                }
            }
        }
    }

    fn update_pending_operation_ids(&mut self, current_time: Time, next_event_time: Time) {
        if let Some(current_time_queue) = self.pending_operation_ids.remove(&current_time) {
            self.pending_operation_ids
                .entry(next_event_time)
                .or_default()
                .extend(current_time_queue.iter());
        }
    }

    pub fn compute_parallel(
        &mut self,
        strategy: SortStrategy,
        mut progress_callback: Option<&mut dyn FnMut(usize, usize)>,
    ) -> Result<(), String> {
        let critical_paths = self.compute_critical_paths();
        self.init_pending_opeations(strategy, &critical_paths);
        let mut current_time: Time = 0;

        let total_operations_count: usize = self.operations.len();
        let mut unscheduled_operations_count: usize = self.operations.len();

        while unscheduled_operations_count > 0 {
            let mut current_completed_operation_ids: Vec<OperationId> = Default::default();

            if let Some(current_time_queue) = self.pending_operation_ids.get_mut(&current_time) {
                let initial_queue_len: usize = current_time_queue.len();
                for _ in 0..initial_queue_len {
                    let operation_id: OperationId = current_time_queue.pop_front().unwrap();

                    let operation: &Operation = &self.operations[operation_id];
                    let resource_group: &mut ResourceGroup =
                        &mut self.resource_groups[operation.assigned_resource_group_id];

                    if let Some((allocated_resource_id, work_span)) = resource_group
                        .allocate_best_resource_for_operation(operation.duration, current_time)
                    {
                        let operation: &mut Operation = &mut self.operations[operation_id];
                        operation.assigned_resource_id = Some(allocated_resource_id);
                        operation.scheduled_span = Some(work_span);

                        unscheduled_operations_count -= 1;

                        current_completed_operation_ids.push(operation_id);

                        if let Some(ref mut callback) = progress_callback {
                            callback(
                                total_operations_count - unscheduled_operations_count,
                                unscheduled_operations_count,
                            );
                        }

                        // if work_span.start == current_time {

                        // }
                        // else {
                        //     current_time_queue.push_back(operation_id);
                        // }
                    } else {
                        current_time_queue.push_back(operation_id);
                    }
                }
            }

            if unscheduled_operations_count == 0 {
                break;
            }

            self.add_successors_to_pending_operations(
                current_time,
                current_completed_operation_ids,
                strategy,
                &critical_paths,
            );

            let next_event_time: Time = self
                .find_next_event_time(current_time)
                .ok_or("no future event times")?;
            if next_event_time == current_time {
                return Err(format!(
                    "unable to find next_event_time after current_time: {current_time}. unscheduled_operations_count: {unscheduled_operations_count}"
                ));
            }

            self.update_pending_operation_ids(current_time, next_event_time);

            for queue in self.pending_operation_ids.values_mut() {
                sort_queue_by_strategy(
                    &self.operations,
                    &self.batches,
                    queue,
                    strategy,
                    &critical_paths,
                );
            }

            current_time = next_event_time;
        }

        return Ok(());
    }

    pub fn compute_parallel_with_console_progress(
        &mut self,
        strategy: SortStrategy,
    ) -> Result<(), String> {
        use std::io::{self, Write};

        let total = self.operations.len();
        let mut scheduled = 0;
        let width = 40usize; // ширина полосы в символах

        let mut callback = |sched: usize, _total: usize| {
            scheduled = sched;
            let filled = (scheduled as f64 / total as f64 * width as f64) as usize;
            let empty = width - filled;
            print!(
                "\r[{}>{}] {}/{}",
                "=".repeat(filled),
                " ".repeat(empty),
                scheduled,
                total
            );
            io::stdout().flush().unwrap();
        };

        let result: Result<(), String> = self.compute_parallel(strategy, Some(&mut callback));

        // Завершаем строку
        println!();
        return result;
    }

    pub fn find_best_schedule(original: &Schedule) -> (Schedule, SortStrategy, Time) {
        let strategies: [SortStrategy; 10] = [
            SortStrategy::PriorityThenDueTime,
            SortStrategy::DueTimeThenPriority,
            SortStrategy::ShortestDurationFirst,
            SortStrategy::LongestDurationFirst,
            SortStrategy::MostSuccessorsFirst,
            SortStrategy::FewestSuccessorsFirst,
            SortStrategy::MostTotalSuccessorDuration,
            SortStrategy::FewestTotalSuccessorDuration,
            SortStrategy::EarliestDueDate,
            SortStrategy::LongestCriticalPathFirst,
        ];

        let mut best_schedule = original.clone();
        let mut best_execute_time = Time::MAX;
        let mut best_strategy = strategies[0];

        for &strategy in &strategies {
            let mut schedule: Schedule = original.clone();
            match schedule.compute_parallel(strategy, None) {
                Ok(()) => {
                    let execute_time = schedule.total_execute_time();
                    if execute_time < best_execute_time {
                        best_execute_time = execute_time;
                        best_schedule = schedule;
                        best_strategy = strategy;
                    }
                }
                Err(e) => {
                    eprintln!("Strategy {:?} failed: {}", strategy, e);
                }
            }
        }

        return (best_schedule, best_strategy, best_execute_time);
    }

    fn earliest_start(&self, operation_id: OperationId) -> Time {
        let operation: &Operation = &self.operations[operation_id];

        let max_predecessor_end_time: Time = operation
            .predecessor_ids
            .iter()
            .map(|id: &OperationId| {
                self.operations[*id]
                    .scheduled_span
                    .map(|span: Span| span.end)
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);

        return max_predecessor_end_time.max(self.batches[operation.assigned_batch_id].start_time);
    }

    fn select_operation_by_priority_then_due_time(
        &self,
        eligible_operation_ids: &[OperationId],
    ) -> Option<OperationId> {
        return eligible_operation_ids
            .iter()
            .min_by_key(|id: &&OperationId| {
                let operation: &Operation = &self.operations[**id];
                let batch: &Batch = &self.batches[operation.assigned_batch_id];
                return (batch.priority, batch.due_time);
            })
            .copied();
    }

    fn get_eligible_operation_ids(&self) -> Vec<OperationId> {
        return self
            .operations
            .iter()
            .filter(|op: &&Operation| {
                return !op.is_scheduled()
                    && op
                        .predecessor_ids
                        .iter()
                        .all(|id: &OperationId| self.operations[*id].is_scheduled());
            })
            .map(|op: &Operation| op.id)
            .collect();
    }

    pub fn compute_serial(&mut self) -> Result<(), String> {
        let mut eligible_operation_ids: Vec<OperationId> = self.get_eligible_operation_ids();

        while !eligible_operation_ids.is_empty() {
            let operation_id: OperationId = self
                .select_operation_by_priority_then_due_time(&eligible_operation_ids)
                .ok_or("No eligible operations")?;

            let operation_earliest_start: Time = self.earliest_start(operation_id);
            let operation: &mut Operation = &mut self.operations[operation_id];
            let resource_group: &mut ResourceGroup =
                &mut self.resource_groups[operation.assigned_resource_group_id];

            if let Some((allocated_resource_id, work_span)) = resource_group
                .allocate_best_resource_for_operation(operation.duration, operation_earliest_start)
            {
                operation.assigned_resource_id = Some(allocated_resource_id);
                operation.scheduled_span = Some(work_span);
                let operation: &Operation = &self.operations[operation_id];

                eligible_operation_ids.retain(|id: &usize| *id != operation_id);

                for successor_id in &operation.successor_ids {
                    let successor: &Operation = &self.operations[*successor_id];

                    if !successor.is_scheduled()
                        && successor
                            .predecessor_ids
                            .iter()
                            .all(|pred: &OperationId| self.operations[*pred].is_scheduled())
                    {
                        if !eligible_operation_ids.contains(successor_id) {
                            eligible_operation_ids.push(*successor_id);
                        }
                    }
                }
            } else {
                return Err(format!(
                    "Couldnt been able to allocate resource for operation: {operation_id}"
                ));
            }
        }

        return Ok(());
    }

    fn compute_critical_paths(&self) -> Vec<Time> {
        let n = self.operations.len();
        let mut paths = vec![0; n];

        // рекурсивная функция с мемоизацией
        fn dfs(op_id: usize, ops: &[Operation], paths: &mut [Time]) -> Time {
            if paths[op_id] > 0 {
                return paths[op_id];
            }
            let mut max_succ = 0;
            for &succ_id in &ops[op_id].successor_ids {
                let succ_path = dfs(succ_id, ops, paths);
                if succ_path > max_succ {
                    max_succ = succ_path;
                }
            }
            paths[op_id] = ops[op_id].duration + max_succ;
            paths[op_id]
        }

        for id in 0..n {
            if paths[id] == 0 {
                dfs(id, &self.operations, &mut paths);
            }
        }

        return paths;
    }

    pub fn total_execute_time(&self) -> Time {
        return self
            .operations
            .iter()
            .filter_map(|oper: &Operation| oper.scheduled_span.map(|span: Span| span.end))
            .max()
            .unwrap_or(0);
    }

    pub fn print(&self, schedule_name: &str) {
        println!("schedule ({schedule_name}):");

        for operation in &self.operations {
            if let Some(span) = operation.scheduled_span {
                println!("operation {}: [{}, {})", operation.id, span.start, span.end);
            } else {
                println!("operation {}: not assinged", operation.id);
            }
        }

        println!("total execute time: {}", self.total_execute_time());
    }

    pub fn save_to_file(&self, filename: &str, strategy_name: &str) -> std::io::Result<()> {
        use std::fs::File;
        use std::io::Write;

        let mut file = File::create(filename)?;
        writeln!(file, "Schedule generated with strategy: {}", strategy_name)?;
        writeln!(file, "Total execute time: {}", self.total_execute_time())?;
        writeln!(file, "Operations:")?;

        for op in &self.operations {
            if let Some(span) = op.scheduled_span {
                writeln!(
                    file,
                    "operation id: {:3} | earliest start: {:3} | start: {:4} | end: {:4} | duration: {:3} | batch id: {} | group id: {} | predecessor ids: {:?}",
                    op.id,
                    &self.batches[op.assigned_batch_id].start_time,
                    span.start,
                    span.end,
                    op.duration,
                    op.assigned_batch_id,
                    op.assigned_resource_group_id,
                    op.predecessor_ids,
                )?;
            } else {
                writeln!(file, "operation id: {:3} | NOT SCHEDULED", op.id)?;
            }
        }
        Ok(())
    }
}
