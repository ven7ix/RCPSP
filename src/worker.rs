use crate::{
    indices::{ResourceGroupId, ResourceId},
    time::{Span, Time},
};

#[derive(Clone)]
pub struct Resource {
    pub id: ResourceId,
    pub allocations: Vec<Span>,
}

impl Resource {
    pub fn new(id: ResourceId) -> Self {
        return Self { id: id, allocations: Vec::new() };
    }

    pub fn find_free_span(&self, duration: Time, after_time: Time) -> Option<Span> {
        let mut potential_span: Span = Span::new(after_time, duration);

        loop {
            if let Some(allocation) = self
                .allocations
                .iter()
                .find(|span: &&Span| potential_span.overlaps(&span))
            {
                potential_span = Span::new(allocation.end, duration);
            } else {
                return Some(potential_span);
            }
        }
    }

    pub fn allocate(&mut self, span: Span) {
        self.allocations.push(span);
    }
}

#[derive(Clone)]
pub struct ResourceGroup {
    pub id: ResourceGroupId,
    pub resources: Vec<Resource>,
    pub next_available_time: Time,
}

impl ResourceGroup {
    pub fn new(id: ResourceGroupId, resource_count: usize) -> Self {
        return Self {
            id: id,
            resources: (0..resource_count)
                .map(|id: ResourceId| Resource::new(id))
                .collect(),
            next_available_time: 0,
        };
    }

    fn update_next_available_time(&mut self) {
        self.next_available_time = self
            .resources
            .iter()
            .filter_map(|resource| resource.allocations.last().map(|span| span.end))
            .min()
            .unwrap_or(0);
    }

    pub fn find_best_resource_for_operation(&self, duration: Time, after_time: Time) -> Option<(ResourceId, Span)> {
        let mut best: Option<(ResourceId, Span)> = None;
        for (i, resource) in self.resources.iter().enumerate() {
            if let Some(span) = resource.find_free_span(duration, after_time) {
                match best {
                    None => best = Some((i, span)),
                    Some((_, ref best_span)) if span.start < best_span.start => {
                        best = Some((i, span));
                    }
                    _ => {}
                }
            }
        }
        return best;
    }

    pub fn allocate_resource(&mut self, resource_id: ResourceId, span: Span) {
        self.resources[resource_id].allocate(span);
        self.update_next_available_time();
    }
}
