pub mod time;
pub mod indices;
pub mod job;
pub mod worker;
pub mod schedule;

use crate::job::Operation;
use crate::job::Batch;
use crate::worker::ResourceGroup;
use crate::schedule::Schedule;

// добавить генерацию задачи
// добавить итерации алгоритма с разными параметрами
fn main() {
    let mut schedule: Schedule = Schedule::new();

    schedule.add_resource_group(ResourceGroup::new(0, 2));

    schedule.add_batch(Batch::new(0, 0, 10, 0));
    schedule.add_batch(Batch::new(0, 0, 10, 0));

    schedule.add_operation(Operation::new(0, 5, 0, 0));
    schedule.add_operation(Operation::new(1, 3, 0, 0));
    schedule.add_operation(Operation::new(2, 4, 1, 0));
    schedule.add_operation(Operation::new(3, 2, 1, 0));

    schedule.add_precedence(0, 2);
    schedule.add_precedence(1, 3);

    // match schedule.compute_schedule() {
    //     Ok(()) => schedule.print_schedule(""),
    //     Err(e) => println!("Error: {}", e),
    // }
    
    match schedule.compute_schedule_parallel() {
        Ok(()) => schedule.print_schedule("parallel"),
        Err(e) => println!("Error: {}", e),
    }
}