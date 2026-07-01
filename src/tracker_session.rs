use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct TrackerSession {
    pub start_time: i64,
    pub duration: i64,
}

impl TrackerSession {
    pub fn new(start_time: i64, duration: i64) -> Self {
        Self {
            start_time,
            duration,
        }
    }
}