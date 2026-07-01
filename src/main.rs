mod day;
mod tracker;
mod tracker_event;
mod tracker_session;
mod chronos;

use std::env;

#[tokio::main]
async fn main() {
    // collect the args
    let args: Vec<String> = env::args().collect();

    // create the chronos object
    let mut chronos = match chronos::Chronos::new().await {
        Ok(chronos) => chronos,
        Err(e) => {
            println!("Error: {}", e.message);
            return;
        }
    };
    
    // if more than "chronos" as args, execute the command else run tui
    if args.len() > 1 {
        match &args[1][..] {
            _ => match chronos.run_command(args).await {
                Ok(output) => println!("{}", output),
                Err(e) => println!("Error: {}", e.message),
            },
        };
    } else {
        match chronos.tui().await {
            Ok(_) => (),
            Err(e) => println!("Error: {}", e.message),
        }
    }
}
