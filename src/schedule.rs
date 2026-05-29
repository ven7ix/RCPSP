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
    LongestSuccessorDuration,
    ShortestSuccessorDuration,
    EarliestDueDate,
    LongestCriticalPathFirst,
    Random,
}

impl SortStrategy {
    pub fn all() -> &'static [SortStrategy] {
        &[SortStrategy::PriorityThenDueTime, SortStrategy::DueTimeThenPriority, SortStrategy::ShortestDurationFirst, SortStrategy::LongestDurationFirst, SortStrategy::MostSuccessorsFirst, SortStrategy::FewestSuccessorsFirst, SortStrategy::LongestSuccessorDuration, SortStrategy::ShortestSuccessorDuration, SortStrategy::EarliestDueDate, SortStrategy::LongestCriticalPathFirst, SortStrategy::Random]
    }
}

fn sort_queue_by_strategy(operations: &Vec<Operation>, batches: &Vec<Batch>, critical_path_data: &Option<CriticalPathData>, queue: &mut VecDeque<OperationId>, strategy: SortStrategy) {
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
            vec.sort_by_key(|id: &OperationId| std::cmp::Reverse(operations[*id].successor_ids.len()));
        }
        SortStrategy::FewestSuccessorsFirst => {
            vec.sort_by_key(|id: &OperationId| operations[*id].successor_ids.len());
        }
        SortStrategy::LongestSuccessorDuration => {
            vec.sort_by_key(|&id| {
                let total: Time = operations[id]
                    .successor_ids
                    .iter()
                    .map(|&sid| operations[sid].duration)
                    .sum();
                std::cmp::Reverse(total)
            });
        }
        SortStrategy::ShortestSuccessorDuration => {
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
            let critical_path = critical_path_data
                .as_ref()
                .expect("cp_data must be initialized before sorting");
            vec.sort_by_key(|&id| std::cmp::Reverse(critical_path.path_lengths[id]));
        }
        SortStrategy::Random => {
            use rand::seq::SliceRandom;
            vec.shuffle(&mut rand::rng());
        }
    }

    queue.extend(vec);
}

#[derive(Clone)]
pub struct CriticalPathData {
    pub earliest_start: Vec<Time>,
    pub earliest_finish: Vec<Time>,
    pub latest_start: Vec<Time>,
    pub latest_finish: Vec<Time>,
    pub float: Vec<Time>,        // резерв времени (0 = на критическом пути)
    pub path_lengths: Vec<Time>, // длина пути от узла до листа (для сортировки)
}

#[derive(Clone)]
pub struct Schedule {
    resource_groups: Vec<ResourceGroup>,
    pub batches: Vec<Batch>,
    pub operations: Vec<Operation>,
    pending_operation_ids: BTreeMap<Time, VecDeque<OperationId>>,
    critical_path_data: Option<CriticalPathData>,
}

