use std::time::Duration;
use chrono::DateTime;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::{Layout, Rect},
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use ratatui::layout::{Alignment, Constraint, Direction};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Bar, BarChart, BorderType, Borders, List, ListState};
/// # Chronos
///
/// Chronos is a struct that tracks time by storing events in a sqlite database.
///
/// You can manage trackers and add events or provide it a command

/// ================================================================================================
/// IMPORTS
/// ================================================================================================

use sqlx::{AssertSqlSafe, migrate::MigrateDatabase, query, FromRow, Pool, Row, Sqlite, SqliteConnection, SqlitePool};

// =================================================================================================
// CONSTANTS
// =================================================================================================
const DATABASE_URL: &str = "sqlite://sqlite.db";
const VERSION: &str = "2.0.0";

pub enum ChronosErrorTypes {
    NoArguments,
    DatabaseError,
    CommandNotFound,
    InvalidCommand,
    NameAlreadyExists,
    TrackerAlreadyExists,
    NoCurrentTracker,
}

enum ChronosTuiState {
    Home,
    TrackerSelection,
    CreateTracker,
    Tracker,
    Error
}

pub struct ChronosError {
    pub error_type: ChronosErrorTypes,
    pub message: String,
}

impl ChronosError {
    pub fn new(error_type: ChronosErrorTypes, message: String) -> ChronosError {
        ChronosError {
            error_type,
            message
        }
    }
}

/// ================================================================================================
/// Struct definition
/// ================================================================================================
pub struct Chronos {
    pool: Pool<Sqlite>,
    pub database_ok: bool,
    pub trackers: Vec<String>,
    tui_state: ChronosTuiState,
    current_tracker: Option<String>,
    current_events: Option<Vec<(String, i64)>>,
    current_sessions: Option<Vec<String>>,
    error: Option<ChronosError>,
    pub exit: bool,
}

impl Chronos {
    pub async fn new() -> Result<Self, ChronosError> {
        let pool = match Self::connect_to_database().await {
            Ok(pool) => pool,
            Err(error) => return Err(error),
        };

        let mut database_ok = true;

        // verify that the tracker table exists
        let tables = Self::list_from_pool(&pool).await;
        if !tables.contains(&"trackers".to_string()) {
            match Self::create_tracker_table(&pool).await {
                Ok(_) => {},
                Err(_) => database_ok = false,
            }
        }

        let trackers = Self::get_tracker_list(&pool).await;

        // return a new Chronos object
        Ok(Self {
            pool,
            database_ok,
            trackers,
            tui_state: ChronosTuiState::Home,
            current_tracker: None,
            current_events: None,
            current_sessions: None,
            error: None,
            exit: false,
        })
    }

