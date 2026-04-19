use crate::{indices::{BatchId, OperationId, ResourceGroupId, ResourceId}, time::{Span, Time}};

pub struct Operation {
    pub id: OperationId,
    pub duration: Time,
    pub assigned_batch_id: BatchId,
    pub assigned_resource_group_id: ResourceGroupId,
    pub predecessor_ids: Vec<OperationId>,
    pub successor_ids: Vec<OperationId>,
    pub assigned_resource_id: Option<ResourceId>,
    pub scheduled_span: Option<Span>
}

impl Operation {
    pub fn new(id: usize, duration: Time, assigned_batch_id: BatchId, assigned_resource_group_id: ResourceGroupId) -> Self {
        return Self {
            id: id,
            duration: duration,
            assigned_batch_id: assigned_batch_id,
            assigned_resource_group_id: assigned_resource_group_id,
            predecessor_ids: Vec::new(),
            successor_ids: Vec::new(),
            assigned_resource_id: None,
            scheduled_span: None
        };
    }
    
    pub fn is_scheduled(&self) -> bool {
        return self.scheduled_span.is_some();
    }
}

pub struct Batch {
    pub id: BatchId,
    pub operation_ids: Vec<OperationId>,
    pub start_time: Time,
    pub due_time: Time,
    pub priority: usize
}

impl Batch {
    pub fn new(id: BatchId, start_time: Time, due_time: Time, priority: usize) -> Self {
        return Self {
            id: id,
            operation_ids: Vec::new(),
            start_time: start_time,
            due_time: due_time,
            priority: priority
        };
    }
}