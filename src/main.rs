// =================================================================================================
// IMPORTS
// =================================================================================================
use sqlx::{migrate::MigrateDatabase, FromRow, Pool, Row, Sqlite, SqliteConnection, SqlitePool};

use std::env;

// =================================================================================================
// CONSTANTS
// =================================================================================================
const DATABASE_URL: &str = "sqlite://sqlite.db";

#[tokio::main]
async fn main() {

    // connect to the database.
    let database = match connect_to_database().await {
        Ok(database) => database,
        Err(error) => panic!("{:#?}", error),
    };

    // get all programs
    let programs: Vec<String> = match sqlx::query_scalar(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
            AND table_type = 'BASE TABLE'
        "#
    )
        .fetch_all(&database)
        .await {
        Ok(data) => data,
        Err(_) => Vec::new(),
    };

    println!("Found {} tables", programs.len());


}

async fn connect_to_database() -> Result<Pool<Sqlite>, sqlx::error::Error> {
    // Check if the database exists
    let mut database_exists = Sqlite::database_exists(DATABASE_URL).await?;

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
        panic!("Failed to create database");
    }

    // Try to connect to db.
    let mut last_result:Result<Pool<Sqlite>, sqlx::error::Error> = Err(sqlx::error::Error::BeginFailed);

    for _ in 0..3 {
        match SqlitePool::connect(&DATABASE_URL).await {
            Ok(pool) => {
                last_result = Ok(pool);
                break;
            },
            Err(error) => {
                eprintln!("Failed to connect to database: {}", error);
                last_result = Err(error);
            }
        }
    }

    // return connection or error.
    last_result
}