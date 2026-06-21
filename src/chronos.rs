use std::fmt::Error;
/// # Chronos
///
/// Chronos is a struct that tracks time by storing events in a sqlite database.
///
/// You can simply create one and then validate_and_run a command (e.g. For a cli app)
/// Or you can execute actions directly from predefined functions.

/// ================================================================================================
/// IMPORTS
/// ================================================================================================

use sqlx::{AssertSqlSafe, migrate::MigrateDatabase, query, FromRow, Pool, Row, Sqlite, SqliteConnection, SqlitePool};

// =================================================================================================
// CONSTANTS
// =================================================================================================
const DATABASE_URL: &str = "sqlite://sqlite.db";

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
pub struct Chronos {
    pub pool: Pool<Sqlite>,
}

impl Chronos {
    pub async fn new() -> Result<Self, ChronosError> {
        let pool = match Self::connect_to_database().await {
            Ok(pool) => pool,
            Err(error) => return Err(error),
        };

        // return a new Chronos object
        Ok(Self {
            pool,
        })
    }

    pub async fn validate_and_run(&self, args: Vec<String>) -> Result<String, ChronosError>{
        // make sure an actual command was given
        if args.len() < 2 {
            return Err(ChronosError::new(ErrorTypes::NoArguments, "No arguments provided enter 'chronos help to see available commands'".to_string()));
        }

        // define messages
        let invalid_message = "Invalid command, enter 'chronos help' to get a list of all available commands";
        let mut message = "Success!".to_string();

        // execute based on the command
        match &args[1][..] {
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

                // get the projects list
                let projects = self.list().await;

                // create the output message
                if projects.len() == 1 {
                    message = format!("found 1 project: {}\n", projects[0]);
                } else {
                    // Create the output string for the projects list
                    let mut output_string: String = String::new();

                    // generate the project's list output
                    for index in 0..projects.len() {
                        output_string.push_str(&format!("\n{}\t{}", projects.len() - 1 - index, projects[index]))
                    }

                    // return the final message
                    message = format!("found {} projects: {}",projects.len() , output_string);
                }
            },
            "create" => {
                if args.len() != 3 {
                    return Err(ChronosError::new(ErrorTypes::InvalidCommand, invalid_message.to_string()));
                }

                match self.create(&args[2]).await {
                    Ok(execution_message) => message = execution_message,
                    Err(error) => message = error.message,
                }
            }
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
        String::from("Available commands:\nhelp \t show this help message\nlist \t lists all current projects\ncreate [project name]\tcreates a new project")
    }

    // return a vec of all projects
    pub async fn list(&self) -> Vec<String> {
        // get all the table names
        let rows = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY rowid DESC"
        )
            .fetch_all(&self.pool)
            .await
            .unwrap_or_else(|_| Vec::new());

        // convert it into a Vec<String>
        rows
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .collect()
    }

    // function to create new project (simply a table in the sqlite database)
    pub async fn create(&self, project_name: &str) -> Result<String, ChronosError> {
        // First make sure that the name isn't already taken
        if self.list().await.contains(&project_name.to_string()) {
            return Err(ChronosError::new(ErrorTypes::InvalidCommand, format!("Project {} already exists", project_name)));
        }

        // Create the table with the project's name
        match query(AssertSqlSafe(format!("CREATE TABLE {} (timestamp INTEGER NOT NULL UNIQUE, event TEXT NOT NULL)", project_name)))
            .execute(&self.pool)
            .await {
            Ok(_) => Ok(format!("Project {} created", project_name)),
            Err(error) => Err(ChronosError::new(ErrorTypes::DatabaseError, format!("Failed to create project: {}", error))),
        }
    }
}

/*
// get the time stamp first to get the earliest time available
        let timestamp = match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs() as i64,
            Err(error) => return Err(Error::new(ErrorTypes::NoArguments, format!("Can't get system time: {}", error)),
        };
*/