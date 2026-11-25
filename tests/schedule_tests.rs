use rcpsp::job::*;
use rcpsp::priority::*;
use rcpsp::schedule::*;
use rcpsp::schedule_generator::*;
use rcpsp::schedulers::{self, ParallelScheduler, SerialScheduler};
use rcpsp::solver::*;
use rcpsp::time::*;
use rcpsp::worker::*;
use std::collections::HashMap;

/// Собирает расписание вручную: `ops` — (длительность, партия, группа ресурсов), `edges` — (предшественник, последователь).
fn build(group_sizes: &[usize], batch_starts: &[Time], ops: &[(Time, usize, usize)], edges: &[(usize, usize)]) -> Schedule {
    let mut schedule = Schedule::new();
    for (id, &size) in group_sizes.iter().enumerate() {
        schedule.add_resource_group(ResourceGroup::new(id, size));
    }
    for (id, &start) in batch_starts.iter().enumerate() {
        schedule.add_batch(Batch::new(id, start, 0, 0));
    }
    for (id, &(duration, batch, group)) in ops.iter().enumerate() {
        schedule.add_operation(Operation::new(id, duration, batch, group));
    }
    for &(from, to) in edges {
        schedule.add_precedence(from, to);
    }
    schedule
}

/// Проверяет корректность готового расписания:
/// все операции размещены, длительность совпадает, соблюдены старты партий,
/// предшественники и отсутствие пересечений на одном ресурсе.
fn assert_valid(schedule: &Schedule, context: &str) {
    let mut per_resource: HashMap<(usize, usize), Vec<(u32, u32)>> = HashMap::new();

    for op in &schedule.operations {
        let span = op
            .scheduled_span
            .unwrap_or_else(|| panic!("{context}: operation {} is not scheduled", op.id));
        assert_eq!(span.end - span.start, op.duration, "{context}: wrong span length for operation {}", op.id);
        assert!(span.start >= schedule.batches[op.assigned_batch_id].start_time, "{context}: operation {} starts before its batch", op.id);

        for &pred in &op.predecessor_ids {
            let pred_end = schedule.operations[pred]
                .scheduled_span
                .unwrap()
                .end;
            assert!(pred_end <= span.start, "{context}: operation {} starts before predecessor {pred} ends", op.id);
        }

        let resource_id = op
            .assigned_resource_id
            .unwrap_or_else(|| panic!("{context}: operation {} has no resource", op.id));
        per_resource
            .entry((op.assigned_resource_group_id, resource_id))
            .or_default()
            .push((span.start, span.end));
    }

    for ((group, resource), mut spans) in per_resource {
        spans.sort();
        for pair in spans.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "{context}: overlap on resource {resource} of group {group}: {:?} and {:?}", pair[0], pair[1]);
        }
    }
}

fn test_config(seed: u64) -> GenerationConfig {
    GenerationConfig {
        resource_group_sizes: vec![2, 1, 3],
        operation_chains_count: 20,
        min_opearions_per_chain: 4,
        max_opearions_per_chain: 9,
        min_operation_duration: 1,
        max_operation_duration: 12,
        batch_count: 5,
        max_batch_priority: 6,
        max_batch_start_time: 10,
        batch_due_time_factor: 0.6,
        extra_precedence_probability: 0.3,
        seed_count: 1,
        random_seed: seed,
    }
}

#[test]
fn serial_schedules_are_valid_for_all_strategies() {
    for seed in 0..40 {
        let base = generate_schedule(&test_config(seed));
        for &strategy in SortStrategy::all() {
            let mut schedule = base.clone();
            schedule
                .compute_serial(strategy)
                .unwrap_or_else(|e| panic!("serial {strategy:?} seed {seed}: {e}"));
            assert_valid(&schedule, &format!("serial {strategy:?} seed {seed}"));
        }
    }
}

#[test]
fn parallel_schedules_are_valid_for_all_strategies() {
    for seed in 0..40 {
        let base = generate_schedule(&test_config(seed));
        for &strategy in SortStrategy::all() {
            let mut schedule = base.clone();
            schedule
                .compute_parallel(strategy)
                .unwrap_or_else(|e| panic!("parallel {strategy:?} seed {seed}: {e}"));
            assert_valid(&schedule, &format!("parallel {strategy:?} seed {seed}"));
        }
    }
}

