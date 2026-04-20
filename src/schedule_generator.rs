use rand::{rng, RngExt};
use crate::{
    schedule::Schedule,
    job::{Batch, Operation},
    worker::ResourceGroup,
};

pub struct GenerationConfig {
    pub batches_count: usize,
    pub ops_per_batch_min: usize,
    pub ops_per_batch_max: usize,
    pub num_resource_groups: usize,
    pub resources_per_group: Vec<usize>,
    pub duration_min: u32,
    pub duration_max: u32,
    pub start_time_max: u32,
    pub due_time_factor: f64,
    pub priority_min: usize,
    pub priority_max: usize
}

impl GenerationConfig {
    pub fn new() -> Self {
        Self {
            batches_count: 3,
            ops_per_batch_min: 20000,
            ops_per_batch_max: 50000,
            num_resource_groups: 2,
            resources_per_group: vec![2, 2],
            duration_min: 1,
            duration_max: 10,
            start_time_max: 0,
            due_time_factor: 2.0,
            priority_min: 1,
            priority_max: 5,
        }
    }
}

pub fn generate_random_schedule(config: &GenerationConfig) -> Schedule {
    let mut rng = rng();
    let mut schedule = Schedule::new();

    // группы ресурсов
    assert_eq!(config.resources_per_group.len(), config.num_resource_groups);
    for (i, res_count) in config.resources_per_group.iter().enumerate() {
        schedule.add_resource_group(ResourceGroup::new(i, *res_count));
    }

    let mut next_op_id = 0;

    // партии
    for batch_id in 0..config.batches_count {
        // параметры партии
        let start_time = if config.start_time_max > 0 {
            rng.random_range(0..=config.start_time_max)
        } else {
            0
        };
        let priority = rng.random_range(config.priority_min..=config.priority_max);
        
        let batch = Batch::new(batch_id, start_time, 0, priority);
        let batch_id = schedule.add_batch(batch);

        let num_ops = rng.random_range(config.ops_per_batch_min..=config.ops_per_batch_max);
        let mut op_ids_in_batch = Vec::with_capacity(num_ops);

        // операции
        for _ in 0..num_ops {
            let duration = rng.random_range(config.duration_min..=config.duration_max);
            let resource_group_id = rng.random_range(0..config.num_resource_groups);
            let op = Operation::new(next_op_id, duration, batch_id, resource_group_id);
            let op_id = schedule.add_operation(op);
            op_ids_in_batch.push(op_id);
            next_op_id += 1;
        }

        // линейный путь
        for i in 0..op_ids_in_batch.len().saturating_sub(1) {
            let pred = op_ids_in_batch[i];
            let succ = op_ids_in_batch[i + 1];
            schedule.add_precedence(pred, succ);
        }

        // суммарная длительность (для due_time)
        let total_duration: u32 = op_ids_in_batch.iter()
            .map(|&id| schedule.operations[id].duration)
            .sum();
        let due_time = (total_duration as f64 * config.due_time_factor).ceil() as u32 + start_time;
        
        schedule.batches[batch_id].due_time = due_time;
    }

    return schedule;
}