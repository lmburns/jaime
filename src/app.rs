use clap::{crate_authors, crate_name, crate_version, App, AppSettings, Arg, ArgMatches};
use once_cell::sync::Lazy;
use std::env;

pub(crate) static NO_COLOR: Lazy<bool> = Lazy::new(|| env::var_os("NO_COLOR").is_some());

#[derive(Debug)]
pub(crate) struct Handler {
    matches: ArgMatches,
}

impl<'a> Handler {
    pub(crate) fn build() -> App<'a> {
        App::new(crate_name!())
            .version(crate_version!())
            .author(crate_authors!())
            .about("Command line launcher")
            .global_setting(AppSettings::PropagateVersion)
            .global_setting(AppSettings::DisableVersionForSubcommands)
            .global_setting(
                NO_COLOR
                    .then(|| AppSettings::ColorNever)
                    .unwrap_or(AppSettings::ColoredHelp),
            )
            .arg(
                Arg::new("command")
                    .long("command")
                    .short('c')
                    .takes_value(true)
                    .required(false)
                    .about("Command to open in the launcher"),
            )
            .arg(
                Arg::new("fzf")
                    .long("fzf")
                    .short('f')
                    .takes_value(false)
                    .required(false)
                    .about("Use fzf instead of skim library"),
            )
            .arg(
                Arg::new("skim")
                    .long("skim-binary")
                    .short('s')
                    .takes_value(false)
                    .required(false)
                    .about("Use skim binary instead of skim library"),
            )
    }

    pub(crate) fn parse() -> Handler {
        Handler {
            matches: Handler::build().get_matches(),
        }
    }

    /// Get the raw matches.
    #[allow(unused)]
    pub(crate) fn matches(&'a self) -> &'a ArgMatches {
        &self.matches
    }

    pub(crate) fn command(&'a self) -> Option<&'a str> {
        self.matches.value_of("command")
    }

    pub(crate) fn has_command(&'a self) -> bool {
        self.matches.is_present("command")
    }

    pub(crate) fn fzf(&'a self) -> bool {
        self.matches.is_present("fzf")
    }

    pub(crate) fn skim(&'a self) -> bool {
        self.matches.is_present("skim")
    }
}
