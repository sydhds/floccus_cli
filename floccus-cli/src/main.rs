mod cli;
mod git;
// mod xbel;

// std
use std::borrow::Cow;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
// third-party
use directories::ProjectDirs;
use git2::Repository;
use thiserror::Error;
use toml_edit::{value, DocumentMut, TomlError};
use tracing::{debug, error, info};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;
// internal
use crate::cli::{
    parse_cli_and_override, AddArgs, Cli, Commands, FindArgs, InitArgs, Placement, PrintArgs,
    RemoveArgs, Under,
};
use crate::git::{git_clone, git_fetch, git_merge, git_push};
use floccus_xbel::{Xbel, XbelError, XbelItem, XbelItemOrEnd, XbelNestingIterator, XbelPath};

const FLOCCUS_CLI_CONFIG_ENV: &str = "FLOCCUS_CLI_CONFIG";
const FLOCCUS_CLI_QUALIFIER: &str = "app";
const FLOCCUS_CLI_ORGANIZATION: &str = "";
const FLOCCUS_CLI_APPLICATION: &str = "Floccus-cli";

const FLOCCUS_CLI_CONFIG_SAMPLE: &str = r#"
[git]
    enable = true
    repository_url = "https://github.com/__GITHUB_USER__/__GIT_REPO_NAME__.git"
    repository_name = "bookmarks"
    repository_token = ""
    repository_ssh_key = ""
    disable_push = true
"#;

