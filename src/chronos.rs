use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use axum::routing::post;
use ratatui::{text::Line, widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, List, ListState, Paragraph}, symbols::Marker, style::Modifier, prelude::Direction, DefaultTerminal, Frame, layout::{Alignment, Constraint, Layout, Rect}};
use sqlx::{migrate::MigrateDatabase, query, Pool, Row, Sqlite, SqlitePool};
use crate::tracker::Tracker;
use chrono::{
    DateTime,
    Local,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use mime_guess::from_path;
use rust_embed::Embed;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

// Constants
const DATABASE_URL: &str = "sqlite://sqlite.db";
const HOST: &str = "127.0.0.1";
const PORT: &str = "8080";

// Embed to include the frontend in the binary
#[derive(Embed)]
#[folder = "frontend/"]
struct Assets;


// Enum for types of errors
pub enum ChronosErrorTypes {
    NoArguments,
    DatabaseError,
    InvalidCommand,
    NameAlreadyExists,
    TrackerAlreadyExists,
    NoCurrentTracker,
    WebAppError,
    ColorEyreInstallFailed,
}

// Enum for tui state
pub enum ChronosTuiState {
    Selection,
    Creation,
    Tracker,
    Error,
}

// error object
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

// Chronos struct
pub struct Chronos {
    // For the core functionality
    pool: Pool<Sqlite>,
    trackers: Vec<Tracker>,

    // For the Tui
    tui_state: ChronosTuiState,
    selected_tracker: Option<Tracker>,
    last_error: Option<ChronosError>,
    exit: bool,

    // For the webapp
    cancelation_token: tokio_util::sync::CancellationToken,
}

// impl block for the functions needed for chronos initiation
impl Chronos {
    // The new function
    pub async fn new() -> Result<Self, ChronosError> {
        // get the pool
        let pool = match Self::connect_to_database().await {
            Ok(pool) => pool,
            Err(e) => return Err(e)
        };

        // verify that the tracker table exists
        let tables = match Self::get_tables(&pool).await {
            Ok(tables) => tables,
            Err(e) => return Err(e)
        };

        // create the table if needed
        if !tables.contains(&"trackers".to_string()) {
            match Self::create_trackers_table(&pool).await {
                Ok(_) => {},
                Err(_) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, "Failed to create tracker table".to_string())),
            };
        }

        // get the trackers from the database
        let trackers = match Self::get_trackers(&pool).await {
            Ok(trackers) => trackers,
            Err(e) => return Err(e)
        };

        Ok(Self {
            pool,
            trackers,

            tui_state: ChronosTuiState::Selection,
            selected_tracker: None,
            last_error: None,
            exit: false,

            cancelation_token: tokio_util::sync::CancellationToken::new(),
        })
    }

    // function to connect to the database that returns a pool
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

        // attempt to connect to db 3 times
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

    // function to get all the tables (trackers)
    pub async fn get_tables(pool: &Pool<Sqlite>) -> Result<Vec<String>, ChronosError> {
        // gt the rows from the tracker tables
        let rows = match sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetch_all(pool).await {
            Ok(rows) => rows,
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to query database: {}", e)))
        };

        // extract the names
        let mut table_names: Vec<String> = Vec::new();
        for row in rows {
            table_names.push(row.get(0));
        }

        Ok(table_names)
    }

    // function to create the tracker tables which acts like a disc partition
    async fn create_trackers_table(pool: &Pool<Sqlite>) -> Result<(), ChronosError> {
        // Create the table with the tracker's name
        match query("CREATE TABLE trackers (name TEXT NOT NULL, created_at INTEGER NOT NULL UNIQUE)")
            .execute(pool)
            .await {
            Ok(_) => Ok(()),
            Err(error) => Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to create tracker: {}", error))),
        }
    }

    //function to get the trackers (name and created_at)
    async fn get_trackers(pool: &Pool<Sqlite>) -> Result<Vec<Tracker>, ChronosError> {
        // get the rows from the tracker table
        let rows = match query("SELECT * FROM trackers ORDER BY created_at DESC").fetch_all(pool).await {
            Ok(rows) => rows,
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::DatabaseError, format!("Failed to query database: {}", e)))
        };

        // extract the name and created_at field and the new traker in the output vec
        let mut trackers: Vec<Tracker> = vec![];
        for row in rows {
            let name = row.get(0);
            let created_at = row.get(1);

            trackers.push(Tracker::new(name, created_at, pool.clone()).await?);
        }

        Ok(trackers)
    }

    // function to update the list of trackers
    async fn update_trackers(&mut self) -> Result<(), ChronosError> {
        self.trackers = match Chronos::get_trackers(&self.pool).await {
            Ok(trackers) => trackers,
            Err(e) => return Err(e)
        };
        Ok(())
    }
}

