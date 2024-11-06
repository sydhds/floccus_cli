// std
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::LazyLock;
// third-party
use clap::{Args, Parser, Subcommand};
use thiserror::Error;
use tracing::debug;
use url::Url;
// internal
use crate::cli::config::FloccusCliConfig;

const CLI_REPOSITORY_NAME_DEFAULT: &str = "bookmarks";

static CLI_REPOSITORY_SSH_KEY_DEFAULT: LazyLock<String> = LazyLock::new(|| {
    format!("{}/.ssh/id_ed25519", std::env::var("HOME").unwrap_or_default())
});

#[derive(Debug, Clone, Parser)]
#[command(name = "floccus-cli")]
#[command(version, about = "A cli tool compatible with Floccus", long_about = None)]
pub struct Cli {
    #[arg(
        short = 'r',
        long = "repository",
        help = "(Optional) git repository path"
    )]
    pub repository_folder: Option<PathBuf>,
    #[arg(
        short = 'g',
        long = "git",
        help = "Git repository url, e.g.https://github.com/_USERNAME_/_REPO_.git"
    )]
    pub repository_url: Option<Url>,
    #[arg(
        short = 'n',
        long = "name",
        help = "Repository local name",
        default_value = CLI_REPOSITORY_NAME_DEFAULT
    )]
    pub repository_name: String,
    #[arg(
        short = 't',
        long = "token",
        help = "Repository token",
    )]
    pub repository_token: Option<String>,
    #[arg(
        short = 's',
        long = "ssh_key",
        help = "Repository ssh key",
        long_help = "Repository private ssh key path (e.g. ~/.ssh/id_rsa or ~/.ssh/id_ed25519) - Only for git clone with ssh url (aka git@github.com:_USERNAME_/_REPO_.git)",
        default_value = &**CLI_REPOSITORY_SSH_KEY_DEFAULT,
    )]
    pub repository_ssh_key: PathBuf,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Error, Debug)]
pub enum OverrideCliError {
    #[error("Cannot set url username")]
    UrlSetUsername,
}

#[derive(Error, Debug)]
pub enum ParseCliError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error(transparent)]
    OverrideCli(#[from] OverrideCliError),
}

/// Parse from command line arguments and override values from config file
pub fn parse_cli_and_override(config_path: Option<PathBuf>) -> Result<Cli, ParseCliError> {
    let mut cli = Cli::parse();
    if let Some(config_path) = config_path {
        let config_str = std::fs::read_to_string(config_path)?;
        let config: FloccusCliConfig = toml::from_str(config_str.as_str())?;
        override_cli_with(&mut cli, config)?;
    }

    Ok(cli)
}

fn override_cli_with(cli: &mut Cli, config: FloccusCliConfig) -> Result<(), OverrideCliError> {
    
    // Merge config into cli
    if config.git.enable {
        
        if cli.repository_token.is_none() {
            cli.repository_token = config.git.repository_token;
        }
        if cli.repository_ssh_key == PathBuf::from(&**CLI_REPOSITORY_SSH_KEY_DEFAULT) {
            if let Some(repo_ssh_key) = config.git.repository_ssh_key {
                if repo_ssh_key != PathBuf::from("") {
                    cli.repository_ssh_key = repo_ssh_key;
                } 
            }
        }
        
        if cli.repository_url.is_none() {
            cli.repository_url = config.git.repository_url;
        }
        
        // merge url with git token
        if let Some(ref repository_token) = cli.repository_token {
            let repo_url = cli.repository_url.clone();
            if let Some(mut repo_url) = repo_url {
                if !repository_token.is_empty() {
                    repo_url
                        .set_username(repository_token)
                        .map_err(|_e| OverrideCliError::UrlSetUsername)?;
                    cli.repository_url = Some(repo_url);
                }
            }
        }
        
        if cli.repository_name == CLI_REPOSITORY_NAME_DEFAULT
            && config.git.repository_name.is_some()
        {
            cli.repository_name = config.git.repository_name.unwrap();
        }

        if config.git.disable_push.is_some() {
            match cli.command {
                Commands::Add(ref mut add_args) => {
                    if add_args.disable_push.is_none() {
                        add_args.disable_push = config.git.disable_push;
                    }
                }
                Commands::Rm(ref mut rm_args) => {
                    if rm_args.disable_push.is_none() {
                        rm_args.disable_push = config.git.disable_push;
                    }
                }
                _ => {}
            }
        }
    }

    debug!("cli (with config): {:?}", cli);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Subcommand)]
pub enum Commands {
    #[command(about = "Init Floccus cli config file")]
    Init(InitArgs),
    #[command(about = "Print bookmarks")]
    Print(PrintArgs),
    #[command(about = "Add bookmark")]
    Add(AddArgs),
    #[command(about = "Remove bookmark")]
    Rm(RemoveArgs),
    #[command(about = "Find bookmark")]
    Find(FindArgs),
}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct InitArgs {}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct PrintArgs {}

