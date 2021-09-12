#![allow(unused)]
#![deny(
    clippy::all,
    clippy::complexity,
    clippy::correctness,
    clippy::pedantic,
    clippy::perf,
    clippy::style
)]
#![deny(
    absolute_paths_not_starting_with_crate,
    anonymous_parameters,
    bad_style,
    const_err,
    dead_code,
    ellipsis_inclusive_range_patterns,
    exported_private_dependencies,
    ill_formed_attribute_input,
    improper_ctypes,
    keyword_idents,
    macro_use_extern_crate,
    meta_variable_misuse, // May have false positives
    missing_abi,
    missing_debug_implementations, // can affect compile time/code size
    no_mangle_generic_items,
    non_shorthand_field_patterns,
    noop_method_call,
    overflowing_literals,
    path_statements,
    patterns_in_fns_without_body,
    pointer_structural_match,
    private_in_public,
    pub_use_of_private_extern_crate,
    semicolon_in_expressions_from_macros,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unaligned_references,
    unconditional_recursion,
    unreachable_pub,
    unsafe_code,
    // unused,
    // unused_allocation,
    // unused_comparisons,
    // unused_extern_crates,
    // unused_import_braces,
    // unused_lifetimes,
    // unused_parens,
    // unused_qualifications,
    variant_size_differences,
    while_true
)]

use colored::Colorize;
use failure::{format_err, Error};
use rustyline::{error::ReadlineError, Editor};
use serde::{Deserialize, Serialize};
use skim::{
    prelude::{
        SkimItemReader, SkimItemReaderOption, SkimItemReceiver, SkimItemSender, SkimOptionsBuilder,
    },
    AnsiString, DisplayContext, Skim, SkimItem,
};

use std::{
    borrow::Cow, collections::HashMap, env, io::{Cursor, BufReader}, path::PathBuf, process::Command, sync::Arc,
};
pub use std::{cell::RefCell, rc::Rc};

/// Wrapper used to implement attributes of `SkimItem` for a `String`
pub(crate) struct SkimJaime(String);

impl From<String> for SkimJaime {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl SkimItem for SkimJaime {
    fn display(&self, _: DisplayContext) -> AnsiString {
        self.0.to_string().into()
    }

    fn text(&self) -> Cow<str> {
        self.0.to_string().into()
    }

    fn output(&self) -> Cow<str> {
        self.0.clone().into()
    }
}

#[derive(Debug)]
pub struct Context {
    pub cache_directory: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub options:     HashMap<String, Action>,
    pub shell:       Option<String>,
    pub description: Option<String>,
}

impl Config {
    #[must_use]
    pub fn into_action(self) -> Action {
        Action::Select {
            options:     self.options,
            description: self.description,
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
        description: Option<String>,
        options:     HashMap<String, Action>,
    },
}

fn run_shell(context: &Context, cmd: &str, shell: &str) -> Result<(), Error> {
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

fn run_shell_command_for_output(
    context: &Context,
    cmd: &str,
    shell: &str,
) -> Result<Vec<SkimJaime>, Error> {
    let mut builder = Command::new(shell);

    if shell == "zsh" {
        builder.arg("--shwordsplit"); // -y
        builder.arg("--no-unset"); // -u
        builder.arg("--errexit"); // -e
    } else if shell == "bash" {
        builder.arg("-e");
        builder.arg("-u");
    }

    let output = builder
        .arg("-c")
        .arg(cmd)
        .env("JAIME_CACHE_DIR", &context.cache_directory)
        .output()?;

    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(|g| g.to_owned().into())
        .collect::<Vec<SkimJaime>>())
}

fn display_selector(
    items: SkimItemReceiver,
    preview: Option<&str>,
) -> Result<Option<String>, Error> {
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

    let mut options = SkimOptionsBuilder::default()
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
        .map_err(|err| format_err!("{}", err))?;

    let item_reader_opts = SkimItemReaderOption::default().ansi(true).build();
    let item_reader = Rc::new(RefCell::new(SkimItemReader::new(item_reader_opts)));

    options.cmd_collector = item_reader.clone();

    // `SkimItemReader` is a helper to turn any `BufRead` into a stream of
    // `SkimItem` `SkimItem` was implemented for `AsRef<str>` by default

    // let item_reader = SkimItemReader::new(item_reader_opts);

    // let j = Cursor::new(items);

    // let rx_item = item_reader.borrow().of_bufread(Cursor::new(items.into));

    let options = options;

    let selected_items = Skim::run_with(&options, Some(items));

    Ok(selected_items
        .map_or_else(Vec::new, |out| {
            if out.is_abort {
                std::process::exit(1);
            }
            out.selected_items
        })
        .get(0)
        .map(|selected| selected.output().to_string()))
}

fn readline() -> Result<String, Error> {
    let mut rl = Editor::<()>::new();

    let line = rl.readline("> ");
    match line {
        Ok(line) => Ok(line),
        Err(ReadlineError::Interrupted) => Err(format_err!("Interrupted")),
        Err(ReadlineError::Eof) => Err(format_err!("EOF")),
        Err(err) => Err(err.into()),
    }
}

fn skim_items<I: SkimItem>(items: Vec<I>) -> SkimItemReceiver {
    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) =
        skim::prelude::bounded(items.len());

    for g in items {
        let _drop = tx_item.send(Arc::new(g));
    }

    rx_item
}

impl Action {
    /// # Errors
    /// Could return an error if the configuration file is unable to be parsed
    //
    /// # Panics
    /// Unwrapping on `ctrlc`
    pub fn run(&self, context: &Context, config: &Config) -> Result<(), Error> {
        let shell = &config.shell.as_ref().map_or(
            env::var("SHELL").unwrap_or_else(|_| "sh".to_string()),
            ToOwned::to_owned,
        );

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

                                let output = skim_items(run_shell_command_for_output(
                                    context, &command, shell,
                                )?);

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

                run_shell(context, &command, shell)
            },
            Action::Select {
                options,
                description,
            } => {
                let input = skim_items(
                    options
                        .keys()
                        .map(|k| {
                            if let Some(desc) = description {
                                format!("{}: {}", k, desc).into()
                            } else {
                                k.to_string().into()
                            }
                        })
                        .collect::<Vec<SkimJaime>>(),
                );
                let selected_command = display_selector(input, None)?;

                selected_command.map_or(Ok(()), |selected_command| {
                    match options.get(
                        &selected_command
                            .contains(':')
                            .then(|| selected_command.split(':').collect::<Vec<_>>()[0].to_string())
                            .unwrap_or(selected_command),
                    ) {
                        Some(widget) => widget.run(context, config),
                        None => Ok(()),
                    }
                })
            },
        }
    }
}
