use failure::{Error, ResultExt};
use std::{
    fs::File,
    process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

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

    let running = Arc::new(AtomicBool::new(false));
    let r = Arc::clone(&running);
    // Should work for a double-ctrl-c exit, but doesn't
    ctrlc::set_handler(move || {
        if r.load(Ordering::Relaxed) {
            std::process::exit(1);
        } else {
            r.store(true, Ordering::Relaxed);
        }
    })?;

    action.run(&context, &config, &running)?;

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
