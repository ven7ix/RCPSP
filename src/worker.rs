use crate::{indices::{ResourceGroupId, ResourceId}, time::{Span, Time}};

pub struct Resource {
    pub id: ResourceId,
    pub allocations: Vec<Span>
}

impl Resource {
    pub fn new(id: ResourceId) -> Self {
        return Self { id: id, allocations: Vec::new() };
    }
    
    pub fn find_free_span(&self, duration: Time, after_time: Time) -> Option<Span> {
        let mut potential_span: Span = Span::new(after_time, duration);
        
        loop {
            if let Some(allocation) = self.allocations.iter().find(|span: &&Span| potential_span.overlaps(&span)) {
                potential_span = Span::new(allocation.end, duration);
            }
            else {
                return Some(potential_span);
            }
        }
    }
    
    pub fn allocate(&mut self, span: Span) {
        self.allocations.push(span);
    }
}

pub struct ResourceGroup {
    pub id: ResourceGroupId,
    pub resources: Vec<Resource>
}

impl ResourceGroup {
    pub fn new(id: ResourceGroupId, resource_count: usize) -> Self {
        let resources: Vec<Resource> = (0..resource_count).map(|id: ResourceId| Resource::new(id)).collect();
        return Self { id: id, resources: resources };
    }
    
    pub fn allocate_best_resource_for_operation(&mut self, duration: Time, after_time: Time) -> Option<(ResourceId, Span)> {
        let mut best_resource: Option<(ResourceId, Span)> = None;
        
        for (i, resource) in self.resources.iter().enumerate() {
            if let Some(span) = resource.find_free_span(duration, after_time) {
                if best_resource.is_none() || best_resource.unwrap().1.start > span.start {
                    best_resource = Some((i, span));
                }
            }
        }
        
        if let Some((id, span)) = best_resource {
            self.resources[id].allocate(span);
            return Some((id, span));
        }
        
        return None;
    }
}