#[derive(Debug, Clone, PartialEq)]
pub enum Placement {
    Before,
    After,
    InFolderPrepend,
    InFolderAppend,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Under {
    Root,
    Id(u64, Placement),
    Folder(String),
}

impl FromStr for Under {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const PLACEMENT_AFTER_PREFIX: &str = "after=";
        const PLACEMENT_BEFORE_PREFIX: &str = "before=";
        const PLACEMENT_APPEND_PREFIX: &str = "append=";
        const PLACEMENT_PREPEND_PREFIX: &str = "prepend=";

        match s {
            "root" => Ok(Under::Root),
            _ => {
                let (rem, placement) =
                    if let Some(stripped) = s.strip_prefix(PLACEMENT_AFTER_PREFIX) {
                        (stripped, Placement::After)
                    } else if let Some(stripped) = s.strip_prefix(PLACEMENT_BEFORE_PREFIX) {
                        (stripped, Placement::Before)
                    } else if let Some(stripped) = s.strip_prefix(PLACEMENT_APPEND_PREFIX) {
                        (stripped, Placement::InFolderAppend)
                    } else if let Some(stripped) = s.strip_prefix(PLACEMENT_PREPEND_PREFIX) {
                        (stripped, Placement::InFolderPrepend)
                    } else {
                        (s, Placement::InFolderAppend)
                    };

                if let Ok(s_id) = rem.parse::<u64>() {
                    Ok(Under::Id(s_id, placement))
                } else {
                    Ok(Under::Folder(s.to_string()))
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct AddArgs {
    #[arg(short = 'b', long = "bookmark", help = "Url to add")]
    pub(crate) url: String,
    #[arg(short = 't', long = "title", help = "Url title or description")]
    pub(crate) title: String,
    #[arg(short = 'u', long = "under", help = "Add bookmark under ...", default_value = "root", value_parser=under_parser)]
    pub(crate) under: Under,
    #[clap(
        long = "disable-push",
        help = "Add the new bookmark locally but do not push (git push) it",
        default_missing_value("true"),
        default_value("true"),
        num_args(0..=1),
        require_equals(true),
    )]
    pub(crate) disable_push: Option<bool>,
}

// FIXME: Result error fix
fn under_parser(s: &str) -> Result<Under, &'static str> {
    Under::from_str(s).map_err(|_| "cannot parse under argument")
}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct RemoveArgs {
    #[arg(short = 'i', long = "item", help = "Remove bookmark or folder", value_parser=under_parser)]
    pub(crate) under: Under,
    #[clap(
        long = "disable-push",
        help = "Remove a bookmark or folder locally but do not push (git push) it",
        default_missing_value("true"),
        default_value("true"),
        num_args(0..=1),
        require_equals(true),
    )]
    pub(crate) disable_push: Option<bool>,
    #[arg(
        long = "dry-run",
        help = "Do not remove - just print",
        action,
        required = false
    )]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct FindArgs {
    #[arg(
        short = 't',
        long = "title",
        help = "Only search in folder or bookmark titles (Default: search on url & titles)",
        action,
        required = false
    )]
    pub(crate) title: bool,
    #[arg(
        short = 'u',
        long = "url",
        help = "Only search in folder or bookmark url (Default: search on url & titles)",
        action,
        required = false
    )]
    pub(crate) url: bool,
    #[arg(
        short = 'f',
        long = "folder",
        help = "Perform search only for folders",
        action,
        required = false
    )]
    pub(crate) folder: bool,
    #[arg(
        short = 'b',
        long = "bookmark",
        help = "Perform search only for bookmarks",
        action,
        required = false
    )]
    pub(crate) bookmark: bool,
    /// What to find
    pub(crate) find: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG_1: &str = r#"
[logging]
    # Logging level -> 0: ERROR, 1: WARN, 2: INFO, 3: DEBUG, 4: TRACE
    level = 2

[git]
    enable = true
    repository_url = "https://github_pat_MY_TOKEN@github.com/sydhds/floccus_test.git"
    repository_name = "bookmarks"
    disable_push = false
    "#;

    #[test]
    fn test_cli_override() {
        let mut cli = Cli::parse_from([
            "target/debug/floccus_cli",
            "rm",
            "-i",
            "5",
            "--disable-push",
        ]);
        let config: FloccusCliConfig = toml::from_str(CONFIG_1).unwrap();
        override_cli_with(&mut cli, config);

        if let Commands::Rm(rm_args) = cli.command {
            // Note: disable-push is set to false in config and then override by command line
            assert_eq!(rm_args.disable_push, Some(true))
        } else {
            unreachable!()
        }
    }
}
