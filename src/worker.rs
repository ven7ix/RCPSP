use crate::{
    indices::{ResourceGroupId, ResourceId},
    time::{Span, Time},
};

#[derive(Clone)]
pub struct Resource {
    pub id: ResourceId,
    /// Занятые интервалы: не пересекаются и всегда отсортированы по времени.
    pub allocations: Vec<Span>,
}

impl Resource {
    pub fn new(id: ResourceId) -> Self {
        Self { id, allocations: Vec::new() }
    }

    /// Самый ранний свободный интервал длиной `duration`, начинающийся не раньше `after_time`
    /// (учитывает «окна» между уже занятыми интервалами).
    pub fn find_free_span(&self, duration: Time, after_time: Time) -> Span {
        // Операция нулевой длины ресурс не занимает.
        if duration == 0 {
            return Span::new(after_time, 0);
        }

        let mut start = after_time;
        for allocation in &self.allocations {
            if allocation.end <= start {
                continue;
            }
            if allocation.start >= start + duration {
                break; // окно перед этим интервалом достаточно большое
            }
            start = allocation.end;
        }
        Span::new(start, duration)
    }

    pub fn allocate(&mut self, span: Span) {
        if span.duration() == 0 {
            return;
        }
        debug_assert!(
            self.allocations
                .iter()
                .all(|a| !a.overlaps(&span)),
            "resource {} is double-booked",
            self.id
        );

        let position = self
            .allocations
            .partition_point(|a| a.start < span.start);
        self.allocations.insert(position, span);
    }
}

#[derive(Clone)]
pub struct ResourceGroup {
    pub id: ResourceGroupId,
    pub resources: Vec<Resource>,
}

impl ResourceGroup {
    pub fn new(id: ResourceGroupId, resource_count: usize) -> Self {
        Self { id, resources: (0..resource_count).map(Resource::new).collect() }
    }

    /// Ресурс группы, на котором операция начнётся раньше всего (при равенстве — с меньшим номером).
    /// `None`, если в группе нет ресурсов.
    pub fn find_best_resource_for_operation(&self, duration: Time, after_time: Time) -> Option<(ResourceId, Span)> {
        self.resources
            .iter()
            .enumerate()
            .map(|(id, resource)| (id, resource.find_free_span(duration, after_time)))
            .min_by_key(|&(_, span)| span.start)
    }

    pub fn allocate_resource(&mut self, resource_id: ResourceId, span: Span) {
        self.resources[resource_id].allocate(span);
    }

    pub fn clear(&mut self) {
        self.resources
            .iter_mut()
            .for_each(|resource| resource.allocations.clear());
    }
}
