use crate::indices::*;
use crate::job::*;
use crate::schedule::Schedule;
use crate::time::*;
use crate::worker::*;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Deserialize, Serialize)]
pub struct GenerationConfig {
    pub resource_group_sizes: Vec<usize>,
    pub operation_chains_count: usize,
    pub min_opearions_per_chain: usize,
    pub max_opearions_per_chain: usize,
    pub min_operation_duration: Time,
    pub max_operation_duration: Time,
    pub batch_count: usize,
    pub max_batch_priority: usize,
    pub max_batch_start_time: Time,
    pub batch_due_time_factor: f64,
    pub extra_precedence_probability: f64,
    pub seed_count: u64,
    pub random_seed: u64,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            resource_group_sizes: vec![1],
            operation_chains_count: 1,
            min_opearions_per_chain: 3,
            max_opearions_per_chain: 3,
            min_operation_duration: 1,
            max_operation_duration: 5,
            batch_count: 1,
            max_batch_priority: 0,
            max_batch_start_time: 0,
            batch_due_time_factor: 1.0,
            extra_precedence_probability: 0.0,
            seed_count: 10,
            random_seed: 0,
        }
    }
}

pub fn load_config_from_json(path: &Path) -> Result<GenerationConfig, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let config = serde_json::from_reader(reader)?;
    return Ok(config);
}

pub fn generate_schedule(config: &GenerationConfig) -> Schedule {
    let mut rng = StdRng::seed_from_u64(config.random_seed);
    let mut schedule = Schedule::new();

    // resource groups
    let mut resource_group_ids = Vec::new();
    for &size in &config.resource_group_sizes {
        let rg_id = schedule.add_resource_group(ResourceGroup::new(resource_group_ids.len(), size));
        resource_group_ids.push(rg_id);
    }

    // batches
    for batch in 0..config.batch_count {
        let start = rng.random_range(0..=config.max_batch_start_time);
        let priority = rng.random_range(0..=config.max_batch_priority);
        let batch = Batch::new(batch, start, 0, priority);
        schedule.add_batch(batch);
    }

    // operations
    let mut job_operations: Vec<Vec<OperationId>> = Vec::new();
    let mut global_order: Vec<OperationId> = Vec::new();

    let min_dur = config.min_operation_duration.max(1);
    let max_dur = config.max_operation_duration.max(min_dur);

    for _ in 0..config.operation_chains_count {
        let num_ops = rng.random_range(config.min_opearions_per_chain..=config.max_opearions_per_chain);
        let mut ops_in_job = Vec::new();

        for i in 0..num_ops {
            let duration = rng.random_range(min_dur..=max_dur);

            // случайная группа вместо детерминированного i % len —
            // создаёт непредсказуемое распределение нагрузки по ресурсам
            let rg_id = if i == 0 || rng.random_range(0u32..100) < 60 {
                // 60% шанс — случайная группа
                resource_group_ids[rng.random_range(0..resource_group_ids.len())]
            } else {
                // 40% шанс — та же группа что и предыдущая операция в цепочке,
                // создаёт локальные узкие места внутри цепочки
                let prev_op_id: OperationId = ops_in_job[i - 1];
                schedule.operations[prev_op_id].assigned_resource_group_id
            };

            let op = Operation::new(schedule.operations.len(), duration, 0, rg_id);
            let op_id = schedule.add_operation(op);
            ops_in_job.push(op_id);
            global_order.push(op_id);

            if i > 0 {
                schedule.add_precedence(ops_in_job[i - 1], op_id);
            }
        }

        // ветвление: иногда добавляем вторую входящую зависимость для последней операции
        // от случайной предыдущей цепочки — создаёт слияния в графе
        if !job_operations.is_empty() && rng.random_range(0u32..100) < 40 {
            let prev_job = &job_operations[rng.random_range(0..job_operations.len())];
            let from_op = prev_job[rng.random_range(0..prev_job.len())];
            let to_op = *ops_in_job.last().unwrap();
            // проверяем что не добавляем дублирующую связь
            if !schedule.operations[from_op]
                .successor_ids
                .contains(&to_op)
            {
                schedule.add_precedence(from_op, to_op);
            }
        }

        job_operations.push(ops_in_job);
    }

    // operation -> batch distribution
    // операции из одной цепочки могут попасть в разные батчи —
    // это создаёт конфликты приоритетов внутри одного пути зависимостей
    let mut batch_operation_ids: Vec<Vec<OperationId>> = vec![Vec::new(); config.batch_count];
    for job_ops in &job_operations {
        // голова цепочки — случайный батч
        let head_batch = rng.random_range(0..config.batch_count);
        for (i, &op_id) in job_ops.iter().enumerate() {
            // хвост цепочки иногда попадает в другой батч
            let batch_id = if i == job_ops.len() - 1 && rng.random_range(0u32..100) < 30 { rng.random_range(0..config.batch_count) } else { head_batch };
            schedule.operations[op_id].assigned_batch_id = batch_id;
            batch_operation_ids[batch_id].push(op_id);
        }
    }

    // batches due times
    for (batch_id, op_ids) in batch_operation_ids.iter().enumerate() {
        let total_duration: Time = op_ids
            .iter()
            .map(|&op_id| schedule.operations[op_id].duration)
            .sum();
        let slack = (total_duration as f64 * config.batch_due_time_factor) as Time;
        let due = schedule.batches[batch_id].start_time + slack;
        schedule.batches[batch_id].due_time = due;
        schedule.batches[batch_id].operation_ids = op_ids.clone();
    }

    // extra precedences: случайные дуги вперёд по global_order
    // создают дополнительные зависимости между несвязанными цепочками
    let num_ops = global_order.len();
    let max_extra = (num_ops as f64 * config.extra_precedence_probability) as usize;
    for _ in 0..max_extra {
        let a_idx = rng.random_range(0..num_ops.saturating_sub(1));
        let b_idx = rng.random_range(a_idx + 1..num_ops);
        let op_a = global_order[a_idx];
        let op_b = global_order[b_idx];
        // избегаем дублей
        if !schedule.operations[op_a]
            .successor_ids
            .contains(&op_b)
        {
            schedule.add_precedence(op_a, op_b);
        }
    }

    return schedule;
}
