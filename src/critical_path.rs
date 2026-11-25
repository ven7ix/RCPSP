use std::collections::VecDeque;

use crate::indices::OperationId;
use crate::schedule::Schedule;
use crate::time::Time;

#[derive(Clone)]
pub struct CriticalPathData {
    pub earliest_start: Vec<Time>,
    pub earliest_finish: Vec<Time>,
    pub latest_start: Vec<Time>,
    pub latest_finish: Vec<Time>,
    pub float: Vec<Time>,        // резерв времени (0 = на критическом пути)
    pub path_lengths: Vec<Time>, // длина пути от узла до листа (для сортировки)
}

impl Schedule {
    /// Порядок Кана. Возвращает `Err`, если в графе есть цикл.
    fn topological_order(&self) -> Result<Vec<OperationId>, String> {
        let mut remaining = self.predecessor_counts();
        let mut queue: VecDeque<OperationId> = (0..remaining.len())
            .filter(|&id| remaining[id] == 0)
            .collect();
        let mut order = Vec::with_capacity(remaining.len());

        while let Some(id) = queue.pop_front() {
            order.push(id);
            for &successor in &self.operations[id].successor_ids {
                remaining[successor] -= 1;
                if remaining[successor] == 0 {
                    queue.push_back(successor);
                }
            }
        }

        if order.len() == remaining.len() { Ok(order) } else { Err("cycle detected in precedence graph".to_string()) }
    }

    pub fn compute_critical_path_data(&self) -> Result<CriticalPathData, String> {
        let n = self.operations.len();
        let order = self.topological_order()?;

        // Прямой проход: ES, EF.
        let mut es = vec![0; n];
        let mut ef = vec![0; n];
        for &id in &order {
            let op = &self.operations[id];
            let batch_start = self.batches[op.assigned_batch_id].start_time;
            es[id] = op
                .predecessor_ids
                .iter()
                .map(|&p| ef[p])
                .fold(batch_start, Time::max);
            ef[id] = es[id] + op.duration;
        }
        let project_end = ef.iter().copied().max().unwrap_or(0);

        // Обратный проход: LF, LS и длина хвоста пути.
        let mut lf = vec![project_end; n];
        let mut ls = vec![project_end; n];
        let mut path_lengths = vec![0; n];
        for &id in order.iter().rev() {
            let op = &self.operations[id];
            lf[id] = op
                .successor_ids
                .iter()
                .map(|&s| ls[s])
                .fold(project_end, Time::min);
            ls[id] = lf[id] - op.duration;
            path_lengths[id] = op.duration
                + op.successor_ids
                    .iter()
                    .map(|&s| path_lengths[s])
                    .max()
                    .unwrap_or(0);
        }

        let float = (0..n).map(|i| ls[i] - es[i]).collect();
        Ok(CriticalPathData { earliest_start: es, earliest_finish: ef, latest_start: ls, latest_finish: lf, float, path_lengths })
    }
}