    pub async fn get_tracker_list(pool: &Pool<Sqlite>) -> Vec<String> {
        let rows = query("SELECT name, created_at FROM trackers ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
        .unwrap_or_else(|_| Vec::new());

        rows.iter().map(|row| row.get(0)).collect()
    }

    pub async fn validate_and_run(&mut self, args: Vec<String>) -> Result<String, ChronosError>{
        // make sure an actual command was given
        if args.len() < 2 {
            return Err(ChronosError::new(ChronosErrorTypes::NoArguments, "No arguments provided enter 'chronos help to see available commands'".to_string()));
        }

        // define messages
        let invalid_message = "Invalid command, enter 'chronos help' to get a list of all available commands";
        let mut message = "Success!".to_string();

        self.trackers = Self::get_tracker_list(&self.pool).await;

        // execute based on the command
        match &args[1][..] {
            "--version" => {
                // verify that no extra arguments where given
                if args.len() > 2 {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // return the help message
                message = VERSION.to_string();
            }

            "help" => {
                // verify that no extra arguments where given
                if args.len() > 2 {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // return the help message
                message = Self::help();
            },
            "list" => {
                // verify that no extra arguments where given
                if args.len() > 2 {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // get the tracker list
                let trackers = self.trackers.clone();

                // create the output message
                if trackers.len() == 1 {
                    message = format!("found 1 tracker: {}\n0\n", trackers[0]);
                } else {
                    // Create the output string for the  tracker list
                    let mut output_string: String = String::new();

                    // generate the tracker's list output
                    for index in 0..trackers.len() {
                        output_string.push_str(&format!("\n{}\t{}", trackers.len() - 1 - index, trackers[index]))
                    }

                    // return the final message
                    message = format!("found {} trackers: {}", trackers.len(), output_string);
                }
            },
            "create" => {
                // make sure the right amount of arguments are there
                if args.len() != 3 {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // execute the command
                match self.create_tracker(&mut args[2].clone()).await {
                    Ok(_) => message = format!("Created tracker {}", args[2]),
                    Err(error) => message = error.message,
                };
            },
            "delete" => {
                // verify the command
                if args.len() != 3 {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // run the command
                match self.delete_tracker(Some(args[2].clone())).await {
                    Ok(()) => message = format!("Deleted tracker {}", args[2]),
                    Err(error) => message = error.message,
                }
            },
            "toggle" => {
                // verify the command
                if args.len() != 3 {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                if !self.trackers.contains(&args[2]) {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, format!("Tracker {} does not exist", args[2])));
                }

                self.current_tracker = Some(args[2].clone());
                self.update_event_list().await;

                // run the commandSome(args[2])
                match self.toggle().await {
                    Ok(execution_message) => message = format!("Toggled tracker {}", args[2]),
                    Err(error) => message = error.message,
                }
            },
            "log" => {
                // verify the command
                if args.len() != 4 {
                    return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // update the variables
                self.current_tracker = Some(args[2].clone());
                self.update_event_list().await;

                // get the day selector
                let days_selector: usize = match &args[3].parse() {
                    Ok(days) => *days,
                    Err(_) => return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, format!("Invalid days selector {}", args[3]))),
                };

                // run the command
                match self.report(days_selector) {
                    Ok(execution_message) => message = execution_message,
                    Err(error) => message = error.message,
                }
            },
            _ => return Err(ChronosError::new(ChronosErrorTypes::CommandNotFound, format!("Command {} not found", args[1]))),
        }

        Ok(message)
    }

    async fn connect_to_database() -> Result<Pool<Sqlite>, ChronosError> {
        // Check if the database exists
        let mut database_exists = match Sqlite::database_exists(DATABASE_URL).await {
            Ok(exists) => exists,
            Err(error) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to check if database exists: {}", error))),
        };

        // Try to create database if missing
        if !database_exists {
            for _ in 0..3 {
                println!("Attempting to create database...");
                match Sqlite::create_database(DATABASE_URL).await {
                    Ok(_) => {
                        println!("Successfully created database");
                        database_exists = true;
                        break;
                    },
                    Err(error) => eprintln!("Failed to create database: {}", error),
                }
            }
        }

        // Panic if failed to create database
        if !database_exists {
            return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, "Failed to create database".to_string()));
        }

        // Try to connect to db.
        let mut last_result:Result<Pool<Sqlite>, ChronosError> = Err(ChronosError::new(ChronosErrorTypes::DatabaseError, "Did not try to connect to databse".to_string()));

        for _ in 0..3 {
            match SqlitePool::connect(&DATABASE_URL).await {
                Ok(pool) => {
                    last_result = Ok(pool);
                    break;
                },
                Err(error) => last_result = Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to connect to database: {}", error))),
            }
        }

        // return connection or error.
        last_result
    }

    // returns the help message
    fn help() -> String {
        String::from("Available commands:\
        help \t show this help message\
        list [tracker name] [amount of days]\t lists all current trackers\
        create [tracker name]\tcreates a new tracker\
        delete [tracker name]\tdeletes a tracker\
        toggle [tracker name]\ttoggles the tracker")
    }

    async fn list_from_pool(pool: &Pool<Sqlite>) -> Vec<String> {
        // get all the table names
        let rows = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY rowid DESC"
        )
            .fetch_all(pool)
            .await
            .unwrap_or_else(|_| Vec::new());

        // convert it into a Vec<String>
        rows
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .collect()
    }

    // function to toggle the tracker
    pub async fn toggle(&mut self) -> Result<(), ChronosError> {
        // get the current list of events
        let current = match self.current_events.clone() {
            Some(events) => events,
            None => return Err(ChronosError::new(ChronosErrorTypes::NoCurrentTracker, "No current tracker".to_string())),
        };

        // get the time stamp
        let timestamp = Self::get_timestamp();

        // create the event type based on the last event in the tracker's table
        let mut event = "start";

        let length = current.len();

        if length > 0 {
            if current.last().unwrap().0 == "start" {
                event = "stop";
            }
        }


        // add it to the tracker's table
        match query(AssertSqlSafe(format!("INSERT INTO {} (event_type, timestamp) VALUES (?, ?)", self.current_tracker.as_ref().unwrap())))
            .bind(event)
            .bind(timestamp)
            .execute(&self.pool)
            .await {
            Ok(_) => {},
            Err(error) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to add event to tracker: {}", error))),
        };

        self.update_event_list().await;
        Ok(())
    }

    // function that formats a timestamp
    fn convert_timestamp(mut timestamp: i64) -> String {
        // get individual units of time
        let seconds = timestamp % 60;
        timestamp /= 60;
        let minutes = timestamp % 60;
        timestamp /= 60;
        let hours = timestamp % 24;
        timestamp /= 24;
        let days = timestamp % 365;
        timestamp /= 365;

        // create the variable to store the output
        let mut output = String::new();

        // generate the output string
        if timestamp > 1 {
            output.push_str(format!("{} years, ", timestamp).as_str());
        }  else if timestamp > 0 {
            output.push_str("1 year, ");
        }

        if days > 1 {
            output.push_str(format!("{} days, ", days).as_str());
        } else if days > 0 {
            output.push_str("1 day, ");
        } else if timestamp > 0 {
            output.push_str("0 days, ");
        }

        if hours > 1 {
            output.push_str(format!("{} hours, ", hours).as_str());
        } else if days > 0 {
            output.push_str("1 hour, ");
        } else if timestamp > 0 || days > 0 {
            output.push_str("0 hours, ");
        }

        if minutes > 1 {
            output.push_str(format!("{} minutes, ", minutes).as_str());
        } else if days > 0 {
            output.push_str("1 minute, ");
        } else if timestamp > 0 || days > 0 || hours > 0{
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

    // function to update the list of events
    async fn update_event_list(&mut self) {
        // query the database for the events in the tracker's table
        let rows = query(AssertSqlSafe(format!("SELECT * FROM {} ORDER BY timestamp ASC", self.current_tracker.as_ref().unwrap())))
            .fetch_all(&self.pool)
            .await
            .unwrap();

        // convert the rows into a Vec<(i64, String)>
        let mut events: Vec<(String, i64)> = vec![];

        for event in rows {
            let timestamp = event.get::<i64, _>(0);
            let event_type = event.get::<String, _>(1);
            events.push((event_type, timestamp));
        }

        self.current_events = Some(events);
    }

    // create the trackers table
    async fn create_tracker_table(pool: &Pool<Sqlite>) -> Result<(), ChronosError> {
        // Create the table with the tracker's name
        match query("CREATE TABLE trackers (name TEXT NOT NULL, created_at INTEGER NOT NULL UNIQUE)")
            .execute(pool)
            .await {
            Ok(_) => Ok(()),
            Err(error) => Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to create tracker: {}", error))),
        }
    }

    // function to delete a tracker
    async fn delete_tracker(&mut self, tracker: Option<String>) -> Result<(), ChronosError> {
        // remove from the trackers table
        match query("DELETE FROM trackers WHERE name = ( ? )")
            .bind(tracker.clone().unwrap())
            .execute(&self.pool)
            .await {
            Ok(_) => {},
            Err(error) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to delete tracker: {}", error))),
        };

        // remove it's table
        match query(AssertSqlSafe(format!("DROP TABLE {}", tracker.clone().unwrap())))
            .execute(&self.pool)
            .await {
            Ok(_) => {},
            Err(error) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to delete tracker: {}", error))),
        };

        // update the list of trackers
        self.trackers = Self::get_tracker_list(&self.pool).await;

        return Ok(());

    }

    fn exit(&mut self) {
        self.exit = true;
    }


    // function to create a tracker
    async fn create_tracker(&mut self, tracker_name: &mut String) -> Result<(), ChronosError> {
        // make sure it doesn't already exist
        if self.trackers.contains(tracker_name) {
            return Err(ChronosError::new(ChronosErrorTypes::TrackerAlreadyExists, format!("Tracker {} already exists", tracker_name)));
        }

        // add it to the trackers table
        match query(AssertSqlSafe(format!("INSERT INTO trackers (name, created_at) VALUES ('{}', '{}')", tracker_name, Self::get_timestamp())))
            .execute(&self.pool)
            .await {
            Ok(_) => self.trackers = Self::get_tracker_list(&self.pool).await,
            Err(error) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to create tracker: {}", error))),
        }

        // create the table for the tracker
        match query(AssertSqlSafe(format!("CREATE TABLE {} (timestamp INTEGER NOT NULL UNIQUE, event_type TEXT NOT NULL)", tracker_name)))
            .execute(&self.pool)
            .await {
            Ok(_) => self.current_tracker = Some(tracker_name.clone()),
            Err(error) => self.tui_state = return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to create tracker: {}", error))),
        }

        // update the input
        tracker_name.clear();

        Ok(())
    }

    // function to get the current timestamp
    fn get_timestamp() -> i64 {
        match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs() as i64,
            Err(_) => 0,
        }
    }

    // function to get the current time by day
    fn get_time_by_day(&self) -> Vec<(i64, String, i32)> {
        // initialize the variables needed to calculate the time by day
        let mut timestamp = Self::get_timestamp() - 24 * 60 * 60;

        let mut time_by_day: Vec<(i64, String, i32)> = vec![];

        let mut current_day: i64 = 0;
        let mut last_start = 0;
        let mut day_index = 1;

        // loop through the events in the tracker's table and calculate the time by day
        match &self.current_events {
            Some(sessions) => {
                for session in sessions {
                    if session.0 == "start" && session.1 > timestamp {
                        last_start = session.1;

                    } else if  session.1 == sessions.last().unwrap().1 && session.0 == "stop"{
                        current_day += session.1 - last_start;
                        time_by_day.push((current_day, format!("Day {}", day_index), day_index - 1));
                    } else if session.0 == "stop"{
                        current_day += session.1 - last_start;
                    } else {
                        last_start = session.1;
                        timestamp -= 24 * 60 * 60;
                        day_index += 1;
                    }
                }

                if sessions.last().unwrap().0 == "start" {
                    current_day += Self::get_timestamp() - last_start;
                    time_by_day.push((current_day, format!("Day {}", day_index), day_index - 1));
                }
            }
            None => {},
        };

        time_by_day.reverse();
        time_by_day
    }

    // function to get the report for a specific amout days since now
    fn report(&self, day_selector: usize) -> Result<String, ChronosError> {
        let mut time_by_day = self.get_time_by_day();

        let mut time = 0;
        let mut days = 0;

        if day_selector < time_by_day.len() {
            days = time_by_day.len() - day_selector;
        }

        // filter the time by day to get the report

        for day in &mut time_by_day {
            if day.2 < days as i32 {
                continue;
            }

            time += day.0;
        }

        Ok(Self::convert_timestamp(time))
    }

    // function to select a tracker
    async fn select_tracker(&mut self, tracker_name: &String) {
        if self.trackers.contains(tracker_name) {
            self.current_tracker = Some(tracker_name.clone());
            self.update_event_list().await;
        }
    }

    // function to run the TUI
    pub async fn tui(&mut self) -> Result<(), ()> {
        color_eyre::install();
        let mut terminal = ratatui::init();

        let result = self.run_tui(terminal).await;

        ratatui::restore();
        result
    }

    // function that runs the main TUI loop

    pub async fn run_tui(&mut self, mut terminal:DefaultTerminal) -> Result<(), ()> {
        // create the states
        let mut list_state = ListState::default().with_selected(None);
        let mut input = String::new();
        let mut event_list_state = ListState::default().with_selected(None);
        let mut graph_list_state = ListState::default().with_selected(None);

        loop {
            if self.exit {
                break;
            }

            // Rendering
            terminal.draw(|f| {
                self.render(f, &mut list_state, &mut input, &mut event_list_state, &mut graph_list_state);
            }).unwrap();

            // Poll for input with a 1-second timeout so the clock redraws every second
            if event::poll(std::time::Duration::from_millis(200)).expect("Error polling events") {
                if let Event::Key(key) = event::read().expect("Error reading events") {
                    self.handle_key_events(key, &mut list_state, &mut input, &mut event_list_state, &mut graph_list_state).await;
                }
            }
        }

        Ok(())
    }

    fn render(&mut self, frame: &mut Frame<'_>, list_state: &mut ListState, input: &mut String, event_list_state: &mut ListState, graph_list_state: &mut ListState) {
        match self.tui_state {
            ChronosTuiState::Home => {
                let horizontal = Layout::horizontal([Constraint::Percentage(100)]);

                let [first_area] = frame.area().layout(&horizontal);

                self.render_home(frame, first_area);
            },
            ChronosTuiState::TrackerSelection => {
                self.render_tracker_selection(frame, list_state);
            }
            ChronosTuiState::CreateTracker => {
                self.render_create_tracker(frame, input);
            }
            ChronosTuiState::Tracker => self.render_tracker(frame, event_list_state, graph_list_state),
            ChronosTuiState::Error => self.render_error(frame),
        }
    }

    fn render_error(&mut self, frame: &mut Frame<'_>){
        let horizontal = Layout::horizontal([Constraint::Percentage(100)]);
        let [first_area] = frame.area().layout(&horizontal);

        let error_message = Paragraph::new("An error occurred. Please try again.")
            .block(Block::default().borders(Borders::ALL).title("ERROR").title_bottom(Line::from(vec![
                " Back <ESC> ".into(),
            ])));
        frame.render_widget(error_message, first_area);
    }

   fn render_tracker(&mut self, frame: &mut Frame<'_>, event_list_state: &mut ListState, graph_list_state: &mut ListState){

       let mut sessions: Vec<(i64, i64)> = vec![];
       let mut last_start = 0;
       let mut is_active = false;


       match &self.current_events {
           Some(events) => {
               for event in events {
                   if event.0 == "start" {
                       last_start = event.1;
                       is_active = true;
                   } else if event.0 == "stop"{
                       sessions.push((last_start, event.1 - last_start));
                       is_active = false;
                   }
               }
           }
           None => {}
       }

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(frame.area());
        let inner = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(30),
                Constraint::Percentage(70),
            ])
            .split(outer[1]);

       let mut bars: Vec<Bar> = vec![];


       let time_by_day = self.get_time_by_day();

       let mut max = time_by_day.iter().map(|t| t.0).max().unwrap_or(1);
       if max == 0 {
           max = 1;
       }

       let mut selector = 0;

       if let Some(selection) = graph_list_state.selected() {
           selector = selection;
       }

       let mut time = 0;

       for day in &time_by_day {
           if day.2 < selector as i32 {
               continue;
           }

           time += day.0;
           let bar = Bar::with_label(day.1.clone(), (day.0 * 100 / max) as u64).white();
           bars.push(bar);
       }

        let graph = BarChart::vertical(bars)
            .bar_width(5)
            .block(Block::default().borders(Borders::ALL).title("GRAPH").title_bottom(Line::from(vec![
                " Select More/Fewer days <Up> <Down>".into(),
            ])));
        frame.render_widget(graph, outer[0]);


        let clock = Paragraph::new(Self::convert_timestamp(time))
            .centered()
            .block(Block::default().borders(Borders::ALL).title("CLOCK").title_bottom(Line::from(vec![
                " Back <ESC> ".into(),
                "Start/Stop <ENTER> ".into(),
            ])));
        frame.render_widget(clock, inner[0]);

       let mut printable_events: Vec<String>  = vec![];

       for session in &sessions {
           let mut date_time = "".to_string();

           if let Some(datetime) = DateTime::from_timestamp(session.0, 0) {
               date_time = datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string();
           }

           printable_events.push(format!("{} - {}", date_time, Self::convert_timestamp(session.1)));
       }

       self.current_sessions = Some(printable_events.clone());

        let event_list = List::new(printable_events)
            .highlight_symbol("> ")
            .block(Block::default().borders(Borders::ALL).title("EVENT LIST").title_bottom(Line::from(vec![
                " Scroll Up/Down <Up> <Down> ".into(),
            ])));
       frame.render_stateful_widget(event_list, inner[1], event_list_state);;
    }

