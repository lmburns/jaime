#[allow(unused)]
use anyhow::{anyhow, Context as AnyhowContext, Result};
use colored::Colorize;
use once_cell::sync::Lazy;
use rustyline::{error::ReadlineError, Editor};
use serde::{Deserialize, Serialize};
use skim::{
    prelude::{SkimItemReader, SkimItemReaderOption, SkimOptionsBuilder},
    Skim,
};

use crate::app::Handler;
use std::{
    collections::HashMap,
    env,
    io::{Cursor, Write},
    path::PathBuf,
    process::{self, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

#[macro_export]
macro_rules! jaime_error {
    ($($err:tt)*) => ({
        eprintln!("{}: {}", "[jaime error]".red().bold(), format!($($err)*));
    })
}

static NUM_RUNS: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

#[cfg(not(windows))]
const FZF_BIN: &str = "fzf";
#[cfg(windows)]
const FZF_BIN: &str = "fzf.exe";

#[cfg(not(windows))]
const SKIM_BIN: &str = "sk";
#[cfg(windows)]
const SKIM_BIN: &str = "sk.exe";

#[derive(Debug)]
pub(crate) struct Context {
    pub(crate) cache_directory: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Config {
    pub(crate) options:     HashMap<String, Action>,
    pub(crate) shell:       Option<String>,
    pub(crate) description: Option<String>,
}

impl Config {
    #[must_use]
    pub(crate) fn into_action(self) -> Action {
        Action::Select {
            options:     self.options,
            description: self.description,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub(crate) enum Widget {
    FromCommand {
        command: String,
        preview: Option<String>,
    },
    FreeText,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub(crate) enum Action {
    Command {
        description: Option<String>,
        command:     String,
        widgets:     Option<Vec<Widget>>,
    },
    Select {
        description: Option<String>,
        options:     HashMap<String, Action>,
    },
}

fn run_shell(context: &Context, cmd: &str, shell: &str) -> Result<()> {
    let mut builder = Command::new(shell);

    if shell == "zsh" {
        builder.arg("--shwordsplit");
        builder.arg("--no-unset");
        builder.arg("--errexit");
    } else if shell == "bash" {
        builder.arg("-e");
        builder.arg("-u");
    }

    builder
        .arg("-c")
        .arg(cmd)
        .env("JAIME_CACHE_DIR", &context.cache_directory)
        .status()?;

    Ok(())
}

fn run_shell_command_for_output(context: &Context, cmd: &str, shell: &str) -> Result<String> {
    let mut builder = Command::new(shell);

    if shell == "zsh" {
        builder.arg("--shwordsplit"); // -y
        builder.arg("--no-unset"); // -u
        builder.arg("--errexit"); // -e
    } else if shell == "bash" {
        builder.arg("-e");
        builder.arg("-u");
    }

    Ok(std::str::from_utf8(
        builder
            .arg("-c")
            .arg(cmd)
            .env("JAIME_CACHE_DIR", &context.cache_directory)
            .output()?
            .stdout
            .as_slice(),
    )?
    .to_owned())
}

/// Display selection with the `skim` library
fn display_selector(input: String, preview: Option<&str>) -> Option<String> {
    let mut skim_args = Vec::new();
    let default_height = String::from("50%");
    let default_margin = String::from("0%");
    let default_layout = String::from("default");
    // This is the default settings within the skim 'src/' folder
    let default_theme = String::from(
        "matched:108,matched_bg:0,current:254,current_bg:236,current_match:151,current_match_bg:\
         236,spinner:148,info:144,prompt:110,cursor:161,selected:168,header:109,border:59",
    );

    skim_args.extend(
        std::env::var("SKIM_DEFAULT_OPTIONS")
            .ok()
            .and_then(|val| shlex::split(&val))
            .unwrap_or_default(),
    );

    let options = SkimOptionsBuilder::default()
        .preview(preview)
        .margin(Some(
            skim_args
                .iter()
                .find(|arg| arg.contains("--margin") && *arg != &"--margin".to_string())
                .unwrap_or_else(|| {
                    skim_args
                        .iter()
                        .position(|arg| arg.contains("--margin"))
                        .map_or(&default_margin, |pos| &skim_args[pos + 1])
                }),
        ))
        .height(Some(
            skim_args
                .iter()
                .find(|arg| arg.contains("--height") && *arg != &"--height".to_string())
                .unwrap_or_else(|| {
                    skim_args
                        .iter()
                        .position(|arg| arg.contains("--height"))
                        .map_or(&default_height, |pos| &skim_args[pos + 1])
                }),
        ))
        .layout(
            skim_args
                .iter()
                .find(|arg| arg.contains("--layout") && *arg != &"--layout".to_string())
                .unwrap_or_else(|| {
                    skim_args
                        .iter()
                        .position(|arg| arg.contains("--layout"))
                        .map_or(&default_layout, |pos| &skim_args[pos + 1])
                }),
        )
        .color(Some(
            skim_args
                .iter()
                .find(|arg| {
                    arg.contains("--color") && *arg != &"--color".to_string() && !arg.contains("{}")
                })
                .unwrap_or_else(|| {
                    skim_args
                        .iter()
                        .position(|arg| arg.contains("--color"))
                        .map_or(&default_theme, |pos| &skim_args[pos + 1])
                }),
        ))
        .bind(
            skim_args
                .iter()
                .filter(|arg| arg.contains("--bind"))
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )
        .reverse(skim_args.iter().any(|arg| arg.contains("--reverse")))
        .tac(skim_args.iter().any(|arg| arg.contains("--tac")))
        .nosort(skim_args.iter().any(|arg| arg.contains("--no-sort")))
        .inline_info(skim_args.iter().any(|arg| arg.contains("--inline-info")))
        .multi(false)
        .build()
        .unwrap();

    // `SkimItemReader` is a helper to turn any `BufRead` into a stream of
    // `SkimItem` `SkimItem` was implemented for `AsRef<str>` by default
    let item_reader_opts = SkimItemReaderOption::default().ansi(true).build();
    let item_reader = SkimItemReader::new(item_reader_opts);
    let items = item_reader.of_bufread(Cursor::new(input));

    // let item_reader =
    // Rc::new(RefCell::new(SkimItemReader::new(item_reader_opts))); let items =
    // item_reader.borrow().of_bufread(Cursor::new(input));

    let selected_items = Skim::run_with(&options, Some(items));

    selected_items
        .map_or_else(Vec::new, |out| {
            if out.is_abort {
                process::exit(130);
            }
            out.selected_items
        })
        .get(0)
        .map(|selected| selected.output().to_string())
}

/// Display selection with the `fzf` binary
fn display_selector_fzf(input: &str, preview: Option<&str>) -> Option<String> {
    // Spawn fzf
    let mut command = Command::new(FZF_BIN);

    if let Some(prev) = preview {
        command.arg("--preview").arg(prev);
        command.arg("--preview-window").arg(":nohidden");
    } else {
        command.arg("--preview-window").arg(":hidden");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    if let Some(fzf_opts) = env::var_os("FZF_DEFAULT_OPTS") {
        command.env("FZF_DEFAULT_OPTS", fzf_opts);
    }

    let mut child = command.spawn().expect("failed to spawn fzf");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("failed to feed list of items to fzf");

    let output = child.wait_with_output().expect("failed to select with fzf");

    // No item selected on non-zero exit code
    if !output.status.success() {
        return None;
    }

    // Get selected item, assert validity
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let stdout = stdout.strip_suffix('\n').unwrap_or(stdout);

    Some(stdout.into())
}

/// Display selection with the `skim` binary
fn display_selector_skim(input: &str, preview: Option<&str>) -> Option<String> {
    let mut command = Command::new(SKIM_BIN);
    if let Some(prev) = preview {
        command.arg("--preview").arg(prev);
        command.arg("--preview-window").arg(":nohidden");
    } else {
        command.arg("--preview-window").arg(":hidden");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    if let Some(skim_opts) = env::var_os("SKIM_DEFAULT_OPTIONS") {
        command.env("SKIM_DEFAULT_OPTIONS", skim_opts);
    }

    let mut child = command.spawn().expect("failed to spawn skim");

    // Communicate list of items to skim
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("failed to feed list of items to skim");

    let output = child
        .wait_with_output()
        .expect("failed to select with skim");

    // No item selected on non-zero exit code
    if !output.status.success() {
        return None;
    }

    // Get selected item, assert validity
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let stdout = stdout.strip_suffix('\n').unwrap_or(stdout);

    Some(stdout.into())
}

fn readline() -> Result<String> {
    let mut rl = Editor::<()>::new();

    let line = rl.readline("> ");
    match line {
        Ok(line) => Ok(line),
        Err(ReadlineError::Interrupted) => Err(anyhow!("Interrupted")),
        Err(ReadlineError::Eof) => Err(anyhow!("EOF")),
        Err(err) => Err(err.into()),
    }
}

impl Action {
    /// # Errors
    /// Could return an error if the configuration file is unable to be parsed
    ///
    /// # Panics
    /// Should never panic. Unwraps after checking for valid command
    pub(crate) fn run(&self, context: &Context, config: &Config, handler: &Handler) -> Result<()> {
        let shell = &config.shell.as_ref().map_or(
            env::var("SHELL").unwrap_or_else(|_| "sh".to_string()),
            ToOwned::to_owned,
        );

        match self {
            Action::Command {
                command, widgets, ..
            } => {
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

                                let output =
                                    run_shell_command_for_output(context, &command, shell)?;

                                let selected_command = if handler.fzf() {
                                    display_selector_fzf(
                                        &output,
                                        preview.as_ref().map(|s| s.as_ref()),
                                    )
                                } else if handler.skim() {
                                    display_selector_skim(
                                        &output,
                                        preview.as_ref().map(|s| s.as_ref()),
                                    )
                                } else {
                                    display_selector(output, preview.as_ref().map(|s| s.as_ref()))
                                };

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

                run_shell(context, &command, shell)
            },
            Action::Select {
                options,
                description: _,
            } => {
                let input = options
                    .keys()
                    .map(|k| {
                        if let Some(Action::Select {
                            description: Some(description),
                            ..
                        }) = options.get(k)
                        {
                            format!("{}: {}", k.green().bold(), description.magenta())
                        } else if let Some(Action::Command {
                            description: Some(description),
                            ..
                        }) = options.get(k)
                        {
                            format!("{}: {}", k.green().bold(), description.magenta())
                        } else {
                            k.green().bold().to_string()
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("\n");

                let selected_command =
                    if handler.has_command() && NUM_RUNS.load(Ordering::Relaxed) == 0 {
                        let cmd = handler.command().map(ToString::to_string).unwrap();
                        if options.keys().any(|k| *k == cmd) {
                            Some(cmd)
                        } else {
                            let avail = options.keys().fold(String::new(), |mut acc, k| {
                                acc.push_str(&format!("{}, ", k.yellow()));
                                acc
                            });
                            jaime_error!(
                                "{} is an invalid selection and doesn't match any of the keys you \
                                 have in your configuration file.\nAvailable keys are: {}",
                                cmd.green(),
                                avail
                                    .strip_suffix(", ")
                                    .map_or(avail.clone(), ToString::to_string)
                            );
                            process::exit(1);
                        }
                    } else if handler.fzf() {
                        display_selector_fzf(&input, None)
                    } else if handler.skim() {
                        display_selector_skim(&input, None)
                    } else {
                        display_selector(input, None)
                    };

                selected_command.map_or(Ok(()), |selected_command| {
                    match options.get(
                        &selected_command
                            .contains(':')
                            .then(|| selected_command.split(':').collect::<Vec<_>>()[0].to_string())
                            .unwrap_or(selected_command),
                    ) {
                        Some(widget) => {
                            NUM_RUNS.fetch_add(1, Ordering::Relaxed);
                            widget.run(context, config, handler)
                        },
                        None => Ok(()),
                    }
                })
            },
        }
    }
}
