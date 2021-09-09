use failure::{format_err, Error};
use rustyline::{error::ReadlineError, Editor};
use serde::{Deserialize, Serialize};
use skim::{
    prelude::{SkimItemReader, SkimItemReaderOption, SkimOptionsBuilder},
    Skim,
};

use std::{collections::HashMap, io::Cursor, path::PathBuf, process::Command};

#[derive(Debug)]
pub struct Context {
    pub cache_directory: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub options: HashMap<String, Action>,
    pub shell:   Option<String>,
}

impl Config {
    pub fn into_action(self) -> Action {
        Action::Select {
            options: self.options,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Widget {
    FromCommand {
        command: String,
        preview: Option<String>,
    },
    FreeText,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Action {
    Command {
        command: String,
        widgets: Option<Vec<Widget>>,
    },
    Select {
        options: HashMap<String, Action>,
    },
}

fn run_shell(context: &Context, cmd: &str, shell: &str) -> Result<(), Error> {
    Command::new(shell)
        .arg("-c")
        .arg(cmd)
        .env("JAIME_CACHE_DIR", &context.cache_directory)
        .status()?;

    Ok(())
}

fn run_shell_command_for_output(context: &Context, cmd: &str) -> Result<String, Error> {
    Ok(std::str::from_utf8(
        Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("JAIME_CACHE_DIR", &context.cache_directory)
            .output()?
            .stdout
            .as_slice(),
    )?
    .to_owned())
}

fn display_selector(input: String, preview: Option<&str>) -> Result<Option<String>, Error> {
    let mut skim_args = Vec::new();
    let default_height = String::from("50%");
    skim_args.extend(
        std::env::var("SKIM_DEFAULT_OPTIONS")
            .ok()
            .and_then(|val| shlex::split(&val))
            .unwrap_or_default(),
    );

    let item_reader_opts = SkimItemReaderOption::default().ansi(true).build();
    let options = SkimOptionsBuilder::default()
        .preview(preview)
        .height(
            Some(skim_args
                .iter()
                .find(|arg| arg.contains("--height") && *arg != &"--height".to_string())
                .unwrap_or_else(|| {
                    if let Some(pos) = skim_args.iter().position(|arg| arg.contains("--height")) {
                        &skim_args[pos + 1]
                    } else {
                        &default_height
                    }
                })),
        )
        .color(
            skim_args
                .iter()
                .find(|arg| arg.contains("--color") && !arg.contains("{}"))
                .map(|p| p.as_str()),
        )
        .multi(false)
        .build()
        .map_err(|err| format_err!("{}", err))?;

    // `SkimItemReader` is a helper to turn any `BufRead` into a stream of
    // `SkimItem` `SkimItem` was implemented for `AsRef<str>` by default
    let item_reader = SkimItemReader::new(item_reader_opts);
    let items = item_reader.of_bufread(Cursor::new(input));

    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| out.selected_items)
        .unwrap_or_else(Vec::new);

    Ok(selected_items
        .iter()
        .next()
        .map(|selected| selected.output().to_string()))
}

fn readline() -> Result<String, Error> {
    let mut rl = Editor::<()>::new();

    let line = rl.readline("> ");
    match line {
        Ok(line) => Ok(line),
        Err(ReadlineError::Interrupted) => Err(format_err!("Interrupted")),
        Err(ReadlineError::Eof) => Err(format_err!("EOF")),
        Err(err) => Err(err)?,
    }
}

impl Action {
    pub fn run(&self, context: &Context, config: &Config) -> Result<(), Error> {
        match self {
            Action::Command { command, widgets } => {
                let mut args: Vec<String> = Vec::new();

                if let Some(widgets) = widgets {
                    for (index, widget) in widgets.iter().enumerate() {
                        match widget {
                            Widget::FreeText => {
                                args.push(readline()?);
                            },
                            Widget::FromCommand { command, preview } => {
                                let mut command = command.clone();
                                for (i, arg) in args.iter().enumerate().take(index) {
                                    command = command.replace(&format!("{{{}}}", i), arg);
                                }

                                let output = run_shell_command_for_output(context, &command)?;

                                let selected_command =
                                    display_selector(output, preview.as_ref().map(|s| s.as_ref()))?;

                                if let Some(selected_command) = selected_command {
                                    args.push(selected_command);
                                } else {
                                    return Ok(());
                                }
                            },
                        }
                    }
                }

                let mut command = command.clone();

                for (index, arg) in args.iter().enumerate() {
                    command = command.replace(&format!("{{{}}}", index), arg);
                }

                let shell = if let Some(sh) = &config.shell {
                    sh
                } else {
                    "sh"
                };

                run_shell(context, &command, shell)
            },
            Action::Select { options } => {
                let input = options
                    .keys()
                    .map(|k| k.as_ref())
                    .collect::<Vec<&str>>()
                    .join("\n");
                let selected_command = display_selector(input, None)?;

                if let Some(selected_command) = selected_command {
                    match options.get(&selected_command) {
                        Some(widget) => widget.run(context, config),
                        None => Ok(()),
                    }
                } else {
                    Ok(())
                }
            },
        }
    }
}
