use std::collections::{BTreeMap, VecDeque};

use crate::{indices::{BatchId, OperationId, ResourceGroupId}, job::{Batch, Operation}, time::{Span, Time}, worker::ResourceGroup};

pub struct Schedule {
    resource_groups: Vec<ResourceGroup>,
    pub batches: Vec<Batch>,
    pub operations: Vec<Operation>
}

impl Schedule {
    pub fn new() -> Self {
        return Self { resource_groups: Vec::new(), batches: Vec::new(), operations: Vec::new() };
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
        self.operations[predecessor_id].successor_ids.push(successor_id);
        self.operations[successor_id].predecessor_ids.push(predecessor_id);
    }
    
    pub fn select_operation_by_priority_then_due_time(&self, eligible_operation_ids: &[OperationId]) -> Option<OperationId> {
        return eligible_operation_ids
            .iter()
            .min_by_key(|id: &&OperationId| {
                let operation: &Operation = &self.operations[**id];
                let batch: &Batch = &self.batches[operation.assigned_batch_id];
                return (batch.priority, batch.due_time);
            })
            .copied();
    }
    
    pub fn compute_earliest_start(&self, operation_id: OperationId) -> Time {
        let operation: &Operation = &self.operations[operation_id];
        
        let max_predecessor_end_time: Time = operation.predecessor_ids
            .iter()
            .map(|id: &OperationId| self.operations[*id].scheduled_span.map(|span: Span| span.end).unwrap_or(0))
            .max()
            .unwrap_or(0);
    
        return max_predecessor_end_time.max(self.batches[operation.assigned_batch_id].start_time);
    }
    
    pub fn is_operation_ready(&self, operation_id: OperationId, current_time: Time) -> bool {
        let operation: &Operation = &self.operations[operation_id];
        if operation.is_scheduled() {
            return false;
        }
        
        for predecessor_id in &operation.predecessor_ids {
            let predecessor: &Operation = &self.operations[*predecessor_id];
            if let Some(span) = predecessor.scheduled_span {
                if span.end > current_time {
                    return  false;
                }
            }
            else {
                return false;
            }
        }
        
        return self.batches[operation.assigned_batch_id].start_time <= current_time;
    }
    
    fn next_event_time(&self, current_completed_operation: &BTreeMap<Time, Vec<OperationId>>, pending_operations: &BTreeMap<Time, Vec<OperationId>>) -> Option<Time> {
        let next_current_completed_end_time: Option<Time> = current_completed_operation.keys().next().copied();
        let next_pending_operations_start_time: Option<Time> = pending_operations.keys().next().copied();
        
        match (next_current_completed_end_time, next_pending_operations_start_time) {
            (Some(t1), Some(t2)) => Some(t1.min(t2)),
            (Some(t), None) => Some(t),
            (None, Some(t)) => Some(t),
            (None, None) => None
        }
    }
    
    fn sort_eligible_operation_ids_queue(&self, queue: &mut VecDeque<OperationId>) {
        let mut vec: Vec<OperationId> = queue.drain(..).collect();
        vec.sort_by_key(|id: &OperationId| {
            let op: &Operation = &self.operations[*id];
            let batch: &Batch = &self.batches[op.assigned_batch_id];
            (batch.priority, batch.due_time)
        });
        queue.extend(vec);
    }
    
    fn init_eligible_operation_ids_queue_at_time(&self) -> VecDeque<OperationId> {
        let mut queue: VecDeque<OperationId> = VecDeque::new();
        for operation in &self.operations {
            if self.is_operation_ready(operation.id, 0) {
                queue.push_back(operation.id);
                continue;
            }

            // let start = self.batches[operation.assigned_batch_id].start_time;
            // if start > 0 && !operation.is_scheduled() {
            //     pending_operations.entry(start).or_default().push(operation.id);
            // }
        }
        
        self.sort_eligible_operation_ids_queue(&mut queue);
        
        return queue;
    }
    
    // TODO redo later
    fn update_eligible_operation_ids_queue_at_time(&self, current_time: Time, eligible_operation_ids_queue: &mut VecDeque<OperationId>, pending_operations: &mut BTreeMap<Time, Vec<OperationId>>) {
        if let Some(operation_ids) = pending_operations.remove(&current_time) {
            for operation_id in operation_ids {
                if self.is_operation_ready(operation_id, current_time) && !eligible_operation_ids_queue.contains(&operation_id) {
                    eligible_operation_ids_queue.push_back(operation_id);
                }
            }
        }
        self.sort_eligible_operation_ids_queue(eligible_operation_ids_queue);
    }
    
    // TODO redo later
    fn process_completions(&mut self, current_time: Time, eligible_operation_ids_queue: &mut VecDeque<OperationId>, current_completed_operations: &mut BTreeMap<Time, Vec<OperationId>>, pending_operations: &mut BTreeMap<Time, Vec<OperationId>>) {
        if let Some(completed_oprations) = current_completed_operations.remove(&current_time) {
            for operation_id in completed_oprations {
                let successor_ids = &self.operations[operation_id].successor_ids; // копируем
                for successor_id in successor_ids {
                    let successor = &self.operations[*successor_id];
                    if !successor.is_scheduled() && successor.predecessor_ids.iter().all(|id: &OperationId| self.operations[*id].is_scheduled()) {
                        let start = self.batches[successor.assigned_batch_id].start_time;
                        if start <= current_time {
                            if !eligible_operation_ids_queue.contains(successor_id) {
                                eligible_operation_ids_queue.push_back(*successor_id);
                            }
                        } else {
                            pending_operations.entry(start).or_default().push(*successor_id);
                        }
                    }
                }
            }
        }
    }

