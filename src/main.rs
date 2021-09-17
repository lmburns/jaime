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
    unused,
    unused_allocation,
    unused_comparisons,
    unused_extern_crates,
    unused_import_braces,
    unused_lifetimes,
    unused_parens,
    unused_qualifications,
    variant_size_differences,
    while_true
)]
#![allow(clippy::too_many_lines)]

mod app;
mod runner;

use anyhow::{Context as AnyhowContext, Result};
use std::{
    env,
    fs::{self, File},
    path::PathBuf,
    process,
};

fn actual_main() -> Result<()> {
    let create_dir = |path: &PathBuf| -> Result<()> {
        if path.exists() {
            Ok(())
        } else {
            fs::create_dir_all(path).context(format!("unable to create: {}", path.display()))?;
            Ok(())
        }
    };

    let config_path = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|d| d.join(".config")))
        .context("Invalid configuration directory")?
        .join("jaime")
        .join("config.yml");

    create_dir(&config_path)?;

    let file = File::open(&config_path).context("Couldn't read config file")?;
    let config: runner::Config = serde_yaml::from_reader(file)?;

    let action = config.clone().into_action();

    let context = runner::Context {
        cache_directory: env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| dirs::home_dir().map(|d| d.join(".cache")))
            .context("Invalid cache directory")?
            .join("jaime"),
    };

    create_dir(&context.cache_directory)?;

    let app = app::Handler::parse();
    action.run(&context, &config, &app)?;

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
