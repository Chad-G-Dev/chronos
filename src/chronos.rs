use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};
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
const VERSION: &str = "1.0.0";

pub enum ErrorTypes {
    NoArguments,
    DatabaseError,
    CommandNotFound,
    InvalidCommand,
    NameAlreadyExists,
}

pub struct ChronosError {
    pub error_type: ErrorTypes,
    pub message: String,
}

impl ChronosError {
    pub fn new(error_type: ErrorTypes, message: String) -> ChronosError {
        ChronosError {
            error_type,
            message
        }
    }
}

/// ================================================================================================
/// Struct definition
/// ================================================================================================
#[derive(Debug)]
pub struct Chronos {
    pub pool: Pool<Sqlite>,
    pub trackers: Vec<String>,
    pub exit: bool,
}

// TUI
impl Widget for &Chronos {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Create the title of the window
        let title = Line::from(" CHRONOS ".bold());

        // Add Instructions
        let instructions = Line::from(vec![" Quit ".into(), "<Q> ".bold()]);

        // Cretae the main screen
        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        // Create the version message
        let message = Text::from(vec![Line::from(vec![
            "[CHRONOS]-V".into(),
            VERSION.to_string().red()
        ])]);

        // Add everything to the paragraph
        Paragraph::new(message)
            .centered()
            .block(block)
            .render(area, buf);
    }
}

impl Chronos {
    pub async fn new() -> Result<Self, ChronosError> {
        let pool = match Self::connect_to_database().await {
            Ok(pool) => pool,
            Err(error) => return Err(error),
        };

        // get the trackers list
        let trackers = Self::list_from_pool(&pool).await;

        // return a new Chronos object
        Ok(Self {
            pool,
            trackers,
            exit: false,
        })
    }