impl Schedule {
    pub fn new() -> Self {
        return Self { resource_groups: Vec::new(), batches: Vec::new(), operations: Vec::new(), pending_operation_ids: BTreeMap::new(), critical_path_data: None };
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

    // ─── Critical Path ────────────────────────────────────────────────────────

    /// Возвращает Err если в графе есть цикл.
    fn topological_order(&self) -> Result<Vec<OperationId>, String> {
        let n = self.operations.len();
        let mut in_degree: Vec<usize> = vec![0; n];

        for op in &self.operations {
            for &succ in &op.successor_ids {
                in_degree[succ] += 1;
            }
        }

        let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order: Vec<usize> = Vec::with_capacity(n);

        while let Some(id) = queue.pop_front() {
            order.push(id);
            for &succ in &self.operations[id].successor_ids {
                in_degree[succ] -= 1;
                if in_degree[succ] == 0 {
                    queue.push_back(succ);
                }
            }
        }

        if order.len() != n {
            return Err("cycle detected in precedence graph".to_string());
        }

        return Ok(order);
    }

    pub fn compute_critical_path_data(&self) -> Result<CriticalPathData, String> {
        let n = self.operations.len();
        let topo_order = self.topological_order()?;

        // --- Forward pass: ES, EF ---
        let mut es = vec![0 as Time; n];
        let mut ef = vec![0 as Time; n];

        for &id in &topo_order {
            let batch_start = self.batches[self.operations[id].assigned_batch_id].start_time;
            let max_pred_ef = self.operations[id]
                .predecessor_ids
                .iter()
                .map(|&p| ef[p])
                .max()
                .unwrap_or(0);

            es[id] = batch_start.max(max_pred_ef);
            ef[id] = es[id] + self.operations[id].duration;
        }

        let project_end: Time = ef.iter().copied().max().unwrap_or(0);

        // --- Backward pass: LF, LS ---
        let mut lf = vec![project_end; n];
        let mut ls = vec![project_end; n];

        for &id in topo_order.iter().rev() {
            let min_succ_ls = self.operations[id]
                .successor_ids
                .iter()
                .map(|&s| ls[s])
                .min()
                .unwrap_or(project_end);

            lf[id] = min_succ_ls;
            ls[id] = lf[id].saturating_sub(self.operations[id].duration);
        }

        // --- Float и path_lengths ---
        let float: Vec<Time> = (0..n)
            .map(|i| ls[i].saturating_sub(es[i]))
            .collect();

        let mut path_lengths = vec![0 as Time; n];
        for &id in topo_order.iter().rev() {
            let max_succ = self.operations[id]
                .successor_ids
                .iter()
                .map(|&s| path_lengths[s])
                .max()
                .unwrap_or(0);
            path_lengths[id] = self.operations[id].duration + max_succ;
        }

        return Ok(CriticalPathData { earliest_start: es, earliest_finish: ef, latest_start: ls, latest_finish: lf, float, path_lengths });
    }

    // ─── Parallel ──────────────────────────────────────────────────────────────

    fn init_pending_opeations(&mut self, strategy: SortStrategy) {
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
            sort_queue_by_strategy(&self.operations, &self.batches, &self.critical_path_data, queue, strategy);
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

    fn add_successors_to_pending_operations(&mut self, current_time: Time, current_completed_operation_ids: Vec<OperationId>, strategy: SortStrategy) {
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

                let queue = self
                    .pending_operation_ids
                    .entry(start_time)
                    .or_default();

                if !queue.contains(successor_id) {
                    queue.push_back(*successor_id);
                    sort_queue_by_strategy(&self.operations, &self.batches, &self.critical_path_data, queue, strategy);
                }
            }
        }
    }

    pub fn compute_parallel(&mut self, strategy: SortStrategy) -> Result<(), String> {
        self.critical_path_data = Some(self.compute_critical_path_data()?);
        self.init_pending_opeations(strategy);
        let mut current_time: Time = 0;

        let mut unscheduled_operations_count: usize = self.operations.len();

        while unscheduled_operations_count > 0 {
            loop {
                let mut any_scheduled: bool = false;
                let mut current_completed: Vec<OperationId> = Default::default();

                if let Some(mut current_time_queue) = self.pending_operation_ids.remove(&current_time) {
                    let initial_queue_len: usize = current_time_queue.len();
                    for _ in 0..initial_queue_len {
                        let operation_id: OperationId = current_time_queue.pop_front().unwrap();
                        let operation: &Operation = &self.operations[operation_id];
                        let resource_group: &mut ResourceGroup = &mut self.resource_groups[operation.assigned_resource_group_id];

                        match resource_group.find_best_resource_for_operation(operation.duration, current_time) {
                            Some((resource_id, span)) if span.start == current_time => {
                                resource_group.allocate_resource(resource_id, span);

                                let operation_mut = &mut self.operations[operation_id];
                                operation_mut.assigned_resource_id = Some(resource_id);
                                operation_mut.scheduled_span = Some(span);

                                unscheduled_operations_count -= 1;
                                current_completed.push(operation_id);

                                any_scheduled = true;
                            }
                            Some((_resource_id, span)) => {
                                self.pending_operation_ids
                                    .entry(span.start)
                                    .or_default()
                                    .push_back(operation_id);
                            }
                            None => {
                                return Err(format!("Resource group {} cannot allocate operation {} at all", operation.assigned_resource_group_id, operation_id));
                            }
                        }
                    }
                }

                self.add_successors_to_pending_operations(current_time, current_completed, strategy);

                for queue in self.pending_operation_ids.values_mut() {
                    sort_queue_by_strategy(&self.operations, &self.batches, &self.critical_path_data, queue, strategy);
                }

                if !any_scheduled {
                    break;
                }
            }

            if unscheduled_operations_count == 0 {
                break;
            }

            let next_event_time: Time = self
                .find_next_event_time(current_time)
                .ok_or("no future event times")?;
            if next_event_time == current_time {
                return Err(format!("unable to find next_event_time after current_time: {current_time}. unscheduled_operations_count: {unscheduled_operations_count}"));
            }

            if self
                .pending_operation_ids
                .get(&current_time)
                .map_or(false, |q| q.is_empty())
            {
                self.pending_operation_ids.remove(&current_time);
            }

            current_time = next_event_time;
        }

        return Ok(());
    }