    // handle keyboard events
    async fn handle_key_events(&mut self, key_event: KeyEvent, list_state: &mut ListState, input: &mut String, event_list_state: &mut ListState, graph_list_state: &mut ListState) {
        match self.tui_state {
            ChronosTuiState::Home => self.handle_home_key_event(key_event),
            ChronosTuiState::TrackerSelection => self.handle_tracker_selection_key_events(key_event, list_state).await,
            ChronosTuiState::CreateTracker => self.handle_create_tracker_key_events(key_event, input).await,
            ChronosTuiState::Tracker => self.handle_tracker_key_events(key_event, event_list_state, graph_list_state).await,
            ChronosTuiState::Error => self.handle_error_key_events(key_event),
        }
    }

    async fn handle_tracker_key_events(&mut self, key_event: KeyEvent, event_list_state: &mut ListState, graph_list_state: &mut ListState) {
        let mut event_length = 0;
        let graph_length = self.get_time_by_day().len();

        match &self.current_sessions {
            Some(sessions) => {
                event_length = sessions.len();
            }
            None => {}
        }

        match key_event.code {
            KeyCode::Esc => {
                event_list_state.select(None);
                self.current_tracker = None;
                self.tui_state = ChronosTuiState::TrackerSelection;
            }
            KeyCode::Enter => {
                self.toggle().await;
                self.update_event_list().await;
            }
            KeyCode::Up => {
                let (selection, selected) = match event_list_state.selected() {
                    Some(selection) => (selection, true),
                    None => (0, false),
                };

                if selected {
                    if selection > 0 {
                        event_list_state.select(Some(selection.saturating_sub(1)));
                    }
                } else if event_length > 0 {
                    event_list_state.select(Some(0));
                }
            },
            KeyCode::Down => {
                let (selection, selected) = match event_list_state.selected() {
                    Some(selection) => (selection, true),
                    None => (0, false),
                };

                if selected {
                    if selection < event_length {
                        event_list_state.select(Some(selection.saturating_add(1)));
                    }
                } else if event_length > 0 {
                    event_list_state.select(Some(0));
                }
            },
            KeyCode::Left => {
                let (selection, selected) = match graph_list_state.selected() {
                    Some(selection) => (selection, true),
                    None => (0, false),
                };

                if selected {
                    if selection > 0 {
                        graph_list_state.select(Some(selection.saturating_sub(1)));
                    }
                } else if graph_length > 0 {
                    graph_list_state.select(Some(event_length - 1));
                }
            }
            KeyCode::Right => {
                let (selection, selected) = match graph_list_state.selected() {
                    Some(selection) => (selection, true),
                    None => (0, false),
                };

                if selected {
                    if selection < graph_length - 1 {
                        graph_list_state.select(Some(selection.saturating_add(1)));
                    }
                } else if graph_length > 0 {
                    graph_list_state.select(Some(0));
                }
            }
            _ => {}
        }
    }