// impl block for chore features
impl Chronos {
    // function to create a tracker
    pub async fn create_tracker(&mut self, name: &str) -> Result<(), ChronosError> {
        // check if tracker already exists
        let contained = match Self::get_trackers(&self.pool).await {
            Ok(trackers) => {
                let mut is_contained = false;

                for tracker in trackers {
                    if tracker.name == name.to_string() {
                        is_contained = true;
                    }
                }
                is_contained
            }
            Err(e) => return Err(e)
        };
        if !contained {
            return Err(ChronosError::new(ChronosErrorTypes::NameAlreadyExists, format!("Tracker \"{}\" already exists", name)));
        }

        // Create tracker
        match Tracker::new(name, chrono::Utc::now().timestamp(), self.pool.clone()).await {
            Ok(_) => match self.update_trackers().await {
                Ok(_) => (),
                Err(e) => return Err(e),
            },
            Err(e) => return Err(e),
        };
        Ok(())
    }
}

// impl block for commands
impl Chronos {
    // fucntion to match the args parameter and execute the command given if possible
    pub async fn run_command(&mut self, args: Vec<String>) -> Result<String, ChronosError> {
        match args.len() {
            0 | 1 => Err(ChronosError::new(ChronosErrorTypes::NoArguments, "No arguments provided".to_string())),
            2 => {
                if args[1] == "list" {
                    // Generate the output of all the trackers
                    let mut output = String::from("Trackers:\n");

                    for tracker in &self.trackers {
                        output.push_str(&format!("{} - Created at {}\n", tracker.name, tracker.created_at));
                    }

                    return Ok(output);
                }

                Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, "Invalid command".to_string()))
            },
            3 => {
                match &args[1][..] {
                    "create" => {
                        // make sure the name is not already taken
                        if self.trackers.iter().any(|tracker| tracker.name == args[2]) {
                            return Err(ChronosError::new(ChronosErrorTypes::TrackerAlreadyExists, format!("Tracker {} already exists", args[2])));
                        }

                        // create the traker
                        match self.create_tracker(&args[2]).await {
                            Ok(_) => (),
                            Err(e) => return Err(e),
                        };

                        // update the trackers
                        match self.update_trackers().await {
                            Ok(_) => (),
                            Err(e) => return Err(e),
                        };

                        Ok(format!("Tracker {} created", args[2]))
                    }
                    "delete" => {
                        // get the wanted tracker
                        let mut current_tracker: Option<Tracker> = None;
                        for tracker in &self.trackers {
                            if tracker.name == args[2] {
                                current_tracker = Some(tracker.clone());
                            }
                        }

                        // if a tracker is found, delete it
                        match current_tracker {
                            Some(mut tracker) => {
                                match tracker.delete().await {
                                    Ok(_) => (),
                                    Err(e) => return Err(e),
                                }
                            }
                            None => return Err(ChronosError::new(ChronosErrorTypes::NoCurrentTracker, format!("No tracker named {} found", args[2]))),
                        };

                        // update the trackers
                        match self.update_trackers().await {
                            Ok(_) => (),
                            Err(e) => return Err(e),
                        }

                        Ok(format!("Tracker {} deleted", args[2]))
                    }
                    "toggle" => {
                        // get the tarcker
                        let mut current_tracker: Option<Tracker> = None;
                        for tracker in &self.trackers {
                            if tracker.name == args[2] {
                                current_tracker = Some(tracker.clone());
                            }
                        }

                        // if a tracker is selected, try to toggle it
                        match current_tracker {
                            Some(mut tracker) => {
                                match tracker.toggle().await {
                                    Ok(_) => (),
                                    Err(e) => return Err(e),
                                }
                            }
                            None => return Err(ChronosError::new(ChronosErrorTypes::NoCurrentTracker, format!("No tracker named {} found", args[2]))),
                        };

                        Ok(format!("Tracker {} toggled", args[2]))
                    }
                    _ => Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, "Invalid command".to_string())),
                }
            }
            4 => match &args[1][..] {
                "report" => {
                    // parse the day amount from the args
                    let days: usize = match args[3].parse() {
                        Ok(days) => days,
                        Err(_) => return Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, "Invalid number of days".to_string())),
                    };

                    // get the tracker
                    let mut current_tracker: Option<Tracker> = None;
                    for tracker in &self.trackers {
                        if tracker.name == args[2] {
                            current_tracker = Some(tracker.clone());
                        }
                    }

                    // if there is a tracker, report the time
                    match current_tracker {
                        Some(tracker) => Ok(format!("Report for {} days: {}", days, tracker.report(days))),
                        None => Err(ChronosError::new(ChronosErrorTypes::NoCurrentTracker, format!("No tracker named {} found", args[2]))),
                    }
                }
                _ => Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, "Invalid command".to_string())),
            }
            _ => Err(ChronosError::new(ChronosErrorTypes::InvalidCommand, "Invalid command".to_string())),
        }
    }

    // function to format a timestamp into a datetime format
    fn get_date_time_from_timestamp(timestamp: i64) -> String {
        let date_time = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| DateTime::default());

        let local_date_time: DateTime<Local> = DateTime::from(date_time);

        local_date_time.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

