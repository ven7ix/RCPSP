//! Алгоритмы построения расписания. Каждый — отдельная структура, реализующая `Scheduler`,
//! так что добавить новый алгоритм значит добавить файл и строку в `ALL`.

mod parallel;
mod serial;

pub use parallel::ParallelScheduler;
pub use serial::SerialScheduler;

use crate::priority::PriorityKey;
use crate::schedule::Schedule;

/// Алгоритм, который по ключам приоритета расставляет все операции по ресурсам и времени.
pub trait Scheduler: std::fmt::Debug {
    /// Короткое имя для вывода и имён файлов.
    fn name(&self) -> &'static str;

    /// Строит расписание. Ожидает «чистое» состояние (после `Schedule::reset`);
    /// возвращает `Err`, если расписание построить нельзя (цикл в связях, пустая группа ресурсов).
    fn build(&self, schedule: &mut Schedule, keys: &[PriorityKey]) -> Result<(), String>;
}

/// Все доступные алгоритмы.
pub const ALL: [&dyn Scheduler; 2] = [&SerialScheduler, &ParallelScheduler];
