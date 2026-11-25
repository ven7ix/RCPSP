use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};
use std::io::Write;

use rand::{RngExt, SeedableRng, rngs::StdRng};

use crate::indices::*;
use crate::job::*;
use crate::time::*;
use crate::worker::*;

// ─── Strategies ───────────────────────────────────────────────────────────────

/// Ключ сортировки операции: меньше — планируется раньше.
type PriorityKey = (i64, i64);

/// Правило приоритета: какую из доступных операций планировать первой.
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
    const ALL: [SortStrategy; 11] = [SortStrategy::PriorityThenDueTime, SortStrategy::DueTimeThenPriority, SortStrategy::ShortestDurationFirst, SortStrategy::LongestDurationFirst, SortStrategy::MostSuccessorsFirst, SortStrategy::FewestSuccessorsFirst, SortStrategy::LongestSuccessorDuration, SortStrategy::ShortestSuccessorDuration, SortStrategy::EarliestDueDate, SortStrategy::LongestCriticalPathFirst, SortStrategy::Random];

    pub fn all() -> &'static [SortStrategy] {
        &Self::ALL
    }
}

/// Схема построения расписания.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algorithm {
    /// Последовательная: операции выбираются по одной в порядке приоритета и ставятся
    /// на самое раннее возможное время (в том числе в «окна» между уже поставленными).
    Serial,
    /// Параллельная: время движется вперёд, в каждый момент свободные ресурсы
    /// отдаются доступным операциям по приоритету.
    Parallel,
}

impl Algorithm {
    pub const ALL: [Algorithm; 2] = [Algorithm::Serial, Algorithm::Parallel];
}

/// Сколько двусторонних проходов `find_best` и лидерборд делают по умолчанию, пока есть улучшение.
pub const DEFAULT_IMPROVE_ROUNDS: usize = 10;

// ─── Critical path ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CriticalPathData {
    pub earliest_start: Vec<Time>,
    pub earliest_finish: Vec<Time>,
    pub latest_start: Vec<Time>,
    pub latest_finish: Vec<Time>,
    pub float: Vec<Time>,        // резерв времени (0 = на критическом пути)
    pub path_lengths: Vec<Time>, // длина пути от узла до листа (для сортировки)
}

// ─── Schedule ─────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct Schedule {
    resource_groups: Vec<ResourceGroup>,
    pub batches: Vec<Batch>,
    pub operations: Vec<Operation>,
    /// Зерно для стратегии `Random`: одно и то же зерно даёт одно и то же расписание.
    pub random_seed: u64,
}

impl Schedule {
    pub fn new() -> Self {
        Self::default()
    }

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

    // ─── Critical path ────────────────────────────────────────────────────────

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

    // ─── Priorities ───────────────────────────────────────────────────────────

    /// Ключ приоритета каждой операции по выбранной стратегии: чем меньше, тем раньше операция планируется.
    /// Все правила статические, поэтому ключи считаются один раз. При равных ключах побеждает та операция,
    /// которая раньше стала доступной (FIFO) — это перемешивает цепочки, а не планирует их по одной.
    fn priority_keys(&self, strategy: SortStrategy) -> Result<Vec<PriorityKey>, String> {
        let path_lengths = match strategy {
            SortStrategy::LongestCriticalPathFirst => self.compute_critical_path_data()?.path_lengths,
            _ => Vec::new(),
        };
        let mut rng = StdRng::seed_from_u64(self.random_seed);

        let keys = self
            .operations
            .iter()
            .enumerate()
            .map(|(id, op)| {
                let batch = &self.batches[op.assigned_batch_id];
                let (priority, due) = (batch.priority as i64, i64::from(batch.due_time));
                let successors_duration: i64 = op
                    .successor_ids
                    .iter()
                    .map(|&s| i64::from(self.operations[s].duration))
                    .sum();
                let successors_count = op.successor_ids.len() as i64;
                let duration = i64::from(op.duration);

                match strategy {
                    SortStrategy::PriorityThenDueTime => (priority, due),
                    SortStrategy::DueTimeThenPriority => (due, priority),
                    SortStrategy::ShortestDurationFirst => (duration, 0),
                    SortStrategy::LongestDurationFirst => (-duration, 0),
                    SortStrategy::MostSuccessorsFirst => (-successors_count, 0),
                    SortStrategy::FewestSuccessorsFirst => (successors_count, 0),
                    SortStrategy::LongestSuccessorDuration => (-successors_duration, 0),
                    SortStrategy::ShortestSuccessorDuration => (successors_duration, 0),
                    SortStrategy::EarliestDueDate => (due, 0),
                    SortStrategy::LongestCriticalPathFirst => (-i64::from(path_lengths[id]), 0),
                    SortStrategy::Random => (i64::from(rng.random::<u32>()), 0),
                }
            })
            .collect();
        Ok(keys)
    }

    // ─── Common scheduling steps ──────────────────────────────────────────────

    /// Сколько у каждой операции ещё не поставленных предшественников.
    fn predecessor_counts(&self) -> Vec<usize> {
        self.operations
            .iter()
            .map(|op| op.predecessor_ids.len())
            .collect()
    }