    pub fn find_best_schedule_parallel(original: &Schedule) -> (Schedule, SortStrategy, Time) {
        let strategies: [SortStrategy; 10] = [SortStrategy::PriorityThenDueTime, SortStrategy::DueTimeThenPriority, SortStrategy::ShortestDurationFirst, SortStrategy::LongestDurationFirst, SortStrategy::MostSuccessorsFirst, SortStrategy::FewestSuccessorsFirst, SortStrategy::LongestSuccessorDuration, SortStrategy::ShortestSuccessorDuration, SortStrategy::EarliestDueDate, SortStrategy::LongestCriticalPathFirst];

        let mut best_schedule = original.clone();
        let mut best_execute_time = Time::MAX;
        let mut best_strategy = strategies[0];

        for &strategy in &strategies {
            let mut schedule: Schedule = original.clone();
            match schedule.compute_parallel(strategy) {
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

    // ─── Serial ───────────────────────────────────────────────────────────────

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

    fn get_eligible_operation_ids(&self) -> VecDeque<OperationId> {
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

    pub fn compute_serial(&mut self, strategy: SortStrategy) -> Result<(), String> {
        self.critical_path_data = Some(self.compute_critical_path_data()?);

        let mut eligible_operation_ids: VecDeque<OperationId> = self.get_eligible_operation_ids();
        sort_queue_by_strategy(&self.operations, &self.batches, &self.critical_path_data, &mut eligible_operation_ids, strategy);

        while let Some(operation_id) = eligible_operation_ids.pop_front() {
            let operation_earliest_start: Time = self.earliest_start(operation_id);
            let operation: &mut Operation = &mut self.operations[operation_id];
            let resource_group: &mut ResourceGroup = &mut self.resource_groups[operation.assigned_resource_group_id];

            if let Some((allocated_resource_id, work_span)) = resource_group.find_best_resource_for_operation(operation.duration, operation_earliest_start) {
                resource_group.allocate_resource(allocated_resource_id, work_span);

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
                            eligible_operation_ids.push_back(*successor_id);
                        }
                    }
                }
            } else {
                return Err(format!("Couldnt been able to allocate resource for operation: {operation_id}"));
            }

            sort_queue_by_strategy(&self.operations, &self.batches, &self.critical_path_data, &mut eligible_operation_ids, strategy);
        }

        return Ok(());
    }

    pub fn find_best_schedule_serial(original: &Schedule) -> (Schedule, SortStrategy, Time) {
        let strategies: [SortStrategy; 10] = [SortStrategy::PriorityThenDueTime, SortStrategy::DueTimeThenPriority, SortStrategy::ShortestDurationFirst, SortStrategy::LongestDurationFirst, SortStrategy::MostSuccessorsFirst, SortStrategy::FewestSuccessorsFirst, SortStrategy::LongestSuccessorDuration, SortStrategy::ShortestSuccessorDuration, SortStrategy::EarliestDueDate, SortStrategy::LongestCriticalPathFirst];

        let mut best_schedule = original.clone();
        let mut best_execute_time = Time::MAX;
        let mut best_strategy = strategies[0];

        for &strategy in &strategies {
            let mut schedule: Schedule = original.clone();
            match schedule.compute_serial(strategy) {
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

    // ─── Output ───────────────────────────────────────────────────────────────

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
                writeln!(file, "operation id: {:3} | earliest start: {:3} | start: {:4} | end: {:4} | duration: {:3} | batch id: {} | group id: {} | predecessor ids: {:?}", op.id, &self.batches[op.assigned_batch_id].start_time, span.start, span.end, op.duration, op.assigned_batch_id, op.assigned_resource_group_id, op.predecessor_ids,)?;
            } else {
                writeln!(file, "operation id: {:3} | NOT SCHEDULED", op.id)?;
            }
        }
        Ok(())
    }
}
