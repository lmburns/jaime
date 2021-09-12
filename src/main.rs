use failure::{Error, ResultExt};
use std::{fs::File, process};

use jaime::{Config, Context};

fn actual_main() -> Result<(), Error> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("jaime")?;
    let config_path = xdg_dirs.place_config_file("config.yml")?;
    let file = File::open(config_path).context("Couldn't read config file")?;
    let config: Config = serde_yaml::from_reader(file)?;

    let action = config.clone().into_action();

    let context = Context {
        cache_directory: xdg_dirs.create_cache_directory("cache")?,
    };

    if let Err(err) = action.run(&context, &config) {
        eprintln!("Error: {}", err);
        process::exit(1);
    }

    Ok(())
}

fn main() {
    match actual_main() {
        Ok(()) => {},
        Err(err) => {
            println!("{}", err);
            process::exit(1);
        },
    }
}
