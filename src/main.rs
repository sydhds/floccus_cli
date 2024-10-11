mod git;
mod xbel;

// std
use std::error::Error;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::str::FromStr;
// third-party
use clap::{Args, Parser, Subcommand};
use directories::ProjectDirs;
use git2::Repository;
use quick_xml::de::from_reader;
use thiserror::Error;
// internal
use crate::git::{git_clone, git_fetch, git_merge};
use crate::xbel::{Xbel, XbelItem, XbelItemOrEnd, XbelNestingIterator, XbelPath};

#[derive(Debug, Clone, Parser)]
#[command(name = "clap-subcommand")]
#[command(about = "Clap subcommand example", long_about = None)]
pub struct Cli {
    #[arg(
        short = 'r',
        long = "repository",
        help = "(Optional) git repository path"
    )]
    repository_folder: Option<PathBuf>,
    #[arg(
        short = 'g',
        long = "git",
        help = "Git repository url, e.g.https://github.com/your_username/your_repo.git"
    )]
    repository_url: Option<String>,
    #[arg(
        short = 'n',
        long = "name",
        help = "Repository local name",
        default_value = "bookmarks"
    )]
    repository_name: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, PartialEq, Subcommand)]
pub(crate) enum Commands {
    #[command(about = "Print bookmarks")]
    Print(PrintArgs),
    #[command(about = "Add bookmark(s)")]
    Add(AddArgs),
    #[command(about = "Remove bookmark(s)")]
    Rm(RemoveArgs),
}

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

                let (rem, placement) = if s.starts_with(PLACEMENT_AFTER_PREFIX) {
                    (&s[PLACEMENT_AFTER_PREFIX.len()..], Placement::After)
                } else if s.starts_with(PLACEMENT_BEFORE_PREFIX) {
                    (&s[PLACEMENT_BEFORE_PREFIX.len()..], Placement::Before)
                } else if s.starts_with(PLACEMENT_APPEND_PREFIX) {
                    (&s[PLACEMENT_APPEND_PREFIX.len()..], Placement::InFolderAppend)
                } else if s.starts_with(PLACEMENT_PREPEND_PREFIX) {
                    (&s[PLACEMENT_PREPEND_PREFIX.len()..], Placement::InFolderPrepend)
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

impl From<&Under> for XbelPath {
    fn from(value: &Under) -> Self {
        match value {
            Under::Root => XbelPath::Root,
            Under::Id(id, _) => XbelPath::Id(*id),
            Under::Folder(p) => XbelPath::Path(p.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct AddArgs {
    #[arg(short = 'b', long = "bookmark", help = "Url to add")]
    url: String,
    #[arg(short = 't', long = "title", help = "Url title or description")]
    title: String,
    #[arg(short = 'u', long = "under", help = "Add bookmark under ...", default_value = "root", value_parser=under_parser)]
    under: Under,
    #[arg(
        long = "disable-push",
        help = "Add the new bookmark locally but do not push (git push) it",
        action,
        required = false
    )]
    disable_push: bool,
}

// FIXME: Result error fix
fn under_parser(s: &str) -> Result<Under, &'static str> {
    Under::from_str(s).map_err(|_| "cannot parse under argument")
}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct RemoveArgs {
    #[arg(short = 'i', long = "item", help = "Remove bookmark or folder", value_parser=under_parser)]
    under: Under,
    #[arg(
        long = "disable-push",
        help = "Add the new bookmark locally but do not push (git push) it",
        action,
        required = false
    )]
    disable_push: bool,
    #[arg(
        long = "dry-run",
        help = "Do not remove - just print",
        action,
        required = false
    )]
    dry_run: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    println!("cli: {:?}", cli);

    // if repo folder is provided - use it otherwise - use a local data dir
    let repository_folder = if let Some(repository_folder) = cli.repository_folder {
        repository_folder
    } else {
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
        repository_need_pull = true;

        /*
        let head = &repo.head()?;
        // Get the commit associated with the HEAD reference
        // FIXME: unwrap
        let commit = &repo.find_commit(head.target().unwrap())?;
        println!(
            "Cloned repository at commit: {:?}: {:?}",
            commit,
            commit.message()
        );
        */

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
    let xbel_ = std::fs::File::open(bookmark_file_path)?;
    let xbel: Xbel = from_reader(BufReader::new(xbel_))?;

    // TODO: use iterator here?
    /*
    let indent_spaces = 0;
    for item in xbel.items.iter() {
        print_xbel_item(item, indent_spaces);
    }
    */

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

/*
fn print_xbel_item(item: &XbelItem, indent_spaces: usize) {
    const FOLDER_EMOTICON: &str = "\u{1F4C1}";
    const _FOLDER_LINK: &str = "\u{1F310}";
    const FOLDER_LINK1: &str = "\u{1F517}";
    const INDENTER: fn(usize) -> String = |indent_spaces| " ".repeat(indent_spaces);

    match item {
        XbelItem::Folder(f) => {
            println!(
                "{}[{FOLDER_EMOTICON} {}] {}",
                INDENTER(indent_spaces),
                f.id,
                f.title.text
            );
            for item in f.items.iter() {
                print_xbel_item(item, indent_spaces + 2)
            }
        }
        XbelItem::Bookmark(b) => {
            let indent = INDENTER(indent_spaces + 2);
            println!("{}[{FOLDER_LINK1} {}] {}", indent, b.id, b.title.text);
            println!("{}- {}", indent, b.href);
        }
    }
}
*/

#[derive(Error, Debug)]
enum BookmarkAddError {
    #[error("Error: please provide git repository url (or use --disable-push)")]
    PushWithoutUrl,
    #[error("Error while reading Xbel file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Cannot read Xbel file: {0}")]
    XbelReadError(#[from] quick_xml::de::DeError),
    #[error("Cannot find anything in Xbel matching: {0}")]
    XbelPathNotFound(XbelPath),
    #[error("Cannot find anything in Xbel matching id: {0}")]
    IdNotFound(String),
    #[error("Cannot find anything in Xbel matching path: {0}")]
    PathNotFound(String),
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
    if !add_args.disable_push && repository_url.is_none() {
        return Err(BookmarkAddError::PushWithoutUrl);
    }

    // Read xbel
    let bookmark_file_path_xbel = PathBuf::from("bookmarks.xbel");
    let bookmark_file_path = repository_folder.join(bookmark_file_path_xbel.as_path());
    let xbel_ = std::fs::File::open(bookmark_file_path.as_path())?;
    let mut xbel: Xbel = from_reader(BufReader::new(xbel_))?;

    // Build the bookmark
    let bookmark = xbel.new_bookmark(add_args.url.as_str(), add_args.title.as_str());

    // Find where to put the bookmark
    let xbel_path = XbelPath::from(&add_args.under);
    let items = xbel
        .get_items_mut(&xbel_path)
        .ok_or(BookmarkAddError::XbelPathNotFound(xbel_path.clone()))?;

    match xbel_path {
        XbelPath::Root => items.push(bookmark),
        XbelPath::Id(id) => {
            
            if let Under::Id(_id, placement) = &add_args.under {
                // Find the item (matching id) inside items
                let (item, item_idx) = items
                    .iter_mut()
                    .enumerate()
                    .find_map(|(idx, i)| { 
                        if *i.get_id() == id.to_string() { Some((i, idx)) } else { None } 
                    })
                    .ok_or(BookmarkAddError::IdNotFound(id.to_string()))?;
                
                match placement {
                    Placement::Before => {
                        items.insert(item_idx, bookmark);
                    },
                    Placement::After => {
                        items.insert(item_idx.saturating_add(1), bookmark);
                    },
                    Placement::InFolderPrepend => {
                        if let XbelItem::Folder(f) = item {
                            f.items.insert(0, bookmark)
                        } else {
                            return Err(BookmarkAddError::NotaFolder(id.to_string()));
                        }
                    },
                    Placement::InFolderAppend => {
                        if let XbelItem::Folder(f) = item {
                            f.items.push(bookmark)
                        } else {
                            return Err(BookmarkAddError::NotaFolder(id.to_string()));
                        }
                    },
                }
                
            } else {
                unreachable!()
            }
        }
        XbelPath::Path(s) => {
            
            let path_split_last = s
                .rsplit_once('/')// split from the end
                .map(|(_rest, s)| s.to_string()) // discard rest and to_string
                .unwrap_or(s.clone());
            
            // Find the item (matching last path) inside items
            let (item, item_idx) = items
                .iter_mut()
                .enumerate()
                .find_map(|(idx, i)| {
                    if *i.get_title().text == path_split_last { Some((i, idx)) } else { None }
                })
                .ok_or(BookmarkAddError::PathNotFound(s.to_string()))?;
            
            if let XbelItem::Folder(f) = item {
                f.items.push(bookmark)
            } else {
                return Err(BookmarkAddError::NotaFolder(item.get_id().to_string()));
            }
        },
    };

    println!("xbel: {:?}", xbel);
    {
        let mut f = std::fs::File::options()
            .write(true)
            .truncate(true)
            .open(bookmark_file_path.as_path())?;

        let buffer = xbel.write_to_string();
        f.write_all(buffer.as_bytes())?;
    }

    if !add_args.disable_push {
        // Configured author signature
        let author = repo.signature()?;

        // git add
        let status = repo.status_file(bookmark_file_path_xbel.as_path())?;
        println!("status: {:?}", status);
        let mut index = repo.index()?;
        index.add_path(bookmark_file_path_xbel.as_path())?;
        // the modified in-memory index need to flush back to disk
        index.write()?;

        // git commit

        // returns the object id you can use to look up the actual tree object
        let new_tree_oid = index.write_tree()?;
        // this is our new tree, i.e. the root directory of the new commit
        let new_tree = repo.find_tree(new_tree_oid)?;

        // for simple commit, use current head as parent
        // TODO: test
        // you need more than one parent if the commit is a merge
        let head = repo.head()?;
        // FIXME: unwrap
        let parent = repo.find_commit(head.target().unwrap())?;

        let _commit_oid = repo.commit(
            Some("HEAD"),
            &author,
            &author,
            "Floccus bookmarks update",
            &new_tree,
            &[&parent],
        )?;

        // git push
        let mut origin = repo.find_remote("origin")?;
        origin.push(&["refs/heads/main:refs/heads/main"], None)?;
    }

    Ok(())
}

#[derive(Error, Debug)]
enum BookmarkRemoveError {
    #[error("Error: please provide git repository url (or use --disable-push)")]
    PushWithoutUrl,
    #[error("Error while reading Xbel file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Cannot read Xbel file: {0}")]
    XbelReadError(#[from] quick_xml::de::DeError),
    #[error("Cannot find anything in Xbel matching: {0}")]
    XbelPathNotFound(XbelPath),
    #[error("Cannot find anything in Xbel matching id: {0}")]
    IdNotFound(String),
    #[error("Cannot find anything in Xbel matching path: {0}")]
    PathNotFound(String),
    // #[error("Item found with id: {0} but it is not a folder")]
    // NotaFolder(String),
    // // TODO: remap error GitAddError, GitCommitError ...
    // #[error(transparent)]
    // GitError(#[from] git2::Error),
}

fn bookmark_rm(
    rm_args: &RemoveArgs,
    repository_folder: PathBuf,
    repo: &Repository,
    repository_url: Option<String>,
) -> Result<(), BookmarkRemoveError> {
    
    if !rm_args.disable_push && repository_url.is_none() {
        return Err(BookmarkRemoveError::PushWithoutUrl);
    }

    // TODO: xbel::from_file
    let bookmark_file_path_xbel = PathBuf::from("bookmarks.xbel");
    let bookmark_file_path = repository_folder.join(bookmark_file_path_xbel.as_path());
    let xbel_ = std::fs::File::open(bookmark_file_path.as_path())?;
    let mut xbel: Xbel = from_reader(BufReader::new(xbel_))?;
    
    // Find where to put the bookmark
    let xbel_path = XbelPath::from(&rm_args.under);
    // TODO: return (items, item_index)
    let items = xbel
        .get_items_mut(&xbel_path)
        .ok_or(BookmarkRemoveError::XbelPathNotFound(xbel_path.clone()))?;
    
    match xbel_path {
        XbelPath::Root => {
            // TODO: return Error
            unimplemented!()
        }
        XbelPath::Id(id) => {
            
            // Find the item (matching id) inside items
            let (item, item_idx) = items
                .iter_mut()
                .enumerate()
                .find_map(|(idx, i)| {
                    if *i.get_id() == id.to_string() { Some((i, idx)) } else { None }
                })
                .ok_or(BookmarkRemoveError::IdNotFound(id.to_string()))?;
            
            if rm_args.dry_run {
                match item {
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
                items.remove(item_idx);
            }
        }
        XbelPath::Path(s) => {

            let path_split_last = s
                .rsplit_once('/')// split from the end
                .map(|(_rest, s)| s.to_string()) // discard rest and to_string
                .unwrap_or(s.clone());

            // Find the item (matching last path) inside items
            let (item, item_idx) = items
                .iter_mut()
                .enumerate()
                .find_map(|(idx, i)| {
                    if *i.get_title().text == path_split_last { Some((i, idx)) } else { None }
                })
                .ok_or(BookmarkRemoveError::PathNotFound(s.to_string()))?;

            if rm_args.dry_run {
                match item {
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
                items.remove(item_idx);
            }
        }
    }
    
    {
        // TODO: Xbel::to_file
        let mut f = std::fs::File::options()
            .write(true)
            .truncate(true)
            .open(bookmark_file_path.as_path())?;

        let buffer = xbel.write_to_string();
        f.write_all(buffer.as_bytes())?;
    }
    
    if !rm_args.disable_push {
        unimplemented!()
    }
    
    Ok(())
}