// impl block for the tui
impl Chronos {
    // Intital function to run the tui
    pub async fn tui(&mut self) -> Result<(), ChronosError>{
        // install color_eyre
        match color_eyre::install() {
            Ok(_) => (),
            Err(_) => return Err(ChronosError::new(ChronosErrorTypes::ColorEyreInstallFailed, "Failed to install color-eyre".to_string())),
        };

        // get the terminal
        let mut terminal = ratatui::init();

        // run the tui and store the result for later return
        let result = self.run_tui(&mut terminal).await;

        // restore the terminal
        ratatui::restore();

        result
    }

    // function to exit the tui
    fn exit_tui(&mut self) {
        self.exit = true;
    }

    // function to handle rendering and jey events
    async fn run_tui(&mut self, terminal: &mut DefaultTerminal) -> Result<(), ChronosError> {
        // initialize the list states
        let mut selection_list_state = ListState::default().with_selected(None);
        let mut session_list_state = ListState::default().with_selected(None);
        let mut graph_list_state = ListState::default().with_selected(None);

        // intitialize the variable that holds the input
        let mut input = String::new();

        // main loop
        loop {
            // ecit if prompted to
            if self.exit {
                break;
            }

            // render the tui
            terminal.draw(|frame|  {
                self.render_tui(frame, &mut selection_list_state, &mut session_list_state, &mut graph_list_state, &mut input);
            }).unwrap();

            // handle the key events with polling for clock rendering
            if match event::poll(std::time::Duration::from_millis(100)) {
                Ok(true) => true,
                Ok(false) => false,
                Err(_) => false,
            } {
                if let Event::Key(key) = match event::read() {
                    Ok(key) => key,
                    Err(_) => continue,
                } {
                    self.handle_key_event(key, &mut selection_list_state, &mut session_list_state, &mut graph_list_state, &mut input).await;
                }
            }

        }

        Ok(())
    }