    /// Раньше этого момента операцию начинать нельзя: старт партии и окончание всех предшественников.
    fn ready_time(&self, id: OperationId) -> Time {
        let op = &self.operations[id];
        op.predecessor_ids
            .iter()
            .filter_map(|&p| self.operations[p].scheduled_span)
            .map(|span| span.end)
            .fold(self.batches[op.assigned_batch_id].start_time, Time::max)
    }

    /// Лучший ресурс группы операции и интервал на нём, начинающийся не раньше `after`.
    fn find_slot(&self, id: OperationId, after: Time) -> Result<(ResourceId, Span), String> {
        let op = &self.operations[id];
        self.resource_groups[op.assigned_resource_group_id]
            .find_best_resource_for_operation(op.duration, after)
            .ok_or_else(|| format!("resource group {} cannot allocate operation {id} at all", op.assigned_resource_group_id))
    }

    fn commit(&mut self, id: OperationId, resource_id: ResourceId, span: Span) {
        let op = &mut self.operations[id];
        self.resource_groups[op.assigned_resource_group_id].allocate_resource(resource_id, span);
        op.assigned_resource_id = Some(resource_id);
        op.scheduled_span = Some(span);
    }

    /// Сбрасывает результаты предыдущего расчёта, чтобы расписание можно было пересчитать.
    fn reset(&mut self) {
        self.resource_groups
            .iter_mut()
            .for_each(ResourceGroup::clear);
        for op in &mut self.operations {
            op.assigned_resource_id = None;
            op.scheduled_span = None;
        }
    }

    fn ensure_all_scheduled(&self, scheduled: usize) -> Result<(), String> {
        let total = self.operations.len();
        if scheduled == total { Ok(()) } else { Err(format!("only {scheduled} of {total} operations were scheduled (cycle in precedence graph?)")) }
    }

    // ─── Serial ───────────────────────────────────────────────────────────────

    fn schedule_serial(&mut self, keys: &[PriorityKey]) -> Result<(), String> {
        let mut remaining = self.predecessor_counts();
        // Доступные операции: (ключ, порядковый номер появления, id) — номер даёт FIFO при равных ключах.
        let mut eligible: BinaryHeap<Reverse<(PriorityKey, usize, OperationId)>> = BinaryHeap::new();
        let mut arrivals = 0;
        for id in (0..remaining.len()).filter(|&id| remaining[id] == 0) {
            eligible.push(Reverse((keys[id], arrivals, id)));
            arrivals += 1;
        }
        let mut scheduled = 0;

        while let Some(Reverse((_, _, id))) = eligible.pop() {
            let (resource_id, span) = self.find_slot(id, self.ready_time(id))?;
            self.commit(id, resource_id, span);
            scheduled += 1;

            for &successor in &self.operations[id].successor_ids {
                remaining[successor] -= 1;
                if remaining[successor] == 0 {
                    eligible.push(Reverse((keys[successor], arrivals, successor)));
                    arrivals += 1;
                }
            }
        }

        self.ensure_all_scheduled(scheduled)
    }

    // ─── Parallel ─────────────────────────────────────────────────────────────

    fn schedule_parallel(&mut self, keys: &[PriorityKey]) -> Result<(), String> {
        let mut remaining = self.predecessor_counts();
        let mut released = vec![false; remaining.len()];
        // Время, когда операция может начаться → операции, ожидающие этого момента (в порядке появления).
        let mut pending: BTreeMap<Time, Vec<OperationId>> = BTreeMap::new();
        for id in (0..remaining.len()).filter(|&id| remaining[id] == 0) {
            released[id] = true;
            pending
                .entry(self.ready_time(id))
                .or_default()
                .push(id);
        }
        let mut scheduled = 0;

        while let Some((time, mut queue)) = pending.pop_first() {
            queue.sort_by_key(|&id| keys[id]); // сортировка устойчивая: при равных ключах порядок появления сохраняется

            let mut completed = Vec::new();
            for id in queue {
                let (resource_id, span) = self.find_slot(id, time)?;
                if span.start > time {
                    // Ресурсы заняты: операция ждёт, пока освободится самый ранний из них.
                    pending.entry(span.start).or_default().push(id);
                    continue;
                }

                self.commit(id, resource_id, span);
                completed.push(id);
                for &successor in &self.operations[id].successor_ids {
                    remaining[successor] -= 1;
                }
            }
            scheduled += completed.len();

            // Последователи становятся доступными, когда поставлены все их предшественники.
            // Если операция нулевой длины, она попадёт в очередь на тот же `time` и будет обработана следующей.
            for id in completed {
                for &successor in &self.operations[id].successor_ids {
                    if remaining[successor] == 0 && !released[successor] {
                        released[successor] = true;
                        pending
                            .entry(self.ready_time(successor))
                            .or_default()
                            .push(successor);
                    }
                }
            }
        }

        self.ensure_all_scheduled(scheduled)
    }

    // ─── Improvement (forward-backward) ───────────────────────────────────────

