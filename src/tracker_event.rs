use serde::Serialize;

#[derive(Serialize, PartialEq, Clone, Debug)]
pub enum TrackerEventType {
    Start,
    Stop,
}

#[derive(Serialize, Clone, Debug)]
pub struct TrackerEvent {
    pub event_type: TrackerEventType,
    pub timestamp: i64,
}

impl TrackerEvent {
    pub fn new(event_type: TrackerEventType, timestamp: i64) -> Self {
        Self { event_type, timestamp }
    }
}