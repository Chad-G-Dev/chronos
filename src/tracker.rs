use axum::response::Response;
use serde::Serialize;
use sqlx::{query, AssertSqlSafe, Pool, Row, Sqlite};
use crate::chronos::{Chronos, ChronosError, ChronosErrorTypes};
use crate::day::Day;
use crate::tracker_event::{TrackerEvent, TrackerEventType};
use crate::tracker_session::TrackerSession;

#[derive(Serialize, Clone, Debug)]
pub struct Tracker {
    pub name: String,
    pub created_at: i64,
    pub toggled: bool,
    pub last_start: i64,

    pub events: Vec<TrackerEvent>,
    pub sessions: Vec<TrackerSession>,
    pub time_per_day: Vec<Day>,

    #[serde(skip)]
    pool: Pool<Sqlite>,
}

impl Tracker {
    pub async fn new(name: &str, created_at: i64, pool: Pool<Sqlite>) -> Result<Self, ChronosError> {
        if !Chronos::get_tables(&pool).await?.contains(&name.to_string()) {
            match Self::create_table(&name, &pool).await {
                Ok(()) => (),
                Err(e) => return Err(e),
            };
        }

        let events = match Self::query_db(&name, &pool).await {
            Ok(events) => events,
            Err(e) => return Err(e)
        };

        let (sessions, (toggled, last_start)) = Self::get_sessions(&events);
        let time_per_day = Self::get_time_per_day(&sessions);

        Ok(Self {
            name: name.to_string(),
            created_at,
            toggled,
            last_start,

            events,
            sessions,
            time_per_day,

            pool,
        })
    }

