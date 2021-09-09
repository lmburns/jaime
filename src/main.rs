#![deny(
    clippy::all,
    clippy::complexity,
    clippy::correctness,
    clippy::pedantic,
    clippy::perf,
    clippy::style
)]
// #![allow(
//     clippy::string_add,
//     clippy::blanket_clippy_restriction_lints,
//     clippy::filetype_is_file,
//     clippy::create_dir,
//     clippy::else_if_without_else,
//     clippy::exhaustive_enums,
//     clippy::exhaustive_structs,
//     clippy::exit,
//     clippy::implicit_return,
//     clippy::indexing_slicing,
//     clippy::integer_arithmetic,
//     clippy::integer_division,
//     clippy::missing_docs_in_private_items,
//     clippy::missing_errors_doc,
//     clippy::missing_inline_in_public_items,
//     clippy::module_name_repetitions,
//     clippy::pattern_type_mismatch,
//     clippy::shadow_reuse,
//
//     // Need to be fixed
//     clippy::expect_used,
//     clippy::unwrap_used,
//     clippy::panic_in_result_fn,
//     clippy::unreachable,
//     clippy::unwrap_in_result,
//     clippy::expect_fun_call,
//     clippy::unimplemented,
// )]
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

    action.run(&context, &config)?;

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
