use cargo::core::dependency::DepKind;
use cargo::util::command_prelude::*;
use cargo::CargoResult;
use cargo_add::ops::add;
use cargo_add::ops::AddOptions;
use cargo_add::ops::DepOp;
use cargo_add::ops::DepTable;
use indexmap::IndexSet;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("add")
        .setting(clap::AppSettings::DeriveDisplayOrder)
        .about("Add dependencies to a Cargo.toml manifest file")
        .override_usage(
            "\
    cargo add [OPTIONS] <DEP>[@<VERSION>] [+<FEATURE>,...] ...
    cargo add [OPTIONS] <DEP_PATH> [+<FEATURE>,...] ...",
        )
        .after_help(
            "\
EXAMPLES:
  $ cargo add regex --build
  $ cargo add trycmd --dev
  $ cargo add ./crate/parser/
  $ cargo add serde +derive serde_json
",
        )
        .args([
            clap::Arg::new("crates")
                .takes_value(true)
                .value_name("DEP_ID")
                .multiple_occurrences(true)
                .required(true)
                .help("Reference to a package to add as a dependency")
                .long_help(
                "Reference to a package to add as a dependency

You can reference a package by:
- `<name>`, like `cargo add serde` (latest version will be used)
- `<name>@<version-req>`, like `cargo add serde@1` or `cargo add serde@=1.0.38`
- `<path>`, like `cargo add ./crates/parser/`

Additionally, you can specify features for a dependency by following it with a `+<FEATURE>`.",
            ),
            clap::Arg::new("no-default-features")
                .long("no-default-features")
                .help("Disable the default features"),
            clap::Arg::new("default-features")
                .long("default-features")
                .help("Re-enable the default features")
                .overrides_with("no-default-features"),
            clap::Arg::new("features")
                .short('F')
                .long("features")
                .takes_value(true)
                .value_name("FEATURES")
                .multiple_occurrences(true)
                .help("Space-separated list of features to add")
                .long_help("Space-separated list of features to add

Alternatively, you can specify features for a dependency by following it with a `+<FEATURE>`."),
            clap::Arg::new("optional")
                .long("optional")
                .help("Mark the dependency as optional")
                .long_help("Mark the dependency as optional

The package name will be exposed as feature of your crate.")
                .conflicts_with("dev"),
            clap::Arg::new("no-optional")
                .long("no-optional")
                .help("Mark the dependency as required")
                .long_help("Mark the dependency as required

The package will be removed from your features.")
                .conflicts_with("dev")
                .overrides_with("optional"),
            clap::Arg::new("rename")
                .short('r')
                .long("rename")
                .takes_value(true)
                .value_name("NAME")
                .help("Rename the dependency")
                .long_help("Rename the dependency

Example uses:
- Depending on multiple versions of a crate
- Depend on crates with the same name from different registries"),
            clap::Arg::new("registry")
                .long("registry")
                .takes_value(true)
                .value_name("NAME")
                .help("Package registry for this dependency")
                .conflicts_with("git"),
        ])
        .arg_manifest_path()
        .args([
            clap::Arg::new("package")
                .short('p')
                .long("package")
                .takes_value(true)
                .value_name("SPEC")
                .help("Package to modify"),
            clap::Arg::new("offline")
                .long("offline")
                .help("Run without accessing the network")
        ])
        .arg_quiet()
        .arg_dry_run("Don't actually write the manifest")
        .next_help_heading("SECTION")
        .args([
            clap::Arg::new("dev")
                .short('D')
                .long("dev")
                .help("Add as development dependency")
                .long_help("Add as development dependency

Dev-dependencies are not used when compiling a package for building, but are used for compiling tests, examples, and benchmarks.

These dependencies are not propagated to other packages which depend on this package.")
                .group("section"),
            clap::Arg::new("build")
                .short('B')
                .long("build")
                .help("Add as build dependency")
                .long_help("Add as build dependency

Build-dependencies are the only dependencies available for use by build scripts (`build.rs` files).")
                .group("section"),
            clap::Arg::new("target")
                .long("target")
                .takes_value(true)
                .value_name("TARGET")
                .forbid_empty_values(true)
                .help("Add as dependency to the given target platform")
                .group("section"),
        ])
        .next_help_heading("UNSTABLE")
        .args([
            clap::Arg::new("git")
                .long("git")
                .takes_value(true)
                .value_name("URI")
                .help("Git repository location")
                .long_help("Git repository location

Without any other information, cargo will use latest commit on the main branch."),
            clap::Arg::new("branch")
                .long("branch")
                .takes_value(true)
                .value_name("BRANCH")
                .help("Git branch to download the crate from")
                .requires("git")
                .group("git-ref"),
            clap::Arg::new("tag")
                .long("tag")
                .takes_value(true)
                .value_name("TAG")
                .help("Git tag to download the crate from")
                .requires("git")
                .group("git-ref"),
            clap::Arg::new("rev")
                .long("rev")
                .takes_value(true)
                .value_name("REV")
                .help("Git reference to download the crate from")
                .long_help("Git reference to download the crate from

This is the catch all, handling hashes to named references in remote repositories.")
                .requires("git")
                .group("git-ref"),
        ])
}

pub fn exec(config: &mut Config, args: &ArgMatches) -> CliResult {
    let dry_run = args.is_present("dry-run");
    let section = parse_section(args);

    let ws = args.workspace(config)?;
    let packages = args.packages_from_flags()?;
    let packages = packages.get_packages(&ws)?;
    let spec = match packages.len() {
        0 => {
            return Err(CliError::new(
                anyhow::format_err!("no packages selected.  Please specify one with `-p <PKGID>`"),
                101,
            ));
        }
        1 => packages[0],
        len => {
            return Err(CliError::new(
                anyhow::format_err!(
                    "{len} packages selected.  Please specify one with `-p <PKGID>`",
                ),
                101,
            ));
        }
    };

    let dependencies = parse_dependencies(config, args)?;

    let options = AddOptions {
        config,
        spec,
        dependencies,
        section,
        dry_run,
    };
    add(&ws, &options)?;

    Ok(())
}

fn parse_dependencies<'m>(config: &Config, matches: &'m ArgMatches) -> CargoResult<Vec<DepOp>> {
    let crates = matches
        .values_of("crates")
        .into_iter()
        .flatten()
        .map(String::from)
        .collect::<Vec<_>>();
    let git = matches.value_of("git");
    let branch = matches.value_of("branch");
    let rev = matches.value_of("rev");
    let tag = matches.value_of("tag");
    let rename = matches.value_of("rename");
    let registry = matches.registry(config)?;
    let default_features = default_features(matches);
    let features = matches.values_of("features").map(|f| {
        f.flat_map(parse_feature)
            .map(String::from)
            .collect::<IndexSet<_>>()
    });
    let optional = optional(matches);

    if crates.len() > 1 && git.is_some() {
        anyhow::bail!("cannot specify multiple crates with path or git or vers");
    }
    if git.is_some() && !config.cli_unstable().unstable_options {
        anyhow::bail!("`--git` is unstable and requires `-Z unstable-options`");
    }

    if crates.len() > 1 && rename.is_some() {
        anyhow::bail!("cannot specify multiple crates with rename");
    }

    if crates.len() > 1 && features.is_some() {
        anyhow::bail!("cannot specify multiple crates with features");
    }

    let mut deps: Vec<DepOp> = Vec::new();
    for crate_spec in crates {
        if let Some(features) = crate_spec.strip_prefix('+') {
            if !config.cli_unstable().unstable_options {
                anyhow::bail!("`+<feature>` is unstable and requires `-Z unstable-options`");
            }

            if let Some(prior) = deps.last_mut() {
                let features = parse_feature(features);
                prior
                    .features
                    .get_or_insert_with(Default::default)
                    .extend(features.map(String::from));
            } else {
                anyhow::bail!("`+<feature>` must be preceded by a pkgid");
            }
        } else {
            let dep = DepOp {
                crate_spec,
                rename: rename.map(String::from),
                features: features.clone(),
                default_features,
                optional,
                registry: registry.clone(),
                git: git.map(String::from),
                branch: branch.map(String::from),
                rev: rev.map(String::from),
                tag: tag.map(String::from),
            };
            deps.push(dep);
        }
    }
    Ok(deps)
}

fn default_features(matches: &ArgMatches) -> Option<bool> {
    resolve_bool_arg(
        matches.is_present("default-features"),
        matches.is_present("no-default-features"),
    )
}

fn optional(matches: &ArgMatches) -> Option<bool> {
    resolve_bool_arg(
        matches.is_present("optional"),
        matches.is_present("no-optional"),
    )
}

fn resolve_bool_arg(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
        (_, _) => unreachable!("clap should make this impossible"),
    }
}

fn parse_section(matches: &ArgMatches) -> DepTable {
    let kind = if matches.is_present("dev") {
        DepKind::Development
    } else if matches.is_present("build") {
        DepKind::Build
    } else {
        DepKind::Normal
    };

    let mut table = DepTable::new().set_kind(kind);

    if let Some(target) = matches.value_of("target") {
        assert!(!target.is_empty(), "Target specification may not be empty");
        table = table.set_target(target);
    }

    table
}

/// Split feature flag list
fn parse_feature(feature: &str) -> impl Iterator<Item = &str> {
    // Not re-using `CliFeatures` because it uses a BTreeSet and loses user's ordering
    feature
        .split_whitespace()
        .flat_map(|s| s.split(','))
        .filter(|s| !s.is_empty())
}