    pub async fn validate_and_run(&mut self, args: Vec<String>) -> Result<String, ChronosError>{
        // make sure an actual command was given
        if args.len() < 2 {
            return Err(ChronosError::new(ErrorTypes::NoArguments, "No arguments provided enter 'chronos help to see available commands'".to_string()));
        }

        // define messages
        let invalid_message = "Invalid command, enter 'chronos help' to get a list of all available commands";
        let mut message = "Success!".to_string();

        // execute based on the command
        match &args[1][..] {
            "--version" => {
                // verify that no extra arguments where given
                if args.len() > 2 {
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // return the help message
                message = VERSION.to_string();
            }

            "help" => {
                // verify that no extra arguments where given
                if args.len() > 2 {
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // return the help message
                message = Self::help();
            },
            "list" => {
                // verify that no extra arguments where given
                if args.len() > 2 {
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // get the trackers list
                let trackers = self.list().await;

                // create the output message
                if trackers.len() == 1 {
                    message = format!("found 1 tracker: {}\n0\n", trackers[0]);
                } else {
                    // Create the output string for the  trackers list
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
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // execute the command
                match self.create(&args[2]).await {
                    Ok(execution_message) => message = execution_message,
                    Err(error) => message = error.message,
                }
            },
            "delete" => {
                // verify the command
                if args.len() != 3 {
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // run the command
                match self.delete(&args[2]).await {
                    Ok(execution_message) => message = execution_message,
                    Err(error) => message = error.message,
                }
            },
            "toggle" => {
                // verify the command
                if args.len() != 3 {
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // run the command
                match self.start_stop(&args[2]).await {
                    Ok(execution_message) => message = execution_message,
                    Err(error) => message = error.message,
                }
            },
            "log" => {
                // verify the command
                if args.len() != 3 {
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                // run the command
                match self.log(&args[2]).await {
                    Ok(execution_message) => message = execution_message,
                    Err(error) => message = error.message,
                }
            },
            _ => return Err(ChronosError::new(ErrorTypes::CommandNotFound, format!("Command {} not found", args[1]))),
        }

        Ok(message)
    }

    async fn connect_to_database() -> Result<Pool<Sqlite>, ChronosError> {
        // Check if the database exists
        let mut database_exists = match Sqlite::database_exists(DATABASE_URL).await {
            Ok(exists) => exists,
            Err(error) => return Err(ChronosError::new(ErrorTypes::DatabaseError, format!("Failed to check if database exists: {}", error))),
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
            return Err(ChronosError::new(ErrorTypes::DatabaseError, "Failed to create database".to_string()));
        }

        // Try to connect to db.
        let mut last_result:Result<Pool<Sqlite>, ChronosError> = Err(ChronosError::new(ErrorTypes::DatabaseError, "Did not try to connect to databse".to_string()));

        for _ in 0..3 {
            match SqlitePool::connect(&DATABASE_URL).await {
                Ok(pool) => {
                    last_result = Ok(pool);
                    break;
                },
                Err(error) => last_result = Err(ChronosError::new(ErrorTypes::DatabaseError, format!("Failed to connect to database: {}", error))),
            }
        }

        // return connection or error.
        last_result
    }

    // returns the help message
    fn help() -> String {
        String::from("Available commands:\
        help \t show this help message\
        list \t lists all current trackers\
        create [tracker name]\tcreates a new tracker")
    }

    // return a vec of all trackers
    pub async fn list(&self) -> Vec<String> {
        self.trackers.clone()
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

    // function to create new tracker (simply a table in the sqlite database)
    pub async fn create(&mut self, tracker_name: &str) -> Result<String, ChronosError> {
        // First make sure that the name isn't already taken
        if self.trackers.contains(&tracker_name.to_string()) {
            return Err(ChronosError::new(ErrorTypes::InvalidCommand, format!("tracker {} already exists", tracker_name)));
        }

        // Create the table with the tracker's name
        match query(AssertSqlSafe(format!("CREATE TABLE {} (timestamp INTEGER NOT NULL UNIQUE)", tracker_name)))
            .execute(&self.pool)
            .await {
            Ok(_) => {
                self.trackers.append(&mut vec![tracker_name.to_string()]);
                Ok(format!("tracker {} created", tracker_name))
            },
            Err(error) => Err(ChronosError::new(ErrorTypes::DatabaseError, format!("Failed to create tracker: {}", error))),
        }
    }

    pub async fn delete(&self, tracker_name: &str) -> Result<String, ChronosError> {
        // make sure the tracker we're trying to delete exists
        if !self.trackers.contains(&tracker_name.to_string()) {
            return Err(ChronosError::new(ErrorTypes::InvalidCommand, format!("tracker {} not found", tracker_name)));
        }

        // Drop the table (the tracker)
        match query(AssertSqlSafe(format!("DROP TABLE {}", tracker_name)))
        .execute(&self.pool)
        .await {
            Ok(_) => Ok(format!("tracker {} deleted", tracker_name)),
            Err(error) => Err(ChronosError::new(ErrorTypes::DatabaseError, format!("Failed to delete tracker: {}", error))),
        }
    }

    pub async fn start_stop(&self, tracker: &String) -> Result<String, ChronosError> {
        // get the time stamp
        let timestamp = match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs() as i64,
            Err(error) => return Err(ChronosError::new(ErrorTypes::NoArguments, format!("Can't get system time: {}", error))),
        };

        // add it to the tracker's table
        match query(AssertSqlSafe(format!("INSERT INTO {} (timestamp) VALUES (?)", tracker)))
            .bind(timestamp)
            .execute(&self.pool)
            .await {
            Ok(_) => Ok(format!("Event added to tracker: {}", tracker)),
            Err(error) => Err(ChronosError::new(ErrorTypes::DatabaseError, format!("Failed to create tracker: {}", error))),
        }
    }

    // function to get the time as a formatted string
    pub async fn log(&self, tracker_name: &String) -> Result<String, ChronosError> {
        // get all the events
        let rows = match query(AssertSqlSafe(format!("SELECT * FROM {} ORDER BY timestamp ASC", tracker_name)))
            .fetch_all(&self.pool)
            .await {
            Ok(events) => events,
            Err(error) => return Err(ChronosError::new(ErrorTypes::DatabaseError, format!("Failed to query tracker: {}", error))),
        };

        // extract the timestamp
        let events = rows
            .iter()
            .map(|row| row.get::<i64, _>("timestamp"))
            .collect::<Vec<i64>>();

        // prepare variables to calculate time
        let mut toggled = false;
        let mut last_start: i64 = 0;
        let mut time: i64 = 0;

        // calculate time
        for timestamp in events {
            if !toggled {
                last_start = timestamp;
                toggled = true;
            } else {
                time += timestamp - last_start;
                toggled = false;
            }
        }

        Ok(format!("You have worked on {} for {}", tracker_name, Self::convert_timestamp(time)))
    }

    // function that formats a timestamp
    fn convert_timestamp(mut timestamp: i64) -> String {
        // get individual units of time
        let seconds = timestamp % 60;
        println!("{}", seconds);
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

    // function to run the TUI
    pub fn run_tui(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    // function to draw the TUI
    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    // handle events
    fn handle_events(&mut self) -> std::io::Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_events(key_event)
            }
            _ => {}
        };

        Ok(())
    }

    // handle keyboard events
    fn handle_key_events(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}