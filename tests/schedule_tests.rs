#[cfg(test)]
mod tests {
    use rcpsp::job::*;
    use rcpsp::time::*;
    use rcpsp::worker::*;
    use rcpsp::schedule::*;
    
    /// Вспомогательная функция: проверяет, что все предшественники завершились до начала операции
    fn schedule_is_valid(schedule: &Schedule) -> bool {
        for op in &schedule.operations {
            if let Some(span) = op.scheduled_span {
                let start_time: Time = span.start;
                // Проверка времени старта партии
                let batch_start_time: Time = schedule.batches[op.assigned_batch_id].start_time;
                if start_time < batch_start_time {
                    return false;
                }
                // Проверка предшественников
                for &pred_id in &op.predecessor_ids {
                    if let Some(pred_span) = schedule.operations[pred_id].scheduled_span {
                        if pred_span.end > start_time {
                            return false; // предшественник ещё не закончился
                        }
                    } 
                    else {
                        return false; // предшественник не запланирован вообще
                    }
                }
            } 
            else {
                return false; // операция не запланирована
            }
        }
        
        return true;
    }

    #[test]
    fn test_two_predecessors_different_duration_now_correct() {
        let mut schedule = Schedule::new();
        schedule.add_resource_group(ResourceGroup::new(0, 2));
        schedule.add_batch(Batch::new(0, 0, 10, 0));
        schedule.add_operation(Operation::new(0, 8, 0, 0)); // op0 – длинный
        schedule.add_operation(Operation::new(1, 3, 0, 0)); // op1 – короткий
        schedule.add_operation(Operation::new(2, 2, 0, 0)); // op2 – последователь
        schedule.add_precedence(0, 2);
        schedule.add_precedence(1, 2);

        assert!(schedule.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&schedule));

