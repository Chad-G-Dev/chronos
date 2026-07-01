use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct Day {
    pub day_index: usize,
    pub time: i64,
}

impl Day {
    pub fn new(day_index: usize, time: i64) -> Self {
        Self { day_index, time }
    }
}