    // function that handles the rendering of the tui like a dispatch
    fn render_tui(&mut self, frame: &mut Frame, selection_list_state: &mut ListState, session_list_state: &mut ListState, graph_list_state: &mut ListState, input: &mut String) {
        match self.tui_state {
            ChronosTuiState::Selection => self.render_selection_tui(frame, selection_list_state),
            ChronosTuiState::Creation => self.render_creation_tui(frame, input),
            ChronosTuiState::Tracker => self.render_tracker_tui(frame, graph_list_state, session_list_state),
            ChronosTuiState::Error => self.render_error_tui(frame),
        }
    }

    // function to render the tracker selection page
    fn render_selection_tui(&mut self, frame: &mut Frame, selection_list_state: &mut ListState) {
        // define the constraints
        let horizontal_constraints = [
            Constraint::Percentage(75),
            Constraint::Percentage(25),
        ];

        // define the layout
        let horizontal_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(horizontal_constraints)
            .split(frame.area());

        // render the widgets
        self.render_tracker_list_widget(frame, horizontal_layout[0], selection_list_state);
        self.render_clock_widget(frame, horizontal_layout[1]);
    }

    // function that renders the tracker creation tui
    fn render_creation_tui(&mut self, frame: &mut Frame, input: &mut String) {
        // define the block
        let block = Block::default()
            .title(" CHRONOS ")
            .title(" Tracker Creation ")
            .title_style(Modifier::REVERSED)
            .title_bottom(Line::from(vec![
                " Exit <ESC> ".into(),
                " Create Tracker <DEL> ".into(),
            ]))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        // define the paragraph
        let paragraph = Paragraph::new(Line::from(vec![
            " Tracker Name: ".into(),
            input.clone().into(),
        ]))
            .block(block)
            .alignment(Alignment::Center);

        // render the paragraph
        frame.render_widget(paragraph, frame.area());
    }

    // function to render the tracker tui
    fn render_tracker_tui(&mut self, frame: &mut Frame, graph_list_state: &mut ListState, session_list_state: &mut ListState) {
        let vertical_constraints = [
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ];

        let horizontal_constraints = [
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ];

        let second_horizontal_constraint = [
            Constraint::Percentage(80),
            Constraint::Percentage(20),
        ];

        let vertical_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vertical_constraints)
            .split(frame.area());

        let horizontal_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(horizontal_constraints)
            .split(vertical_layout[0]);

        let second_horizontal_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(second_horizontal_constraint)
            .split(vertical_layout[1]);