    // итеративность за счет сортироки фронта
    // генератор
    // Дообъединать массивы
    // Сравнить методы
    // Транспортировка между ресурсами
 
    pub fn compute_schedule_parallel(&mut self) -> Result<(), String> {
        let mut current_time: Time = 0;
        let mut unscheduled_operations_count: usize = self.operations.iter().filter(|op: &&Operation| !op.is_scheduled()).count();
        let mut eligible_operation_ids_queue: VecDeque<OperationId> = self.init_eligible_operation_ids_queue_at_time();
        
        let mut pending_operations: BTreeMap<Time, Vec<OperationId>> = BTreeMap::new();
        for operation in &self.operations {
            let start = self.batches[operation.assigned_batch_id].start_time;
            if start > 0 && !operation.is_scheduled() {
                pending_operations.entry(start).or_default().push(operation.id);
            }
        }
        
        let mut current_completed_operation: BTreeMap<Time, Vec<OperationId>> = Default::default();
        
        while unscheduled_operations_count > 0 {
            
            let initial_queue_len: usize = eligible_operation_ids_queue.len();
            for _ in 0..initial_queue_len {
                let operation_id: OperationId = eligible_operation_ids_queue.pop_front().unwrap();
                
                let operation: &Operation = &self.operations[operation_id];
                let resource_group: &mut ResourceGroup = &mut self.resource_groups[operation.assigned_resource_group_id];

                if let Some((allocated_resource_id, work_span)) = resource_group.allocate_best_resource_for_operation(operation.duration, current_time) {
                    if work_span.start == current_time {
                        let operation: &mut Operation = &mut self.operations[operation_id];
                        operation.assigned_resource_id = Some(allocated_resource_id);
                        operation.scheduled_span = Some(work_span);
                        
                        unscheduled_operations_count -= 1;
                        
                        current_completed_operation.entry(work_span.end).or_default().push(operation_id);
                    }
                }
                else {
                    eligible_operation_ids_queue.push_back(operation_id);
                }
            }
            
            if unscheduled_operations_count == 0 {
                break;
            }
            
            let next_time: Time = self.next_event_time(&current_completed_operation, &pending_operations).ok_or("no future event times")?;
            if next_time == current_time {
                return Err(format!("unable to find next_time after current_time: {current_time}. unscheduled_operations_count: {unscheduled_operations_count}"));
            }
            current_time = next_time;
            
            self.process_completions(current_time, &mut eligible_operation_ids_queue, &mut current_completed_operation, &mut pending_operations);
            self.update_eligible_operation_ids_queue_at_time(current_time, &mut eligible_operation_ids_queue, &mut pending_operations);
        }
        
        return Ok(());
    }
    
    pub fn get_eligible_operation_ids(&self) -> Vec<OperationId> {
        return self.operations
            .iter()
            .filter(|op: &&Operation| {
                return !op.is_scheduled() && op.predecessor_ids.iter().all(|id: &OperationId| self.operations[*id].is_scheduled());
            })
            .map(|op: &Operation| op.id)
            .collect();
    }
    
    pub fn compute_schedule(&mut self) -> Result<(), String> {
        let mut eligible_operation_ids: Vec<OperationId> = self.get_eligible_operation_ids();
        
        while !eligible_operation_ids.is_empty() {
            let operation_id: OperationId = self.select_operation_by_priority_then_due_time(&eligible_operation_ids).ok_or("No eligible operations")?;
            
            let operation_earliest_start: Time = self.compute_earliest_start(operation_id);
            let operation: &mut Operation = &mut self.operations[operation_id];
            let resource_group: &mut ResourceGroup = &mut self.resource_groups[operation.assigned_resource_group_id];
            
            if let Some((allocated_resource_id, work_span)) = resource_group.allocate_best_resource_for_operation(operation.duration, operation_earliest_start) {
                operation.assigned_resource_id = Some(allocated_resource_id);
                operation.scheduled_span = Some(work_span);
                let operation: &Operation = &self.operations[operation_id];
                
                eligible_operation_ids.retain(|id: &usize| *id != operation_id);
                
                for successor_id in &operation.successor_ids {
                    let successor: &Operation = &self.operations[*successor_id];
                    
                    if !successor.is_scheduled() && successor.predecessor_ids.iter().all(|pred: &OperationId| self.operations[*pred].is_scheduled()) {
                        if !eligible_operation_ids.contains(successor_id) {
                            eligible_operation_ids.push(*successor_id);
                        }
                    }
                }
            }
            else {
                return Err(format!("Couldnt been able to allocate resource for operation: {operation_id}"));
            }
        }
        
        return Ok(());
    }
    
    pub fn compute_total_execute_time(&self) -> Time {
        return self.operations
            .iter()
            .filter_map(|oper: &Operation| oper.scheduled_span.map(|span: Span| span.end))
            .max()
            .unwrap_or(0);
    }
    
    pub fn print_schedule(&self, schedule_name: &str) {
        println!("schedule ({schedule_name}):");
        
        for operation in &self.operations {
            if let Some(span) = operation.scheduled_span {
                println!("operation {}: [{}, {})", operation.id, span.start, span.end);
            }
            else {
                println!("operation {}: not assinged", operation.id);
            }
        }
        
        println!("total execute time: {}", self.compute_total_execute_time());
    }
}