fn main() -> Result<(), Box<dyn Error>> {

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
    
    let (config_path, config_path_expected): (Option<PathBuf>, PathBuf) = {
        // if FLOCCUS_CLI_CONFIG environment variable is set use it, otherwise use local config dir.
        let config_env = std::env::var(FLOCCUS_CLI_CONFIG_ENV);
        if let Ok(config_env) = config_env {
            (
                Some(PathBuf::from(config_env.clone())),
                PathBuf::from(config_env),
            )
        } else {
            let cfg = ProjectDirs::from(
                FLOCCUS_CLI_QUALIFIER,
                FLOCCUS_CLI_ORGANIZATION,
                FLOCCUS_CLI_APPLICATION,
            )
            .ok_or("Unable to determine local data directory")?
            .config_local_dir()
            .to_path_buf()
            .join("config.toml");

            if cfg.exists() {
                (Some(cfg.clone()), cfg)
            } else {
                (None, cfg)
            }
        }
    };

    debug!("config_path: {:?}", config_path);

    let cli = parse_cli_and_override(config_path.clone())?;

    debug!("cli args: {:?}", cli);

    // if repo folder is provided - use it otherwise - use a local data dir
    let repository_folder = if let Some(ref repository_folder) = cli.repository_folder {
        repository_folder.clone()
    } else {
        let repo_name = cli.repository_name.clone();
        ProjectDirs::from(
            FLOCCUS_CLI_QUALIFIER,
            FLOCCUS_CLI_ORGANIZATION,
            FLOCCUS_CLI_APPLICATION,
        )
        .ok_or("Unable to determine local data directory")?
        .data_local_dir()
        .join(repo_name)
    };

    info!("repository_folder: {}", repository_folder.display());

    match &cli.command {
        Commands::Init(init_args) => {
            let res = init_app(&cli, init_args, config_path_expected.as_path());

            if let Err(e) = res {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Print(print_args) => {
            let _repo = setup_repo(&cli, &repository_folder)?;
            bookmark_print(print_args, repository_folder)?;
        }
        Commands::Add(add_args) => {
            let repo = setup_repo(&cli, &repository_folder)?;
            let res = bookmark_add(add_args, repository_folder, &repo, cli.repository_url);

            if let Err(e) = res {
                error!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Rm(rm_args) => {
            let repo = setup_repo(&cli, &repository_folder)?;
            let res = bookmark_rm(rm_args, repository_folder, &repo, cli.repository_url);

            if let Err(e) = res {
                error!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Find(find_args) => {
            let res = bookmark_find(find_args, repository_folder);

            if let Err(e) = res {
                error!("Error: {}", e);
                std::process::exit(1);
            }
        }
    };

    Ok(())
}

#[derive(Error, Debug)]
enum InitError {
    #[error("Error: config path ({0}) already exists")]
    ConfigExists(PathBuf),
    #[error(transparent)]
    TomlError(#[from] TomlError),
    #[error("Please provide git repository url (use floccus-cli --help for more information)")]
    GitRepositoryNotProvided,
    #[error("Error while writing config file or creating parent folders for: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Unable to get parents for: {0}")]
    NoParent(PathBuf),
}

fn init_app(cli: &Cli, _init_args: &InitArgs, config_path: &Path) -> Result<(), InitError> {
    debug!("Config file path: {:?}", config_path);

    if config_path.exists() {
        return Err(InitError::ConfigExists(config_path.to_path_buf()));
    }

    if cli.repository_url.is_none() {
        return Err(InitError::GitRepositoryNotProvided);
    }

    let mut config_doc = FLOCCUS_CLI_CONFIG_SAMPLE.parse::<DocumentMut>()?;
    // println!("config: {}", config_doc);

    let repository_url = cli.repository_url.as_ref().unwrap().clone();
    config_doc["git"]["repository_url"] = value(repository_url.to_string());
    
    if let Some(repository_token) = cli.repository_token.as_ref() {
        config_doc["git"]["repository_token"] = value(repository_token);
    }
    
    // FIXME: only for ssh url
    config_doc["git"]["repository_ssh_key"] = value(cli.repository_ssh_key.display().to_string());

    debug!("New config: {}", config_doc);

    let config_path_parent = config_path
        .parent()
        .ok_or(InitError::NoParent(config_path.to_path_buf()))?;
    std::fs::create_dir_all(config_path_parent)?;

    let mut f = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(config_path)?;
    f.write_all(config_doc.to_string().as_bytes())?;

    info!("Successfully written config file path: {:?}", config_path);

    Ok(())
}

fn setup_repo(cli: &Cli, repository_folder: &Path) -> Result<Repository, Box<dyn Error>> {
    let mut repository_need_pull = true; // no need to pull after a clone (for instance)

    let repo = if !repository_folder.exists() {
        // repository folder does not exist - need to clone

        // first check if repository url is provided
        if cli.repository_url.is_none() {
            return Err("Please provide a git repository url".into());
        }
        let repository_url = cli.repository_url.as_ref().unwrap();

        let repo = git_clone(repository_url, repository_folder, Some(cli.repository_ssh_key.as_path()))?;
        repository_need_pull = false;
        repo
    } else {
        Repository::open(repository_folder)?
    };

    // ~ git pull
    if repository_need_pull {
        // TODO: get current branch name from repo?
        let mut remote = repo.find_remote("origin")?;
        let remote_branch = "main";
        let fetch_commit = git_fetch(&repo, &[remote_branch], &mut remote)?;
        git_merge(&repo, remote_branch, fetch_commit)?;
    }

    {
        // Get the HEAD reference
        let head = &repo.head()?;
        // Get the commit associated with the HEAD reference
        let commit = &repo.find_commit(head.target().unwrap())?;
        info!("Repository at commit: {:?}: {:?}", commit, commit.message());
    }

    Ok(repo)
}

fn bookmark_print(
    _print_args: &PrintArgs,
    repository_folder: PathBuf,
) -> Result<(), Box<dyn Error>> {
    const FOLDER_EMOTICON: &str = "\u{1F4C1}";
    const _FOLDER_LINK: &str = "\u{1F310}";
    const FOLDER_LINK1: &str = "\u{1F517}";
    const INDENTER: fn(usize) -> String = |indent_spaces| " ".repeat(indent_spaces);

    let bookmark_file_path = repository_folder.join("bookmarks.xbel");
    let xbel = Xbel::from_file(bookmark_file_path)?;

    let xbel_it = XbelNestingIterator::new(&xbel);
    let mut indent_spaces = 0;
    for item in xbel_it {
        match item {
            XbelItemOrEnd::End(_) => indent_spaces -= 2,
            XbelItemOrEnd::Item(XbelItem::Folder(f)) => {
                println!(
                    "{}[{FOLDER_EMOTICON} {}] {}",
                    INDENTER(indent_spaces),
                    f.id,
                    f.title.text
                );
                indent_spaces += 2;
            }
            XbelItemOrEnd::Item(XbelItem::Bookmark(b)) => {
                let indent = INDENTER(indent_spaces);
                println!("{}[{FOLDER_LINK1} {}] {}", indent, b.id, b.title.text);
                println!("{}- {}", indent, b.href);
            }
        }
    }

    Ok(())
}

impl From<&Under> for XbelPath {
    fn from(value: &Under) -> Self {
        match value {
            Under::Root => XbelPath::Root,
            Under::Id(id, _) => XbelPath::Id(*id),
            Under::Folder(p) => XbelPath::Path(p.clone()),
        }
    }
}

#[derive(Error, Debug)]
enum BookmarkAddError {
    #[error("Error: please provide git repository url (or use --disable-push)")]
    PushWithoutUrl,
    #[error(transparent)]
    XbelReadError(#[from] XbelError),
    #[error("Cannot find anything in Xbel matching: {0}")]
    XbelPathNotFound(XbelPath),
    #[error("Item found with id: {0} but it is not a folder")]
    NotaFolder(String),
    // TODO: remap error GitAddError, GitCommitError ...
    #[error(transparent)]
    GitError(#[from] git2::Error),
}

fn bookmark_add(
    add_args: &AddArgs,
    repository_folder: PathBuf,
    repo: &Repository,
    repository_url: Option<Url>,
) -> Result<(), BookmarkAddError> {
    if add_args.disable_push == Some(false) && repository_url.is_none() {
        return Err(BookmarkAddError::PushWithoutUrl);
    }

    // Read xbel
    let bookmark_file_path_xbel = PathBuf::from("bookmarks.xbel");
    let bookmark_file_path = repository_folder.join(bookmark_file_path_xbel.as_path());
    let mut xbel = Xbel::from_file(&bookmark_file_path)?;

    // Build the bookmark
    let bookmark = xbel.new_bookmark(add_args.url.as_str(), add_args.title.as_str());

    // Find where to put the bookmark
    let xbel_path = XbelPath::from(&add_args.under);
    let (item_index, items) = xbel
        .get_items_mut(&xbel_path)
        .ok_or(BookmarkAddError::XbelPathNotFound(xbel_path.clone()))?;

    match xbel_path {
        XbelPath::Root => items.push(bookmark),
        XbelPath::Id(id) => {
            if let Under::Id(_id, placement) = &add_args.under {
                match placement {
                    Placement::Before => {
                        items.insert(item_index, bookmark);
                    }
                    Placement::After => {
                        items.insert(item_index.saturating_add(1), bookmark);
                    }
                    Placement::InFolderPrepend => {
                        if let XbelItem::Folder(f) = &mut items[item_index] {
                            f.items.insert(0, bookmark)
                        } else {
                            return Err(BookmarkAddError::NotaFolder(id.to_string()));
                        }
                    }
                    Placement::InFolderAppend => {
                        if let XbelItem::Folder(f) = &mut items[item_index] {
                            f.items.push(bookmark)
                        } else {
                            return Err(BookmarkAddError::NotaFolder(id.to_string()));
                        }
                    }
                }
            } else {
                unreachable!()
            }
        }
        XbelPath::Path(_s) => {
            if let XbelItem::Folder(f) = &mut items[item_index] {
                f.items.push(bookmark)
            } else {
                return Err(BookmarkAddError::NotaFolder(
                    items[item_index].get_id().to_string(),
                ));
            }
        }
    };

    debug!("xbel: {:?}", xbel);
    // Write to file locally
    xbel.to_file(bookmark_file_path)?;

    if add_args.disable_push == Some(false) {
        git_push(repo, bookmark_file_path_xbel.as_path())?;
    }

    Ok(())
}

#[derive(Error, Debug)]
enum BookmarkRemoveError {
    #[error("Error: please provide git repository url (or use --disable-push)")]
    PushWithoutUrl,
    #[error(transparent)]
    XbelReadError(#[from] XbelError),
    #[error("Cannot find anything in Xbel matching: {0}")]
    XbelPathNotFound(XbelPath),
    // // TODO: remap error GitAddError, GitCommitError ...
    #[error(transparent)]
    GitError(#[from] git2::Error),
}

fn bookmark_rm(
    rm_args: &RemoveArgs,
    repository_folder: PathBuf,
    repo: &Repository,
    repository_url: Option<Url>,
) -> Result<(), BookmarkRemoveError> {
    if rm_args.disable_push == Some(false) && repository_url.is_none() {
        return Err(BookmarkRemoveError::PushWithoutUrl);
    }

    // Read xbel file
    let bookmark_file_path_xbel = PathBuf::from("bookmarks.xbel");
    let bookmark_file_path = repository_folder.join(bookmark_file_path_xbel.as_path());
    let mut xbel = Xbel::from_file(&bookmark_file_path)?;

    // Find where to put the bookmark
    let xbel_path = XbelPath::from(&rm_args.under);
    let (item_index, items) = xbel
        .get_items_mut(&xbel_path)
        .ok_or(BookmarkRemoveError::XbelPathNotFound(xbel_path.clone()))?;

    match xbel_path {
        XbelPath::Root => {
            // TODO: return Error
            unimplemented!()
        }
        XbelPath::Id(_id) => {
            if rm_args.dry_run {
                match &items[item_index] {
                    XbelItem::Folder(f) => {
                        // XXX:
                        // Folder Debug print folder + all children recursively
                        // Maybe we could print only the folder + children count ?
                        println!("[Dry run] removing folder: {:?}", f);
                    }
                    XbelItem::Bookmark(b) => {
                        println!("[Dry run] removing bookmark: {:?}", b);
                    }
                }
            } else {
                items.remove(item_index);
            }
        }
        XbelPath::Path(_s) => {
            if rm_args.dry_run {
                match &items[item_index] {
                    XbelItem::Folder(f) => {
                        // TODO: print all children or just print the children count (recursive)
                        println!("[Dry run] removing folder: {:?}", f);
                    }
                    XbelItem::Bookmark(b) => {
                        println!("[Dry run] removing bookmark: {:?}", b);
                    }
                }
            } else {
                // TODO: print
                items.remove(item_index);
            }
        }
    }

    // Write to file locally
    xbel.to_file(bookmark_file_path)?;

    if rm_args.disable_push == Some(false) {
        git_push(repo, bookmark_file_path_xbel.as_path())?;
    }

    Ok(())
}

#[derive(Error, Debug)]
enum BookmarkFindError {
    #[error(transparent)]
    XbelReadError(#[from] XbelError),
}

enum FindKind {
    All,
    Folder,
    Bookmark,
}

enum FindWhere {
    All,
    Title,
    Url,
}

fn bookmark_find(
    find_args: &FindArgs,
    repository_folder: PathBuf,
) -> Result<(), BookmarkFindError> {
    let find_kind = if find_args.folder {
        FindKind::Folder
    } else if find_args.bookmark {
        FindKind::Bookmark
    } else {
        FindKind::All
    };

    let find_where = if find_args.title {
        FindWhere::Title
    } else if find_args.url {
        FindWhere::Url
    } else {
        FindWhere::All
    };

    // Read xbel file
    let bookmark_file_path_xbel = PathBuf::from("bookmarks.xbel");
    let bookmark_file_path = repository_folder.join(bookmark_file_path_xbel.as_path());
    let xbel = Xbel::from_file(&bookmark_file_path)?;

    let found_in_title = |item: &XbelItem, to_match: &str| item.get_title().text.contains(to_match);
    let found_in_url = |item: &XbelItem, to_match: &str| {
        item.get_url().unwrap_or(&"".to_string()).contains(to_match)
    };
    let items: Vec<&XbelItem> = xbel
        .into_iter()
        .filter(|i| {
            let match_kind = match find_kind {
                FindKind::Folder => matches!(i, XbelItem::Folder(_)),
                FindKind::Bookmark => matches!(i, XbelItem::Bookmark(_)),
                FindKind::All => true,
            };

            if !match_kind {
                false
            } else {
                match find_where {
                    FindWhere::Title => found_in_title(i, find_args.find.as_str()),
                    FindWhere::Url => found_in_url(i, find_args.find.as_str()),
                    FindWhere::All => {
                        let to_find = find_args.find.as_str();
                        found_in_title(i, to_find) || found_in_url(i, to_find)
                    }
                }
            }
        })
        .collect();

    if items.is_empty() {
        let msg = match find_kind {
            FindKind::All => "Found 0 bookmark or folder",
            FindKind::Folder => "Found 0 folder",
            FindKind::Bookmark => "Found 0 bookmark",
        };
        println!("{}", msg);
    } else {
        let msg = match find_kind {
            FindKind::All => format!(
                "Found {} {} or {}:",
                items.len(),
                pluralize("folder", items.len()),
                pluralize("bookmark", items.len()),
            ),
            FindKind::Folder => format!(
                "Found {} {}:",
                items.len(),
                pluralize("folder", items.len())
            ),
            FindKind::Bookmark => format!(
                "Found {} {}:",
                items.len(),
                pluralize("bookmark", items.len())
            ),
        };

        println!("{}", msg);
        for (idx, i) in items.iter().enumerate() {
            println!("{}- {:?}", idx, i);
        }
    }

    Ok(())
}

fn pluralize(s: &str, count: usize) -> Cow<'_, str> {
    match count {
        0 | 1 => Cow::Borrowed(s),
        _ => Cow::Owned(format!("{}s", s)),
    }
}