#[test]
fn known_makespans() {
    // Две независимые операции (3 и 4): один ресурс — 7, два ресурса — 4.
    for (resources, expected) in [(1, 7), (2, 4)] {
        for algorithm in schedulers::ALL {
            let mut schedule = build(&[resources], &[0], &[(3, 0, 0), (4, 0, 0)], &[]);
            schedule
                .compute(algorithm, SortStrategy::LongestDurationFirst)
                .unwrap();
            assert_valid(&schedule, &format!("{algorithm:?}"));
            assert_eq!(schedule.total_execute_time(), expected, "{algorithm:?}, {resources} resource(s)");
        }
    }

    // Цепочка 3 → 2 не ускоряется вторым ресурсом.
    for algorithm in schedulers::ALL {
        let mut schedule = build(&[2], &[0], &[(3, 0, 0), (2, 0, 0)], &[(0, 1)]);
        schedule
            .compute(algorithm, SortStrategy::PriorityThenDueTime)
            .unwrap();
        assert_eq!(schedule.total_execute_time(), 5);
    }
}

#[test]
fn serial_fills_gaps_before_later_operations() {
    // A (3, партия стартует в 5) выбирается первой, B (2, старт 0) должна встать в окно [0, 2).
    let mut schedule = build(&[1], &[5, 0], &[(3, 0, 0), (2, 1, 0)], &[]);
    schedule
        .compute_serial(SortStrategy::LongestDurationFirst)
        .unwrap();

    assert_eq!(schedule.operations[0].scheduled_span, Some(Span::new(5, 3)));
    assert_eq!(schedule.operations[1].scheduled_span, Some(Span::new(0, 2)));
    assert_valid(&schedule, "gap filling");
}

#[test]
fn resource_finds_gaps_in_sorted_allocations() {
    let mut resource = Resource::new(0);
    resource.allocate(Span::new(5, 3));
    resource.allocate(Span::new(0, 2));

    assert_eq!(resource.allocations, vec![Span::new(0, 2), Span::new(5, 3)]);
    assert_eq!(resource.find_free_span(3, 0), Span::new(2, 3)); // ровно влезает в окно [2, 5)
    assert_eq!(resource.find_free_span(4, 0), Span::new(8, 4)); // окно мало — после последней
    assert_eq!(resource.find_free_span(1, 6), Span::new(8, 1));
    assert_eq!(resource.find_free_span(0, 6), Span::new(6, 0)); // нулевая длина ресурс не занимает
}

#[test]
fn critical_path_data_is_correct() {
    // A(3) → B(2), C(4) независима; длина проекта 5.
    let schedule = build(&[1], &[0], &[(3, 0, 0), (2, 0, 0), (4, 0, 0)], &[(0, 1)]);
    let data = schedule.compute_critical_path_data().unwrap();

    assert_eq!(data.path_lengths, vec![5, 2, 4]);
    assert_eq!(data.earliest_finish, vec![3, 5, 4]);
    assert_eq!(data.float, vec![0, 0, 1]);
}

#[test]
fn cycle_is_reported_as_error() {
    for algorithm in schedulers::ALL {
        let mut schedule = build(&[1], &[0], &[(1, 0, 0), (1, 0, 0), (1, 0, 0)], &[(0, 1), (1, 2), (2, 1)]);
        assert!(
            schedule
                .compute(algorithm, SortStrategy::PriorityThenDueTime)
                .is_err(),
            "{algorithm:?}"
        );
        assert!(schedule.compute_critical_path_data().is_err());
    }
}

#[test]
fn empty_resource_group_is_reported_as_error() {
    for algorithm in schedulers::ALL {
        let mut schedule = build(&[0], &[0], &[(1, 0, 0)], &[]);
        assert!(
            schedule
                .compute(algorithm, SortStrategy::PriorityThenDueTime)
                .is_err(),
            "{algorithm:?}"
        );
    }
}

