mod chronos;

// =================================================================================================
// IMPORTS
// =================================================================================================
use std::{env, io};
use chronos::Chronos;

#[tokio::main]
async fn main() -> io::Result<()> {
    // create chronos
    let mut chronos = match Chronos::new().await {
        Ok(chronos) => chronos,
        Err(error) => panic!("{}", error.message),
    };

    // collect the args
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        // run and print message
        let message = match chronos.validate_and_run(args).await {
            Ok(message) => println!("{}", message),
            Err(error) => eprintln!("{}", error.message),
        };
    } else {
        ratatui::run(|terminal| chronos.run_tui(terminal));
    }

    Ok(())
}