use rcpsp::job::*;
use rcpsp::worker::*;
use rcpsp::schedule::*;

// добавить генерацию задачи
// добавить итерации алгоритма с разными параметрами
fn main() {
    // let config = GenerationConfig::new();
    // let mut schedule = schedule_generator::generate_random_schedule(&config);
    // match schedule.compute_schedule_parallel_new() {
    //     Ok(()) => schedule.print_schedule(""),
    //     Err(e) => println!("Error: {}", e),
    // }
    
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