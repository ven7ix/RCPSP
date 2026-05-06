use std::collections::{BTreeMap, VecDeque};

use crate::indices::*;
use crate::job::*;
use crate::time::*;
use crate::worker::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortStrategy {
    PriorityThenDueTime,
    DueTimeThenPriority,
    ShortestDurationFirst,
    LongestDurationFirst
}

pub struct Schedule {
    resource_groups: Vec<ResourceGroup>,
    pub batches: Vec<Batch>,
    pub operations: Vec<Operation>,
    pending_operation_ids: BTreeMap<Time, VecDeque<OperationId>>
}

impl Schedule {
    pub fn new() -> Self {
        return Self { resource_groups: Vec::new(), batches: Vec::new(), operations: Vec::new(), pending_operation_ids: BTreeMap::new() };
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
    
    fn init_pending_opeations(&mut self) {
        for operation in &self.operations {
            if operation.predecessor_ids.len() > 0 {
                continue;
            }
            
            let start_time: Time = self.batches[operation.assigned_batch_id].start_time;
            self.pending_operation_ids.entry(start_time).or_default().push_back(operation.id);
        }
        
        for queue in self.pending_operation_ids.values_mut() {
            let mut vec: Vec<OperationId> = queue.drain(..).collect();
            vec.sort_by_key(|id: &OperationId| {
                let op: &Operation = &self.operations[*id];
                let batch: &Batch = &self.batches[op.assigned_batch_id];
                (batch.priority, batch.due_time)
            });
            queue.extend(vec);
        }
    }
    
    fn find_next_event_time(&self, current_time: Time) -> Option<Time> {
        // let mut next_event_time: Option<Time> = self.pending_operation_ids.keys().next().copied();
        let mut next_event_time: Option<Time> = None;
        
        for resource_group in &self.resource_groups {
            for resource in &resource_group.resources {
                for span in &resource.allocations {
                    if span.end <= current_time {
                        continue;
                    }
                    
                    match next_event_time {
                        None => next_event_time = Some(span.end),
                        Some(time) if span.end < time => next_event_time = Some(span.end),
                        _ => {}
                    }
                }
            }
        }
        
        if let Some((&time, _)) = self.pending_operation_ids.range((current_time + 1)..).next() {
            match next_event_time {
                None => next_event_time = Some(time),
                Some(existing) if time < existing => next_event_time = Some(time),
                _ => {}
            }
        }
        
        return next_event_time;
    }
    
    fn sort_queue_by_strategy(&self, queue: &mut VecDeque<OperationId>, strategy: SortStrategy) {
        let mut vec: Vec<OperationId> = queue.drain(..).collect();
        
        match strategy {
            SortStrategy::PriorityThenDueTime => {
                vec.sort_by_key(|id: &OperationId| {
                    let op: &Operation = &self.operations[*id];
                    let batch: &Batch = &self.batches[op.assigned_batch_id];
                    (batch.priority, batch.due_time)
                });
            }
            SortStrategy::DueTimeThenPriority => {
                vec.sort_by_key(|id: &OperationId| {
                    let op: &Operation = &self.operations[*id];
                    let batch: &Batch = &self.batches[op.assigned_batch_id];
                    (batch.due_time, batch.priority)
                });
            }
            SortStrategy::ShortestDurationFirst => {
                vec.sort_by_key(|id: &OperationId| {
                    self.operations[*id].duration
                });
            }
            SortStrategy::LongestDurationFirst => {
                vec.sort_by_key(|id: &OperationId| {
                    std::cmp::Reverse(self.operations[*id].duration)
                });
            }
        }
        
        queue.extend(vec);
    }
    
    fn add_successors_to_pending_operations(&mut self, current_time: Time, current_completed_operation_ids: Vec<OperationId>) {
        for operation_id in current_completed_operation_ids {
            let operation = &self.operations[operation_id];
            let successor_ids = &operation.successor_ids;
            for successor_id in successor_ids {
                let successor = &self.operations[*successor_id];
                if successor.is_scheduled() || successor.predecessor_ids.iter().any(|id: &OperationId| !self.operations[*id].is_scheduled()) {
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
                
                let mut start_time: Time = self.batches[successor.assigned_batch_id].start_time.max(max_predecessor_end);
                if start_time < current_time {
                    start_time = current_time;
                }
                
                let queue = self.pending_operation_ids.entry(start_time).or_default();
                
                if !queue.contains(successor_id) {
                    queue.push_back(*successor_id);
                    
                    for queue in self.pending_operation_ids.values_mut() {
                        let mut vec: Vec<OperationId> = queue.drain(..).collect();
                        vec.sort_by_key(|id: &OperationId| {
                            let op: &Operation = &self.operations[*id];
                            let batch: &Batch = &self.batches[op.assigned_batch_id];
                            (batch.priority, batch.due_time)
                        });
                        queue.extend(vec);
                    }
                }
            }
        }
    }
    
    fn update_pending_operation_ids(&mut self, current_time: Time, next_event_time: Time) {
        if let Some(current_time_queue) = self.pending_operation_ids.remove(&current_time) {
            self.pending_operation_ids.entry(next_event_time).or_default().extend(current_time_queue.iter());
        }
    }
    
    // итеративность за счет сортировки фронта
    // генератор
    // сравнить методы
    // транспортировка между ресурсами ?
    
    pub fn compute_schedule_parallel(&mut self) -> Result<(), String> {
        self.init_pending_opeations();
        let mut current_time: Time = 0;
        let mut unscheduled_operations_count: usize = self.operations.len();
        
        while unscheduled_operations_count > 0 {
            let mut current_completed_operation_ids: Vec<OperationId> = Default::default();
            
            if let Some(current_time_queue) = self.pending_operation_ids.get_mut(&current_time) {
                let initial_queue_len: usize = current_time_queue.len();
                for _ in 0..initial_queue_len {
                    let operation_id: OperationId = current_time_queue.pop_front().unwrap();
                    
                    let operation: &Operation = &self.operations[operation_id];
                    let resource_group: &mut ResourceGroup = &mut self.resource_groups[operation.assigned_resource_group_id];

                    if let Some((allocated_resource_id, work_span)) = resource_group.allocate_best_resource_for_operation(operation.duration, current_time) {
                        if work_span.start == current_time {
                            let operation: &mut Operation = &mut self.operations[operation_id];
                            operation.assigned_resource_id = Some(allocated_resource_id);
                            operation.scheduled_span = Some(work_span);
                            
                            unscheduled_operations_count -= 1;
                            
                            current_completed_operation_ids.push(operation_id);
                        }
                        else {
                            current_time_queue.push_back(operation_id);
                        }
                    }
                    else {
                        current_time_queue.push_back(operation_id);
                    }
                }
            }
            
            if unscheduled_operations_count == 0 {
                break;
            }
            
            let next_event_time: Time = self.find_next_event_time(current_time).ok_or("no future event times")?;
            if next_event_time == current_time {
                return Err(format!("unable to find next_event_time after current_time: {current_time}. unscheduled_operations_count: {unscheduled_operations_count}"));
            }
            
            self.add_successors_to_pending_operations(current_time, current_completed_operation_ids);
            self.update_pending_operation_ids(current_time, next_event_time);
            
            for queue in self.pending_operation_ids.values_mut() {
                let mut vec: Vec<OperationId> = queue.drain(..).collect();
                vec.sort_by_key(|id: &OperationId| {
                    let op: &Operation = &self.operations[*id];
                    let batch: &Batch = &self.batches[op.assigned_batch_id];
                    (batch.priority, batch.due_time)
                });
                queue.extend(vec);
            }
            
            current_time = next_event_time;
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