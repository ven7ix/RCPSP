pub type Time = u32;

#[derive(Copy, Clone)]
pub struct Span {
    pub start: Time,
    pub end: Time,
    pub duration: Time
}

impl Span {
    pub fn new(start: Time, duration: Time) -> Self {
        return Self { start: start, end: start + duration, duration: duration };
    }
    
    pub fn overlaps(&self, other: &Span) -> bool {
        return self.start < other.end && other.start < self.end;
    }
}