        self.render_stateful_clock_widget(frame, horizontal_layout[0], graph_list_state);
        self.render_session_list_widget(frame, horizontal_layout[1], session_list_state, graph_list_state);
        self.render_graph_widget(frame, second_horizontal_layout[0], graph_list_state);
        self.render_day_selector_widget(frame, second_horizontal_layout[1], graph_list_state);
    }

    // function to render the error tui
    fn render_error_tui(&mut self, frame: &mut Frame) {
        let block = Block::default()
            .title(" ERROR ")
            .title_style(Modifier::REVERSED)
            .title_bottom(Line::from(vec![
                " Back <ESC> ".into(),
            ]))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let paragraph = Paragraph::new(Line::from(vec![
            match self.last_error {
                Some(ref e) => format!("{}",e.message),
                None => String::from(""),
            }.into()
        ]))
            .block(block)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, frame.area());
    }

    // function that renders the tracker selection list widget
    fn render_tracker_list_widget(&mut self, frame: &mut Frame, area: Rect, list_state: &mut ListState) {
        // generate the list
        let mut tracker_list: Vec<String> = vec![];
        for tracker in &self.trackers {
            tracker_list.push(format!("{} - Created {}", tracker.name, Self::get_date_time_from_timestamp(tracker.created_at)));
        }

        let tracker_selection_list_block = Block::default()
            .title(" CHRONOS ")
            .title(" Tracker Selection ")
            .title_style(Modifier::REVERSED)
            .title_bottom(Line::from(vec![
                " Exit <ESC> ".into(),
                " Create Tracker <C> ".into(),
                " Delete Tracker <DEL> ".into(),
                " Select Tracker <ENTER> ".into(),
            ]))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let tracker_selection_list = List::new(tracker_list)
            .block(tracker_selection_list_block)
            .highlight_style(Modifier::REVERSED)
            .highlight_symbol("> ");

        frame.render_stateful_widget(tracker_selection_list, area, list_state);
    }

    // function that renders the clock
    fn render_clock_widget(&mut self, frame: &mut Frame, area: Rect) {
        let clock_block = Block::default()
            .title(" Clock ")
            .title_style(Modifier::REVERSED)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let mut clock = String::from("0 seconds ");

        match &self.selected_tracker {
            Some(selected_tracker) => {
                clock = selected_tracker.report(selected_tracker.time_per_day.len())
            },
            None => (),
        };

        let paragraph = Paragraph::new(clock)
            .block(clock_block)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    // function that renders the clock with the graph list state for day selection
    fn render_stateful_clock_widget(&mut self, frame: &mut Frame, area: Rect, list_state: &mut ListState) {
        let clock_block = Block::default()
            .title(" Clock ")
            .title_bottom(Line::from(vec![
                " Toggle <ENTER> ".into(),
            ]))
            .title_style(Modifier::REVERSED)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let mut clock = String::from("0 seconds ");

        match &self.selected_tracker {
            Some(selected_tracker) => {
                match list_state.selected() {
                    Some(selection) => clock = selected_tracker.report(selection),
                    None => clock = selected_tracker.report(selected_tracker.time_per_day.len())
                }
            },
            None => (),
        };

        let paragraph = Paragraph::new(clock)
            .block(clock_block)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    // function that renders the sessions as a list takng the graph list state for day selection
    fn render_session_list_widget(&mut self, frame: &mut Frame, area: Rect, list_state: &mut ListState, graph_list_state: &mut ListState) {
        let mut session_list: Vec<String> = vec![];

        let (selection, selected) = match graph_list_state.selected() {
            Some(selected) => (selected, true),
            None => (0, false),
        };

        match &self.selected_tracker {
            Some(tracker) => {
                for session in &tracker.sessions {
                    if session.start_time > tracker.created_at + (selection as i64 * 24 * 60 * 60) && selected {
                        continue;
                    }
                    session_list.push(format!("Session on {}, lasted {}", Self::get_date_time_from_timestamp(session.start_time), Tracker::format_epoch(session.duration)));
                }
            }
            None => (),
        };

        let list_block = Block::default()
            .title(" Sessions ")
            .title_bottom(Line::from(vec![
                " Scroll Up <UP> ".into(),
                " Scroll Down <DOWN> ".into(),
            ]))
            .title_style(Modifier::REVERSED)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let list = List::new(session_list)
            .block(list_block)
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, list_state);
    }

    // function to render the graph
    fn render_graph_widget(&mut self, frame: &mut Frame, area: Rect, graph_list_state: &mut ListState) {
        let mut graph_list: Vec<(f64, f64)> = vec![];

        match self.selected_tracker.clone() {
            Some(tracker) => {
                for (index, day) in tracker.time_per_day.iter().enumerate() {
                    graph_list.push((index as f64, day.time as f64));
                }
            }
            None => (),
        }

        let mut max_day_index = 0.0;
        let mut max_duration = 0.0;

        let (selection, selected) = match graph_list_state.selected() {
            Some(selected) => (selected, true),
            None => (0, false),
        };

        for data in &graph_list {
            if data.0 > (selection as f64) - 1f64 && selected {
                continue;
            }

            if data.0 > max_day_index {
                max_day_index = data.0;
            }
            if data.1 > max_duration {
                max_duration = data.1;
            }
        }

        let mut x_labels: Vec<String> = vec![];
        let mut y_labels: Vec<String> = vec![];

        for i in 0..9 {
            x_labels.push((max_day_index / 10f64 * i as f64).round().to_string());
        }
        x_labels.push(max_day_index.to_string());

        for i in 0..9  {
            y_labels.push((max_duration / 10f64 * i as f64).round().to_string());
        }

        y_labels.push(max_duration.to_string());

        if graph_list.len() < 2 {
            max_day_index += 1.0;
            graph_list.push((max_day_index, 0.0));
        }

        let dataset = Dataset::default()
            .name("Time Per Day")
            .marker(Marker::Bar)
            .graph_type(GraphType::Bar)
            .data(&graph_list);

        let x_axis = Axis::default()
            .title("Day")
            .bounds([0.0, max_day_index])
            .labels(x_labels);

        let y_axis = Axis::default()
            .title("Time")
            .bounds([0.0, max_duration])
            .labels(y_labels);

        let block = Block::default()
            .title(" Graph ")
            .title_bottom(Line::from(vec![
                " Select More Days <RIGHT> ".into(),
                " Select Less Days <RIGHT> ".into(),
                " Back <ESC> ".into(),
            ]))
            .title_style(Modifier::REVERSED)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let chart = Chart::new(vec![dataset])
            .x_axis(x_axis)
            .y_axis(y_axis)
            .block(block);

        frame.render_widget(chart, area);
    }

    // function that renders the amount of days selected
    fn render_day_selector_widget(&mut self, frame: &mut Frame, area: Rect, graph_list_state: &mut ListState) {
        let text = match graph_list_state.selected() {
            Some(1) => "Currently viewing 1 day".to_string(),
            Some(selected) => format!("Currently viewing {} days", selected),
            None => String::from("Currently viewing All days"),
        };

        let block = Block::default()
            .title(" Day ")
            .title_style(Modifier::REVERSED)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let paragraph = Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    // function to dispatch the key events
    async fn handle_key_event(&mut self, key: KeyEvent, selection_list_state: &mut ListState, session_list_state: &mut ListState, graph_list_state: &mut ListState, input: &mut String) {
        match self.tui_state {
            ChronosTuiState::Selection => {
                match self.handle_tracker_selection_keys(key, selection_list_state).await {
                    Ok(_) => (),
                    Err(e) => {
                        self.last_error = Some(e);
                        self.tui_state = ChronosTuiState::Error;
                    },
                }
            }
            ChronosTuiState::Creation => match self.handle_creation_keys(key, input).await {
                Ok(_) => (),
                Err(e) => {
                    self.last_error = Some(e);
                    self.tui_state = ChronosTuiState::Error;
                },
            },
            ChronosTuiState::Tracker => match self.handle_tracker_keys(key, graph_list_state, session_list_state).await {
                Ok(_) => (),
                Err(e) => {
                    self.last_error = Some(e);
                    self.tui_state = ChronosTuiState::Error;
                },
            }
            ChronosTuiState::Error => if key.code == KeyCode::Esc {
                self.tui_state = ChronosTuiState::Selection;
            },
        }
    }

    // function to handle the tracker selection key events
    async fn handle_tracker_selection_keys(&mut self, key: KeyEvent, list_state: &mut ListState) -> Result<(), ChronosError> {
        match key.code {
            KeyCode::Esc => {
                self.exit_tui();
                Ok(())
            },
            KeyCode::Up => {
                let (selection, selected) = match list_state.selected() {
                    Some(selected) => (selected, true),
                    None => (0, false),
                };

                if selection > 0 {
                    list_state.select(Some(selection - 1));
                    self.selected_tracker = Some(self.trackers[selection - 1].clone());
                } else if !selected && self.trackers.len() > 0 {
                    list_state.select(Some(0));
                    self.selected_tracker = Some(self.trackers[0].clone());
                }
                Ok(())
            }
            KeyCode::Down => {
                let (selection, selected) = match list_state.selected() {
                    Some(selected) => (selected, true),
                    None => (0, false),
                };

                if self.trackers.len() > 0 {
                    if selection < self.trackers.len() - 1 {
                        list_state.select(Some(selection + 1));
                        self.selected_tracker = Some(self.trackers[selection + 1].clone())
                    } else if !selected && self.trackers.len() > 0 {
                        list_state.select(Some(0));
                        self.selected_tracker = Some(self.trackers[0].clone())
                    }
                } else {
                    list_state.select(None);
                    self.selected_tracker = None;
                }
                Ok(())

            }
            KeyCode::Delete => {
                match self.selected_tracker.clone() {
                    Some(mut selected_tracker) => {
                        match selected_tracker.delete().await {
                            Ok(_) => (),
                            Err(e) => return Err(e),
                        };
                    },
                    None => (),
                }

                let (selection, selected) = match list_state.selected() {
                    Some(selected) => (selected, true),
                    None => (0, false),
                };

                match self.update_trackers().await {
                    Ok(_) => (),
                    Err(e) => return Err(e),
                }

                if selected && self.trackers.len() > 0 {
                    if selection > 0 {
                        list_state.select(Some(selection - 1));
                        self.selected_tracker = Some(self.trackers[selection - 1].clone());
                    } else {
                        list_state.select(Some(0));
                        self.selected_tracker = Some(self.trackers[0].clone());
                    }

                } else {
                    list_state.select(None);
                    self.selected_tracker = None;
                }

                Ok(())
            }
            KeyCode::Char('c') => {
                self.tui_state = ChronosTuiState::Creation;
                Ok(())
            }
            KeyCode::Enter => {
                match list_state.selected(){
                    Some(selection) => {
                        self.selected_tracker = Some(self.trackers[selection].clone());
                        self.tui_state = ChronosTuiState::Tracker;
                        Ok(())
                    }
                    None => Ok(())
                }
            }
            _ => Ok(())
        }
    }

    // function to handle the tracker creation key events
    async fn handle_creation_keys(&mut self, key: KeyEvent, input: &mut String) -> Result<(), ChronosError> {
        match key.code {
            KeyCode::Esc => {
                self.tui_state = ChronosTuiState::Selection;
                input.clear();
                Ok(())
            },
            KeyCode::Backspace => {
                if input.len() > 0 {
                    input.pop();
                }
                Ok(())
            }
            KeyCode::Char(character) => {
                input.push(character);
                Ok(())
            }
            KeyCode::Enter => {
                if match Self::get_tables(&self.pool).await {
                    Ok(trackers) => trackers,
                    Err(e) => return Err(e),
                }.contains(&input) {
                    return Err(ChronosError::new(ChronosErrorTypes::NameAlreadyExists, format!("Tracker {} already exists", input)));
                }

                match Tracker::new(&input.clone(), chrono::Utc::now().timestamp(), self.pool.clone()).await {
                    Ok(_) => {
                        input.clear();
                        match self.update_trackers().await {
                            Ok(_) => {
                                self.tui_state = ChronosTuiState::Selection;
                                Ok(())
                            },
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            _ => Ok(()),
        }
    }

    // function to handle the tracker key events
    async fn handle_tracker_keys(&mut self, key: KeyEvent, graph_list_state: &mut ListState, session_list_state: &mut ListState) -> Result<(), ChronosError> {
        match key.code {
            KeyCode::Esc => {
                self.tui_state = ChronosTuiState::Selection;
                Ok(())
            }
            KeyCode::Enter => {
                match self.selected_tracker.clone() {
                    Some(mut tracker) => match tracker.toggle().await {
                        Ok(_) => {
                            match self.update_trackers().await {
                                Ok(_) => {
                                    self.selected_tracker = Some(self.trackers.iter().find(|t| t.name == tracker.name).unwrap().clone());
                                    Ok(())
                                },
                                Err(e) => Err(e),
                            }

                        },
                        Err(e) => Err(e),
                    }
                    None => Ok(())
                }
            }
            KeyCode::Up => {
                let (selection, selected) = match session_list_state.selected() {
                    Some(selected) => (selected, true),
                    None => (0, false),
                };

                let session_number = match self.selected_tracker.clone() {
                    Some(tracker) => tracker.sessions.len(),
                    None => 0
                };

                if selection > 0 {
                    session_list_state.select(Some(selection - 1));
                } else if !selected && session_number > 0 {
                    session_list_state.select(Some(0));
                }
                Ok(())
            }
            KeyCode::Down => {
                let (selection, selected) = match session_list_state.selected() {
                    Some(selected) => (selected, true),
                    None => (0, false),
                };

                let session_number = match self.selected_tracker.clone() {
                    Some(tracker) => tracker.sessions.len(),
                    None => 0
                };

                if self.trackers.len() > 0 {
                    if selection < session_number - 1 {
                        session_list_state.select(Some(selection + 1));
                    } else if !selected && session_number > 0 {
                        session_list_state.select(Some(0));
                    }
                } else {
                    session_list_state.select(None);
                }
                Ok(())
            }
            KeyCode::Right => {
                let (selection, selected) = match graph_list_state.selected() {
                    Some(selected) => (selected, true),
                    None => (0, false),
                };

                let days_number = match self.selected_tracker.clone() {
                    Some(tracker) => tracker.time_per_day.len() - 1,
                    None => 0
                };

                if days_number > 0 {
                    if selection <= days_number {
                        graph_list_state.select(Some(selection + 1));
                    } else if !selected && days_number > 0 {
                        graph_list_state.select(Some(0));
                    }
                } else {
                    graph_list_state.select(None);
                }
                Ok(())
            }
            KeyCode::Left => {

                let (selection, selected) = match graph_list_state.selected() {
                    Some(selected) => (selected, true),
                    None => (0, false),
                };

                let days_number = match self.selected_tracker.clone() {
                    Some(tracker) => tracker.time_per_day.len(),
                    None => 0
                };

                if selection > 0 {
                    graph_list_state.select(Some(selection - 1));
                } else if !selected && days_number > 0 {
                    graph_list_state.select(Some(0));
                }
                Ok(())
            }
            _ => Ok(())
        }
    }
}

// impl block for the web app
impl Chronos {
    // function to serve the imbeded forntend
    async fn serve_embed(path: &str) -> Response {
        match Assets::get(path) {
            Some(content) => {
                let mime = from_path(&path).first_or_octet_stream();
                ([("content-type", mime.as_ref())], content.data).into_response()
            }
            None => (StatusCode::NOT_FOUND, "404 Not Found").into_response()
        }
    }

    pub async fn run_web_app(&mut self) -> Result<(), ChronosError> {
        // create the bind address
        let bind_adress = format!("{}:{}", HOST, PORT);

        // start the tcp listener
        let listener = match TcpListener::bind(&bind_adress).await {
            Ok(listener) => listener,
            Err(e) => return Err(ChronosError::new(ChronosErrorTypes::WebAppError, "Failed to start web server".to_string()))
        };

        // Attempt to open the web app in a browser
        if let Err(e) = open::that(format!("http://{}", bind_adress)) {
            return Err(ChronosError::new(ChronosErrorTypes::WebAppError, format!("Failed to open web app: {}", e)));
        }

        // create the router
        let router = self.build_router();

        let token = self.cancelation_token.clone();
        // start the axum webserver
        match axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                token.cancelled().await;
            })
            .await {
            Ok(_) => Ok(()),
            Err(e) => Err(ChronosError::new(ChronosErrorTypes::WebAppError, format!("Failed to start web server: {}", e)))
        }
    }

    // function to build the router
    fn build_router(&mut self) -> Router {
        let token = self.cancelation_token.clone();
        Router::new()
            .route("/kill", post(move || async move {
                token.cancel();
                (StatusCode::OK, "Killed").into_response()
            }))
            .fallback(Self::frontend_handler)
    }

    // function to handle serving the frontend
    async fn frontend_handler(uri: Uri) -> Response {
        // get the path
        let path = uri.path().trim_start_matches("/");

        // Fallback to index.html for empty paths or paths not in the bundle.
        let resolved = if path.is_empty() || Assets::get(path).is_none() {
            "index.html"
        } else {
            path
        };

        Self::serve_embed(resolved).await
    }
}