    // Function that queries the DB for all the events
    async fn query_db(table_name: &str, pool: &Pool<Sqlite>) -> Result<Vec<TrackerEvent>, ChronosError> {
        // query the db
        let rows = match sqlx::query(AssertSqlSafe(format!("SELECT * FROM {} ORDER BY timestamp ASC", table_name)))
            .fetch_all(pool)
            .await {
            Ok(rows) => rows,
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, "Failed to query database".to_string()))
        };

        // create the output vec
        let mut events: Vec<TrackerEvent> = vec![];

        // parse the rows into the output vec
        for row in rows {
            let timestamp = row.get::<i64, _>(0);
            let event_type = row.get::<String, _>(1);

            let tracker_event_type = match &event_type[..] {
                "Start" | "start" => TrackerEventType::Start,
                "Stop" | "stop" => TrackerEventType::Stop,
                _ => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, "Unknown event type".to_string()))
            };

            events.push(TrackerEvent::new(tracker_event_type, timestamp));
        }

        Ok(events)
    }

    // function that build the sessions from the events
    fn get_sessions(events: &Vec<TrackerEvent>) -> (Vec<TrackerSession>, (bool, i64)) {
        // create the output vec
        let mut sessions: Vec<TrackerSession> = vec![];

        // initialize the variables needed
        let mut last_start = 0;
        let mut toggled = false;

        for event in events {
            if event.event_type == TrackerEventType::Start {
                toggled = true;
                last_start = event.timestamp;
            } else {
                sessions.push(TrackerSession::new(last_start, event.timestamp - last_start));
                toggled = false;
            }
        }

        (sessions, (toggled, last_start))
    }

    fn get_time_per_day(sessions: &Vec<TrackerSession>) -> Vec<Day> {
        // create output vec
        let mut time_per_day: Vec<Day> = vec![];

        // initialize the variables
        let mut day_index: usize = 1;
        let mut current_time: i64 = 0;

        let timestamp = Self::get_timestamp();
        let day_epoch_value: i64 = 24 * 60 * 60;

        // Generate the output vec
        for session in sessions {
            if session.start_time >= timestamp - day_epoch_value * day_index as i64 {
                current_time += session.duration;
            } else {
                current_time = session.duration;
                day_index += 1;
                time_per_day.push(Day::new(day_index, current_time));
            }
        }

        time_per_day.push(Day::new(day_index, current_time));

        time_per_day
    }

    pub fn get_timestamp() -> i64 {
        match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs() as i64,
            Err(_) => 0,
        }
    }

    async fn update(&mut self) -> Result<(), ChronosError> {
        match Self::query_db(&self.name, &self.pool).await {
            Ok(events) => {
                self.events = events;
                self.sessions = Self::get_sessions(&self.events).0;
                self.toggled = Self::get_sessions(&self.events).1.0;
                self.last_start = Self::get_sessions(&self.events).1.1;
                self.time_per_day = Self::get_time_per_day(&self.sessions);
                Ok(())
            },
            Err(err) => Err(err),
        }
    }

    async fn create_table(table_name: &str, pool: &Pool<Sqlite>) -> Result<(), ChronosError> {
        match query(AssertSqlSafe(format!("CREATE TABLE {} (timestamp INTEGER NOT NULL, event_type TEXT NOT NULL)", table_name))).execute(pool).await {
            Ok(_) => (),
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to create table: {}", e))),
        };

        // add it to the tracker table
        match query(AssertSqlSafe(format!("INSERT INTO trackers (name, created_at) VALUES ('{}', '{}')", table_name, Self::get_timestamp())))
            .execute(pool)
            .await {
            Ok(_) => Ok(()),
            Err(error) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to create tracker: {}", error))),
        }
    }

    pub async fn delete(&mut self) -> Result<(), ChronosError> {
        match query("DELETE FROM trackers WHERE name = ?")
            .bind(&self.name)
            .execute(&self.pool)
            .await {
            Ok(_) => (),
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to delete tracker: {}", e))),
        };

        match query(AssertSqlSafe(format!("DROP TABLE {}", self.name)))
            .execute(&self.pool)
            .await {
            Ok(_) => (),
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to delete tracker: {}", e))),
        };

        Ok(())

    }

    pub async fn toggle (&mut self) -> Result<(), ChronosError> {
        match query(AssertSqlSafe(format!("INSERT INTO {} (timestamp, event_type) VALUES (?, ?)", self.name)))
            .bind(Self::get_timestamp())
            .bind(if self.toggled { "Stop" } else { "Start" })
            .execute(&self.pool)
            .await {
            Ok(_) => (),
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to toggle tracker: {}", e))),
        };

        match self.update().await {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        Ok(())
    }

    pub fn report(&self, days: usize) -> String {
        let mut time: i64 = 0;

        for session in &self.sessions {
            if session.start_time >= Self::get_timestamp() - 24 * 60 * 60 * days as i64 {
                time += session.duration;
            }
        }

        if self.toggled {
            time += Self::get_timestamp() - self.last_start;
        }

        Self::format_epoch(time)
    }

    pub fn format_epoch(mut time: i64) -> String {
        // get individual units of time
        let seconds = time % 60;
        time /= 60;
        let minutes = time % 60;
        time /= 60;
        let hours = time % 24;
        time /= 24;
        let days = time % 365;
        time /= 365;

        // create the variable to store the output
        let mut output = String::new();

        // generate the output string
        if time > 1 {
            output.push_str(format!("{} years, ", time).as_str());
        }  else if time > 0 {
            output.push_str("1 year, ");
        }

        if days > 1 {
            output.push_str(format!("{} days, ", days).as_str());
        } else if days > 0 {
            output.push_str("1 day, ");
        } else if time > 0 {
            output.push_str("0 days, ");
        }

        if hours > 1 {
            output.push_str(format!("{} hours, ", hours).as_str());
        } else if days > 0 {
            output.push_str("1 hour, ");
        } else if time > 0 || days > 0 {
            output.push_str("0 hours, ");
        }

        if minutes > 1 {
            output.push_str(format!("{} minutes, ", minutes).as_str());
        } else if days > 0 {
            output.push_str("1 minute, ");
        } else if time > 0 || days > 0 || hours > 0{
            output.push_str("0 minutes, ");
        }

        if seconds > 1 {
            output.push_str(format!("{} seconds", seconds).as_str());
        } else if days > 0 {
            output.push_str("1 seconds");
        } else {
            output.push_str("0 seconds");
        }

        output
    }
}