    /// Зеркальная задача: связи развёрнуты, времена готовности партий сброшены в 0.
    /// Расписание в ней, прочитанное с конца, — это «сдвиг всех операций как можно позже».
    fn mirrored(&self) -> Schedule {
        let mut mirror = self.clone();
        mirror.reset();
        for op in &mut mirror.operations {
            std::mem::swap(&mut op.predecessor_ids, &mut op.successor_ids);
        }
        for batch in &mut mirror.batches {
            batch.start_time = 0;
        }
        mirror
    }

    /// Один двусторонний проход: возвращает новое корректное расписание (не обязательно лучшее).
    ///
    /// 1. Назад: операции по убыванию окончания строятся тем же алгоритмом в зеркальной задаче,
    ///    так получаются самые поздние старты при неизменной длительности.
    /// 2. Вперёд: порядок самых поздних стартов становится приоритетом, и расписание
    ///    строится заново тем же алгоритмом с самым ранним временем начала.
    fn justified(&self, algorithm: Algorithm) -> Result<Schedule, String> {
        let span = |op: &Operation| {
            op.scheduled_span
                .expect("justified is called on a computed schedule")
        };
        let horizon = i64::from(self.total_execute_time());

        let backward_keys: Vec<PriorityKey> = self
            .operations
            .iter()
            .map(|op| (-i64::from(span(op).end), -i64::from(span(op).start)))
            .collect();
        let mut mirror = self.mirrored();
        mirror.run(algorithm, &backward_keys)?;

        let forward_keys: Vec<PriorityKey> = self
            .operations
            .iter()
            .zip(&mirror.operations)
            .map(|(op, mirrored_op)| {
                // В обратном времени операция занимает [s, e), в исходном — [horizon - e, horizon - s).
                let latest_start = horizon - i64::from(span(mirrored_op).end);
                (latest_start, i64::from(span(op).start))
            })
            .collect();
        let mut candidate = self.clone();
        candidate.run(algorithm, &forward_keys)?;
        Ok(candidate)
    }

    /// Улучшает уже построенное расписание повторными двусторонними проходами, пока длительность
    /// уменьшается, но не более `max_rounds` раз. Принимаются только строго лучшие результаты,
    /// поэтому длительность никогда не растёт. Возвращает число принятых проходов.
    pub fn improve(&mut self, algorithm: Algorithm, max_rounds: usize) -> Result<usize, String> {
        if self
            .operations
            .iter()
            .any(|op| !op.is_scheduled())
        {
            return Err("schedule must be computed before improving it".to_string());
        }

        let mut accepted = 0;
        while accepted < max_rounds {
            let candidate = self.justified(algorithm)?;
            if candidate.total_execute_time() >= self.total_execute_time() {
                break;
            }
            *self = candidate;
            accepted += 1;
        }
        Ok(accepted)
    }

    // ─── Public API ───────────────────────────────────────────────────────────

    /// Строит расписание по готовым ключам приоритета (прошлый результат сбрасывается).
    fn run(&mut self, algorithm: Algorithm, keys: &[PriorityKey]) -> Result<(), String> {
        self.reset();
        match algorithm {
            Algorithm::Serial => self.schedule_serial(keys),
            Algorithm::Parallel => self.schedule_parallel(keys),
        }
    }

    /// Строит расписание выбранным алгоритмом и правилом приоритета (прошлый результат сбрасывается).
    pub fn compute(&mut self, algorithm: Algorithm, strategy: SortStrategy) -> Result<(), String> {
        let keys = self.priority_keys(strategy)?;
        self.run(algorithm, &keys)
    }

    /// То же, что `compute`, но затем расписание улучшается двусторонним проходом (см. `improve`).
    pub fn compute_improved(&mut self, algorithm: Algorithm, strategy: SortStrategy, max_rounds: usize) -> Result<(), String> {
        self.compute(algorithm, strategy)?;
        self.improve(algorithm, max_rounds).map(|_| ())
    }

    pub fn compute_serial(&mut self, strategy: SortStrategy) -> Result<(), String> {
        self.compute(Algorithm::Serial, strategy)
    }

    pub fn compute_parallel(&mut self, strategy: SortStrategy) -> Result<(), String> {
        self.compute(Algorithm::Parallel, strategy)
    }

    /// Перебирает все стратегии и возвращает расписание с наименьшей длительностью.
    /// `None`, если ни одна стратегия не смогла построить расписание.
    /// При `improve_rounds > 0` каждое расписание дополнительно улучшается двусторонним проходом.
    pub fn find_best(original: &Schedule, algorithm: Algorithm, improve_rounds: usize) -> Option<(Schedule, SortStrategy)> {
        SortStrategy::all()
            .iter()
            .filter_map(|&strategy| {
                let mut schedule = original.clone();
                match schedule.compute_improved(algorithm, strategy, improve_rounds) {
                    Ok(()) => Some((schedule, strategy)),
                    Err(e) => {
                        eprintln!("{algorithm:?} with strategy {strategy:?} failed: {e}");
                        None
                    }
                }
            })
            .min_by_key(|(schedule, _)| schedule.total_execute_time())
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