    fn handle_error_key_events(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => {
                self.tui_state = ChronosTuiState::Home;
                self.error = None;
            },
            _ => {}
        }
    }

    fn handle_home_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => self.exit(),
            KeyCode::Char('t') => {
                if self.database_ok {
                    self.tui_state = ChronosTuiState::TrackerSelection;
                }
            }
            KeyCode::Char('c') => {
                if self.database_ok {
                    self.tui_state = ChronosTuiState::CreateTracker;
                }
            }
            _ => {}
        }
    }

    async fn handle_tracker_selection_key_events(&mut self, key_event: KeyEvent, list_state: &mut ListState) {
        match key_event.code {
            KeyCode::Esc => {
                list_state.select(None);
                self.tui_state = ChronosTuiState::Home
            },
            KeyCode::Up => {
                let (selection, selected) = match list_state.selected() {
                    Some(selection) => (selection, true),
                    None => (0, false),
                };

                if selected {
                    if selection > 0 {
                        list_state.select(Some(selection.saturating_sub(1)));
                        self.select_tracker(&self.trackers[selection].clone()).await;
                    }
                } else if self.trackers.len() > 0 {
                    list_state.select(Some(0));
                    self.select_tracker(&self.trackers.clone()[0]).await;
                }
            },
            KeyCode::Down => {
                let (selection, selected) = match list_state.selected() {
                    Some(selection) => (selection, true),
                    None => (0, false),
                };

                if selected {
                    if selection < self.trackers.len() {
                        list_state.select(Some(selection.saturating_add(1)));
                        self.select_tracker(&self.trackers.clone()[selection]).await;
                    }
                } else if self.trackers.len() > 0 {
                    list_state.select(Some(0));
                    self.select_tracker(&self.trackers.clone()[0]).await;
                }
            },
            KeyCode::Enter => {
                match list_state.selected() {
                    Some(selection) => {
                        let selected_tracker = self.trackers.get(selection).unwrap().clone();

                        self.current_tracker = Some(selected_tracker.clone());
                        self.tui_state = ChronosTuiState::Tracker;
                        self.update_event_list().await;
                    },
                    None => {}

                }
            }
            KeyCode::Delete => {
                match list_state.selected() {
                    Some(selection) => {
                        let selected_tracker = self.trackers.get(selection).unwrap().clone();

                        match self.delete_tracker(Some(selected_tracker.clone())).await {
                            Ok(()) => {
                                if self.trackers.len() > 0 {
                                    list_state.select(Some(0));
                                } else {
                                    list_state.select(None);
                                }
                            }
                            Err(error) => {
                                self.error = Some(error);
                                self.tui_state = ChronosTuiState::Error;
                            }
                        };
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn render_home(&self, frame: &mut Frame, area: Rect) {
        // Create the title of the window
        let title = Line::from(" CHRONOS ".bold());

        let mut instructions_vec:Vec<Span> = vec![];

        if self.database_ok {
            instructions_vec.push(" Select Tracker ".into());
            instructions_vec.push("<T>".into());
        }

        instructions_vec.push(" Quit ".into());
        instructions_vec.push("<ESC> ".bold());
        instructions_vec.push("Create Tracker ".into());
        instructions_vec.push("<C> ".into());

        // Add Instructions
        let instructions = Line::from(instructions_vec);

        // Create the main screen
        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        // Create the version message
        let message = match self.database_ok {
            true => Text::from(vec![Line::from(vec![
                "[CHRONOS]-V".into(),
                VERSION.to_string().red()
            ])]),
            false => Text::from(vec![Line::from(vec![
                "[CHRONOS]-V".into(),
                VERSION.to_string().red(),
                " (Database Error)".into()
            ])]),
        };

        // Add everything to the paragraph
        let paragraph = Paragraph::new(message)
            .centered()
            .block(block);

        frame.render_widget(paragraph, area);
    }

    fn render_tracker_selection(&self, frame: &mut Frame, list_state: &mut ListState) {
        let constraints = [
            Constraint::Percentage(100),
        ];

        let layout = Layout::vertical(constraints).spacing(1);
        let [top] = frame.area().layout(&layout);

        let list = List::new(self.trackers.clone())
            .style(Color::White)
            .block(Block::default().borders(Borders::ALL).title("TRACKERS").title_bottom(Line::from(vec![
                " Back <ESC> ".into(),
                "Scroll Up/Down <Up> <Down> ".into(),
                "Select <ENTER> ".into()
            ])))
            .highlight_style(Modifier::REVERSED)
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, top, list_state);
    }

    fn render_create_tracker(&self, frame: &mut Frame, input: &mut String) {

        let layout = Layout::new(Direction::Vertical, vec![
            Constraint::Min(3),
        ]);
        let [top] = frame.area().layout(&layout);

        let name = Paragraph::new(Line::from(vec![
            "Enter Tracker Name: ".into(),
            "\n".into(),
            input.clone().bold().underlined(),
        ]))
            .block(Block::default().borders(Borders::ALL).title("TRACKER CREATION").title_bottom(Line::from(vec![
                " Back <ESC> ".into(),
                "Create <ENTER> ".into()
            ])));

        frame.render_widget(name.centered().alignment(Alignment::Center), top);
    }

    async fn handle_create_tracker_key_events(&mut self, key_code: KeyEvent, input: &mut String) {
        match key_code.code {
            KeyCode::Esc => self.tui_state = ChronosTuiState::Home,
            KeyCode::Char(char) => input.push(char),
            KeyCode::Backspace => {
                input.pop();
            },
            KeyCode::Enter => {
                match self.create_tracker(input).await {
                    Ok(_) => self.tui_state = ChronosTuiState::TrackerSelection,
                    Err(e) => self.error = Some(e),
                };
            }
            _ => {},
        }
    }

}