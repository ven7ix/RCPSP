//! Планирование RCPSP: данные задачи, алгоритмы построения расписания и их сравнение.
//!
//! - `schedule` — структура задачи/расписания и общие шаги планирования;
//! - `schedulers` — алгоритмы построения расписания (трейт `Scheduler`, серийный и параллельный);
//! - `priority` — правила приоритета (`SortStrategy`);
//! - `critical_path` — критический путь;
//! - `improvement` — двусторонний проход, улучшающий готовое расписание;
//! - `solver` — удобные точки входа (`compute`, `compute_improved`, `find_best`);
//! - `leaderboard` — сравнение стратегий и алгоритмов на множестве сидов;
//! - `schedule_generator` — генерация случайной задачи.

pub mod critical_path;
pub mod improvement;
pub mod indices;
pub mod job;
pub mod leaderboard;
pub mod priority;
pub mod schedule;
pub mod schedule_generator;
pub mod schedulers;
pub mod solver;
pub mod time;
pub mod worker;
