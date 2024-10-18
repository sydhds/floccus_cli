mod cli;
mod git;
mod xbel;

// std
use std::borrow::Cow;
use std::error::Error;
use std::path::PathBuf;
// third-party
use clap::Parser;
use directories::ProjectDirs;
use git2::Repository;
use thiserror::Error;
// internal
use crate::cli::{
    parse_cli_and_override, AddArgs, Cli, Commands, FindArgs, Placement, PrintArgs, RemoveArgs,
    Under,
};
use crate::git::{git_clone, git_fetch, git_merge, git_push};
use crate::xbel::{Xbel, XbelError, XbelItem, XbelItemOrEnd, XbelNestingIterator, XbelPath};

impl From<&Under> for XbelPath {
    fn from(value: &Under) -> Self {
        match value {
            Under::Root => XbelPath::Root,
            Under::Id(id, _) => XbelPath::Id(*id),
            Under::Folder(p) => XbelPath::Path(p.clone()),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config_path: Option<PathBuf> = {
        // if FLOCCUS_CLI_CONFIG is set use it, otherwise find local config directory
        // FIXME: const
        let config_env = std::env::var("FLOCCUS_CLI_CONFIG");
        if let Ok(config_env) = config_env {
            Some(PathBuf::from(config_env))
        } else {
            // FIXME: const
            let cfg = ProjectDirs::from("org", "Floccus", "Floccus-cli")
                .ok_or("Unable to determine local data directory")?
                .config_local_dir()
                .to_path_buf()
                .join("config.toml");

            if cfg.exists() {
                Some(cfg)
            } else {
                None
            }
        }
    };

    println!("config_path: {:?}", config_path);

    let cli = parse_cli_and_override(config_path)?;

    // if repo folder is provided - use it otherwise - use a local data dir
    let repository_folder = if let Some(repository_folder) = cli.repository_folder {
        repository_folder
    } else {
        // FIXME: const
        ProjectDirs::from("org", "Floccus", "Floccus-cli")
            .ok_or("Unable to determine local data directory")?
            .data_local_dir()
            .join(cli.repository_name)
    };

    let mut repository_need_pull = true; // no need to pull after a clone (for instance)

    let repo = if !repository_folder.exists() {
        // repository folder does not exist - need to clone

        // first check if repository url is provided
        if cli.repository_url.is_none() {
            return Err("Please provide a git repository url".into());
        }
        let repository_url = cli.repository_url.as_ref().unwrap();

        let repo = git_clone(repository_url.as_str(), repository_folder.as_path())?;
        repository_need_pull = false;
        repo
    } else {
        Repository::open(repository_folder.as_path())?
    };

    // ~ git pull
    if repository_need_pull {
        // TODO: get current branch name from repo?
        let mut remote = repo.find_remote("origin")?;
        let remote_branch = "main";
        let fetch_commit = git_fetch(&repo, &[remote_branch], &mut remote)?;
        git_merge(&repo, remote_branch, fetch_commit)?;
    }

    // Get the HEAD reference
    let head = &repo.head()?;
    // Get the commit associated with the HEAD reference
    let commit = &repo.find_commit(head.target().unwrap())?;
    println!("Repository at commit: {:?}: {:?}", commit, commit.message());

    match cli.command {
        Commands::Print(print_args) => {
            bookmark_print(&print_args, repository_folder)?;
        }
        Commands::Add(add_args) => {
            let res = bookmark_add(&add_args, repository_folder, &repo, cli.repository_url);

            if let Err(e) = res {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Rm(rm_args) => {
            let res = bookmark_rm(&rm_args, repository_folder, &repo, cli.repository_url);

            if let Err(e) = res {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Find(find_args) => {
            let res = bookmark_find(&find_args, repository_folder);

            if let Err(e) = res {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    };

    Ok(())
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

    let it = XbelNestingIterator::new(&xbel);
    let mut indent_spaces = 0;
    for item in it {
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
    repository_url: Option<String>,
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

    println!("xbel: {:?}", xbel);
    // Write to file locally
    xbel.to_file(bookmark_file_path)?;

    if add_args.disable_push == Some(false) {
        // git_push(repo, bookmark_file_path_xbel.as_path())?;
        println!("Should git push");
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
    repository_url: Option<String>,
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
        // git_push(repo, bookmark_file_path_xbel.as_path())?;
        println!("Should git push");
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
