mod chronos;

// =================================================================================================
// IMPORTS
// =================================================================================================
use std::env;

use chronos::Chronos;

#[tokio::main]
async fn main() {
    // create chronos
    let mut chronos = match Chronos::new().await {
        Ok(chronos) => chronos,
        Err(error) => panic!("{}", error.message),
    };

    // run and print message
    let message = match chronos.validate_and_run(env::args().collect()).await {
        Ok(message) => println!("{}", message),
        Err(error) => eprintln!("{}", error.message),
    };
}