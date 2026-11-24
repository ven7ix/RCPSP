pub type Time = u32;

/// Полуинтервал `[start, end)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Time,
    pub end: Time,
}

impl Span {
    pub fn new(start: Time, duration: Time) -> Self {
        Self { start, end: start + duration }
    }

    pub fn duration(&self) -> Time {
        self.end - self.start
    }

    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }
}