        let op2_start = schedule.operations[2].scheduled_span.unwrap().start;
        assert!(op2_start >= 8, "op2 должен начаться не раньше 8, а начался в {}", op2_start);
    }
    
    
    #[test]
    fn test_chain_with_batch_delay() {
        // Тест 1: цепочка 0->1->2, batch1 стартует позже
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 2));
        s.add_batch(Batch::new(0, 0, 10, 0));   // batch 0
        s.add_batch(Batch::new(1, 10, 20, 0));  // batch 1, старт 10
        s.add_operation(Operation::new(0, 5, 0, 0)); // op0
        s.add_operation(Operation::new(1, 4, 0, 0)); // op1
        s.add_operation(Operation::new(2, 3, 1, 0)); // op2 (batch 1)
        s.add_precedence(0, 1);
        s.add_precedence(1, 2);
        
        let compute_result = s.compute_schedule_parallel();
        
        assert!(compute_result.is_ok());
        assert!(schedule_is_valid(&s), "Нарушены зависимости");

        let op0_end = s.operations[0].scheduled_span.unwrap().end;
        let op1_end = s.operations[1].scheduled_span.unwrap().end;
        let op2_start = s.operations[2].scheduled_span.unwrap().start;
        // op0 (0-5), op1 (5-9), op2 стартует не раньше max(op1_end=9, batch1.start=10) = 10
        assert!(op0_end == 5);
        assert!(op1_end == 9);
        assert!(op2_start >= 10);
        
        match compute_result {
            Ok(()) => s.print_schedule("parallel"),
            Err(e) => println!("Error: {}", e),
        }
    }

    #[test]
    fn test_two_predecessors_different_duration() {
        // Тест 2: op0 длинный, op1 короткий, оба -> op2
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 2));
        s.add_batch(Batch::new(0, 0, 10, 0));
        s.add_operation(Operation::new(0, 8, 0, 0)); // длинный
        s.add_operation(Operation::new(1, 3, 0, 0)); // короткий
        s.add_operation(Operation::new(2, 2, 0, 0)); // successor
        s.add_precedence(0, 2);
        s.add_precedence(1, 2);

        assert!(s.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&s));

        let op2_start = s.operations[2].scheduled_span.unwrap().start;
        // op2 может начаться не раньше max(8,3)=8
        assert!(op2_start >= 8, "op2 начался слишком рано: {}", op2_start);
    }

    #[test]
    fn test_reverse_planning_order() {
        // Тест 3: длинный op0, короткий op1, successor op2
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 2));
        s.add_batch(Batch::new(0, 0, 10, 0));
        s.add_operation(Operation::new(0, 9, 0, 0)); // op0
        s.add_operation(Operation::new(1, 2, 0, 0)); // op1
        s.add_operation(Operation::new(2, 1, 0, 0)); // op2
        s.add_precedence(0, 2);
        s.add_precedence(1, 2);

        assert!(s.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&s));
        let op2_start = s.operations[2].scheduled_span.unwrap().start;
        assert!(op2_start >= 9, "op2 должен начаться после завершения op0 (9)");
    }

    #[test]
    fn test_three_predecessors() {
        // Тест 4: три предшественника с разными длительностями (5,7,4) -> successor
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 3)); // три ресурса
        s.add_batch(Batch::new(0, 0, 10, 0));
        s.add_operation(Operation::new(0, 5, 0, 0));
        s.add_operation(Operation::new(1, 7, 0, 0));
        s.add_operation(Operation::new(2, 4, 0, 0));
        s.add_operation(Operation::new(3, 2, 0, 0)); // successor
        s.add_precedence(0, 3);
        s.add_precedence(1, 3);
        s.add_precedence(2, 3);

        assert!(s.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&s));
        let op3_start = s.operations[3].scheduled_span.unwrap().start;
        // максимум из окончаний: op1 заканчивается в 7
        assert!(op3_start >= 7);
    }

    #[test]
    fn test_delayed_predecessor_due_to_resource() {
        // Тест 5: дефицит ресурсов, чтобы предшественник стартовал позже
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 1)); // один ресурс
        s.add_batch(Batch::new(0, 0, 10, 0));
        // Две независимые длинные операции, чтобы одна была отложена
        s.add_operation(Operation::new(0, 6, 0, 0)); // op0
        s.add_operation(Operation::new(1, 4, 0, 0)); // op1 (предшественник)
        s.add_operation(Operation::new(2, 2, 0, 0)); // successor op2 зависит от op1
        s.add_precedence(1, 2);
        // Также добавим ещё одну операцию без зависимостей, чтобы занять ресурс в начале
        // (можно использовать op0). Порядок планирования может быть: op0 стартует в 0,
        // op1 ждёт, потом стартует в 6, завершается в 10, значит op2 стартует в 10.
        // Проверим, что op2 не начнётся раньше завершения op1.

        assert!(s.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&s));
        let op1_end = s.operations[1].scheduled_span.unwrap().end;
        let op2_start = s.operations[2].scheduled_span.unwrap().start;
        assert!(op2_start >= op1_end, "op2 должен начаться после op1");
    }

    #[test]
    fn test_nested_chains() {
        // Тест 6: две цепочки, финальная операция зависит от обеих
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 2));
        s.add_batch(Batch::new(0, 0, 10, 0));
        // цепочка 0->1 (3+5=8)
        s.add_operation(Operation::new(0, 3, 0, 0));
        s.add_operation(Operation::new(1, 5, 0, 0));
        // цепочка 2->3 (4+2=6)
        s.add_operation(Operation::new(2, 4, 0, 0));
        s.add_operation(Operation::new(3, 2, 0, 0));
        // op4 зависит от op1 и op3
        s.add_operation(Operation::new(4, 6, 0, 0));
        s.add_precedence(0, 1);
        s.add_precedence(2, 3);
        s.add_precedence(1, 4);
        s.add_precedence(3, 4);

        assert!(s.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&s));
        let op1_end = s.operations[1].scheduled_span.unwrap().end;
        let op3_end = s.operations[3].scheduled_span.unwrap().end;
        let op4_start = s.operations[4].scheduled_span.unwrap().start;
        let max_predecessor_end = op1_end.max(op3_end);
        assert!(op4_start >= max_predecessor_end);
    }

    #[test]
    fn test_future_batch_with_predecessors() {
        // Тест 7: операция из будущей партии с несколькими предшественниками
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 2));
        s.add_batch(Batch::new(0, 0, 10, 0));   // batch 0 старт 0
        s.add_batch(Batch::new(1, 0, 20, 15));  // batch 1 старт 15
        s.add_operation(Operation::new(0, 10, 0, 0)); // op0 (batch 0)
        s.add_operation(Operation::new(1, 2, 0, 0));  // op1 (batch 0)
        s.add_operation(Operation::new(2, 4, 1, 0));  // op2 (batch 1), зависит от op0 и op1
        s.add_precedence(0, 2);
        s.add_precedence(1, 2);

        assert!(s.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&s));
        let op0_end = s.operations[0].scheduled_span.unwrap().end;
        let op1_end = s.operations[1].scheduled_span.unwrap().end;
        let batch1_start = s.batches[1].start_time;
        let op2_start = s.operations[2].scheduled_span.unwrap().start;
        // op2 может стартовать не раньше max(op0_end, op1_end, batch1_start)
        let earliest = op0_end.max(op1_end).max(batch1_start);
        assert!(op2_start >= earliest, "op2 начался в {}, ожидалось >= {}", op2_start, earliest);
    }

    #[test]
    fn test_diamond_dependency() {
        // Тест 8: алмазная зависимость 0->1, 0->2, 1->3, 2->3
        let mut s = Schedule::new();
        s.add_resource_group(ResourceGroup::new(0, 2));
        s.add_batch(Batch::new(0, 0, 10, 0));
        s.add_operation(Operation::new(0, 1, 0, 0)); // op0
        s.add_operation(Operation::new(1, 8, 0, 0)); // op1
        s.add_operation(Operation::new(2, 3, 0, 0)); // op2
        s.add_operation(Operation::new(3, 2, 0, 0)); // op3
        s.add_precedence(0, 1);
        s.add_precedence(0, 2);
        s.add_precedence(1, 3);
        s.add_precedence(2, 3);

        assert!(s.compute_schedule_parallel().is_ok());
        assert!(schedule_is_valid(&s));
        let op1_end = s.operations[1].scheduled_span.unwrap().end;
        let op2_end = s.operations[2].scheduled_span.unwrap().end;
        let op3_start = s.operations[3].scheduled_span.unwrap().start;
        assert!(op3_start >= op1_end && op3_start >= op2_end);
    }
    
    #[test]
    fn test_two_predecessors_different_duration_fails_with_partial_fix() {
        // Ситуация: op2 зависит от op0 (длит. 8) и op1 (длит. 3).
        // Правильное поведение: op2 стартует не раньше max(8, 3) = 8.
        // Ваше исправление: successor добавляется при планировании последнего предшественника
        // и start_time = batch.start + operation.end. Если последним планируется op1 (end=3),
        // то op2 получит start_time = 3 и сможет стартовать до завершения op0.

        let mut schedule = Schedule::new();
        schedule.add_resource_group(ResourceGroup::new(0, 2)); // 2 ресурса
        schedule.add_batch(Batch::new(0, 0, 10, 0)); // одна партия

        // Операции
        schedule.add_operation(Operation::new(0, 8, 0, 0)); // op0 – длинная
        schedule.add_operation(Operation::new(1, 3, 0, 0)); // op1 – короткая
        schedule.add_operation(Operation::new(2, 2, 0, 0)); // op2 – последователь

        // Зависимости: 0 -> 2, 1 -> 2
        schedule.add_precedence(0, 2);
        schedule.add_precedence(1, 2);

        // Запускаем метод с вашим исправленным add_successors_to_pending_operations
        let result = schedule.compute_schedule_parallel();
        // Ошибки быть не должно (метод отрабатывает без паники)
        assert!(result.is_ok());

        // Проверяем корректность расписания
        // Функция schedule_is_valid должна вернуть false – предшественник op0 ещё не завершён.
        assert!(
            !schedule_is_valid(&schedule),
            "Ожидалось нарушение зависимостей, но расписание признано корректным"
        );

        // Можно также явно проверить время старта op2
        let op2_start = schedule.operations[2].scheduled_span.unwrap().start;
        let op0_end = schedule.operations[0].scheduled_span.unwrap().end;
        assert!(
            op2_start < op0_end,
            "op2 начался в {}, а должен не раньше {} (конец op0)",
            op2_start, op0_end
        );
    }
}