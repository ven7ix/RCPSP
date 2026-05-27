// #[cfg(test)]
// mod tests {
//     use rcpsp::job::*;
//     use rcpsp::time::*;
//     use rcpsp::worker::*;
//     use rcpsp::schedule::*;
    
//     /// Вспомогательная функция: проверяет, что все предшественники завершились до начала операции
//     fn schedule_is_valid(schedule: &Schedule) -> bool {
//         for op in &schedule.operations {
//             if let Some(span) = op.scheduled_span {
//                 let start_time: Time = span.start;
//                 // Проверка времени старта партии
//                 let batch_start_time: Time = schedule.batches[op.assigned_batch_id].start_time;
//                 if start_time < batch_start_time {
//                     return false;
//                 }
//                 // Проверка предшественников
//                 for &pred_id in &op.predecessor_ids {
//                     if let Some(pred_span) = schedule.operations[pred_id].scheduled_span {
//                         if pred_span.end > start_time {
//                             return false; // предшественник ещё не закончился
//                         }
//                     } 
//                     else {
//                         return false; // предшественник не запланирован вообще
//                     }
//                 }
//             } 
//             else {
//                 return false; // операция не запланирована
//             }
//         }
        
//         return true;
//     }
// }