#[test]
fn recomputing_gives_the_same_result() {
    let mut schedule = generate_schedule(&test_config(7));
    for algorithm in schedulers::ALL {
        for &strategy in SortStrategy::all() {
            schedule.compute(algorithm, strategy).unwrap();
            let first = schedule.total_execute_time();
            schedule.compute(algorithm, strategy).unwrap();
            assert_eq!(first, schedule.total_execute_time(), "{algorithm:?} {strategy:?}");
            assert_valid(&schedule, &format!("recompute {algorithm:?} {strategy:?}"));
        }
    }
}

#[test]
fn random_strategy_is_reproducible() {
    let mut a = generate_schedule(&test_config(3));
    let mut b = a.clone();
    a.random_seed = 42;
    b.random_seed = 42;

    for algorithm in schedulers::ALL {
        a.compute(algorithm, SortStrategy::Random)
            .unwrap();
        b.compute(algorithm, SortStrategy::Random)
            .unwrap();
        let spans = |s: &Schedule| {
            s.operations
                .iter()
                .map(|op| op.scheduled_span)
                .collect::<Vec<_>>()
        };
        assert_eq!(spans(&a), spans(&b), "{algorithm:?}");
    }
}

#[test]
fn find_best_is_not_worse_than_any_strategy() {
    let base = generate_schedule(&test_config(11));
    for algorithm in schedulers::ALL {
        let (best, _) = Schedule::find_best(&base, algorithm, 0).unwrap();
        assert_valid(&best, "find_best");

        for &strategy in SortStrategy::all() {
            let mut schedule = base.clone();
            schedule.compute(algorithm, strategy).unwrap();
            assert!(best.total_execute_time() <= schedule.total_execute_time());
        }
    }
}

#[test]
fn improvement_fixes_a_bad_order() {
    // C и D занимают обе машины, цепочка A → B стартует только в 4: длительность 12, оптимум 8.
    for algorithm in schedulers::ALL {
        let build_bad = || build(&[2], &[0], &[(4, 0, 0), (4, 0, 0), (4, 0, 0), (4, 0, 0)], &[(2, 3)]);

        let mut plain = build_bad();
        plain
            .compute(algorithm, SortStrategy::ShortestDurationFirst)
            .unwrap();
        assert_eq!(plain.total_execute_time(), 12, "{algorithm:?} baseline");

        let mut improved = build_bad();
        improved
            .compute_improved(algorithm, SortStrategy::ShortestDurationFirst, DEFAULT_IMPROVE_ROUNDS)
            .unwrap();
        assert_valid(&improved, &format!("{algorithm:?} improved"));
        assert_eq!(improved.total_execute_time(), 8, "{algorithm:?} improved");
    }
}

#[test]
fn improvement_is_valid_and_never_worse() {
    for seed in 0..40 {
        let base = generate_schedule(&test_config(seed));
        for algorithm in schedulers::ALL {
            for &strategy in SortStrategy::all() {
                let mut schedule = base.clone();
                schedule.compute(algorithm, strategy).unwrap();
                let before = schedule.total_execute_time();

                schedule
                    .improve(algorithm, DEFAULT_IMPROVE_ROUNDS)
                    .unwrap();
                assert_valid(&schedule, &format!("improved {algorithm:?} {strategy:?} seed {seed}"));
                assert!(schedule.total_execute_time() <= before, "{algorithm:?} {strategy:?} seed {seed}: {} > {before}", schedule.total_execute_time());
            }
        }
    }
}

#[test]
fn improving_an_uncomputed_schedule_is_an_error() {
    let mut schedule = build(&[1], &[0], &[(1, 0, 0)], &[]);
    assert!(schedule.improve(&SerialScheduler, 3).is_err());
}

#[test]
fn zero_rounds_changes_nothing() {
    let mut schedule = generate_schedule(&test_config(5));
    schedule
        .compute(&ParallelScheduler, SortStrategy::Random)
        .unwrap();
    let before: Vec<_> = schedule
        .operations
        .iter()
        .map(|op| op.scheduled_span)
        .collect();
    assert_eq!(schedule.improve(&ParallelScheduler, 0).unwrap(), 0);
    let after: Vec<_> = schedule
        .operations
        .iter()
        .map(|op| op.scheduled_span)
        .collect();
    assert_eq!(before, after);
}
