mod xbel;
mod git;

// std
use std::error::Error;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
// third-party
use clap::{Args, Parser, Subcommand};
use directories::ProjectDirs;
use git2::Repository;
use quick_xml::de::from_reader;
use thiserror::Error;
// internal
use crate::xbel::{XbelItem, Xbel, XbelPath, XbelNestingIterator, XbelItemOrEnd};
use crate::git::{git_clone, git_fetch, git_merge};

#[derive(Debug, Clone, Parser)]
#[command(name = "clap-subcommand")]
#[command(about = "Clap subcommand example", long_about = None)]
pub struct Cli {
    #[arg(
        short = 'r',
        long = "repository",
        help = "(Optional) git repository path")]
    repository_folder: Option<PathBuf>,
    #[arg(
        short = 'g',
        long = "git",
        help = "Git repository url, e.g.https://github.com/your_username/your_repo.git"
    )]
    repository_url: Option<String>,
    #[arg(short = 'n', long = "name", help = "Repository local name", default_value="bookmarks")]
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
pub enum Under {
    Root,
    Id(u64),
    Folder(PathBuf)
}

impl FromStr for Under {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s { 
            "root" => Ok(Under::Root),
            _ => {
                if let Ok(s_id) = s.parse::<u64>() {
                    Ok(Under::Id(s_id))
                } else {
                    Ok(Under::Folder(PathBuf::from(s)))
                }
            }
        }
    }
}

impl From<&Under> for XbelPath {
    fn from(value: &Under) -> Self {
        match value {
            Under::Root => XbelPath::Root,
            Under::Id(id) => XbelPath::Id(*id), 
            _ => unimplemented!()
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
    #[arg(long = "disable-push", help = "Add the new bookmark locally but do not push (git push) it", action, required=false)]
    disable_push: bool,
}

fn under_parser(s: &str) -> Result<Under, &'static str> {
    match s {
        "root" => Ok(Under::Root),
        _ => {
            if let Ok(s_id) = s.parse::<u64>() {
                Ok(Under::Id(s_id))
            } else {
                Ok(Under::Folder(PathBuf::from(s)))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Args)]
pub struct RemoveArgs {
    #[arg(short = 'i', long = "item", help = "Remove bookmark or folder", value_parser=under_parser)]
    under: Under,
    #[arg(long = "disable-push", help = "Add the new bookmark locally but do not push (git push) it", action, required=false)]
    disable_push: bool,
}

fn main() -> Result<(), Box<dyn Error>> {

    let cli = Cli::parse();
    println!("cli: {:?}", cli);

    let (repository_folder, repo) = if let Some(repository_url) = cli.repository_url {
        
        // repository url provided by user -> should clone

        // if repo folder is provided - use it else use a local data dir
        let repository_folder = if let Some(repository_folder) = cli.repository_folder {
            repository_folder
        } else {
            ProjectDirs::from("org", "Floccus",  "Floccus-cli")
                .ok_or("Unable to determine local data directory")?
                .data_local_dir()
                .join(cli.repository_name)
        };

        let repo = git_clone(repository_url.as_str(), repository_folder.as_path())?;
        let head = repo.head()?;
        // Get the commit associated with the HEAD reference
        let commit = repo.find_commit(head.target().unwrap())?;
        println!(
            "(Cloned) Repository at commit: {:?}: {:?}",
            commit,
            commit.message()
        );

        let repo = Repository::open(repository_folder.as_path())?;
        (repository_folder, repo)

    } else {

        // No repository url provided -> open repository
        // if repo folder is provided - use it else use a local data dir
        let repository_folder = if let Some(repository_folder) = cli.repository_folder {
            repository_folder
        } else {
            ProjectDirs::from("org", "Floccus",  "Floccus-cli")
                .ok_or("Unable to determine local data directory")?
                .data_local_dir()
                .join(cli.repository_name)
        };

        if !repository_folder.exists() {
            panic!("Could not find the git repository folder on disk, run first with: -g https://github.com/your_username/your_repo.git")
        }

        let repo = Repository::open(repository_folder.as_path())?;
        // Get the HEAD reference
        let head = repo.head()?;
        // Get the commit associated with the HEAD reference
        let commit = repo.find_commit(head.target().unwrap())?;
        println!(
            "(Opened 1) Repository at commit: {:?}: {:?}",
            commit,
            commit.message()
        );

        // ~ git pull
        // TODO: get current branch name from repo?
        let mut remote = repo.find_remote("origin")?;
        let remote_branch = "main";
        let fetch_commit = git_fetch(&repo, &[remote_branch], &mut remote)?;
        git_merge(&repo, remote_branch, fetch_commit)?;

        // FIXME: no re-open ?
        let repo = Repository::open(repository_folder.as_path())?;
        (repository_folder, repo)
    };
    
    match cli.command {
        Commands::Print(print_args) => {
            bookmark_print(&print_args, repository_folder)?;
        }
        Commands::Add(add_args) => {
            let res = bookmark_add(&add_args, repository_folder, &repo);
            match res {
                Err(e) => eprintln!("Error: {}", e), // FIXME: should we exit with code 1?
                _ => {}
            }
        }
        Commands::Rm(_) => {
            unimplemented!()
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
                    INDENTER(indent_spaces), f.id, f.title.text
                );
                indent_spaces += 2;
            },
            XbelItemOrEnd::Item(XbelItem::Bookmark(b)) => {
                let indent = INDENTER(indent_spaces);
                println!("{}[{FOLDER_LINK1} {}] {}", indent, b.id, b.title.text);
                println!("{}- {}", indent, b.href);
            },
        }
    }

    Ok(())
}

fn print_xbel_item(item: &XbelItem, indent_spaces: usize) {
    const FOLDER_EMOTICON: &str = "\u{1F4C1}";
    const _FOLDER_LINK: &str = "\u{1F310}";
    const FOLDER_LINK1: &str = "\u{1F517}";
    const INDENTER: fn(usize) -> String = |indent_spaces| " ".repeat(indent_spaces);

    match item {
        XbelItem::Folder(f) => {
            println!(
                "{}[{FOLDER_EMOTICON} {}] {}",
                INDENTER(indent_spaces), f.id, f.title.text
            );
            for item in f.items.iter() {
                print_xbel_item(item, indent_spaces + 2)
            }
        },
        XbelItem::Bookmark(b) => {
            let indent = INDENTER(indent_spaces + 2);
            println!("{}[{FOLDER_LINK1} {}] {}", indent, b.id, b.title.text);
            println!("{}- {}", indent, b.href);
        }
    }
}

#[derive(Error, Debug)]
enum BookmarkAddError {
    #[error("Error while reading Xbel file")]
    IoError(#[from] std::io::Error),
    #[error("Cannot read Xbel file")]
    XbelReadError(#[from] quick_xml::de::DeError),
    #[error("Cannot find anything in Xbel matching: {0}")]
    XbelPathNotFound(XbelPath),
    #[error("Cannot find anything in Xbel matching id: {0}")]
    IdNotFound(String),
    #[error("Item found with id: {0} is not a folder")]
    NotaFolder(String),
    // TODO: remap error GitAddError, GitCommitError ...
    #[error("Git error")]
    GitError(#[from] git2::Error),
}

fn bookmark_add(add_args: &AddArgs, repository_folder: PathBuf, repo: &Repository) -> Result<(), BookmarkAddError> {
    let bookmark_file_path_xbel = PathBuf::from("bookmarks.xbel");
    let bookmark_file_path = repository_folder.join(bookmark_file_path_xbel.as_path());
    let xbel_ = std::fs::File::open(bookmark_file_path.as_path())?;
    let mut xbel: Xbel = from_reader(BufReader::new(xbel_))?;

    // Build the bookmark
    let highest_id = xbel.get_highest_id();
    // TODO: Xbel::new_bookmark + auto id ?
    let bookmark = XbelItem::new_bookmark(
        (highest_id + 1).to_string().as_str(), add_args.url.as_str(), add_args.title.as_str()
    );

    // Find where to put the bookmark
    let xbel_path = XbelPath::from(&add_args.under);
    let items = xbel
        .get_items_mut(&xbel_path)
        .ok_or(BookmarkAddError::XbelPathNotFound(xbel_path.clone()))?;

    let items = match xbel_path {
        XbelPath::Root => {
            items
        },
        XbelPath::Id(id) => {
            // Note:
            // items == the items containing a Folder/Bookmark with the requested id
            // , but we want to add the bookmark inside the folder

            let item = items
                .iter_mut()
                .find(|i| {
                    *i.get_id() == id.to_string()
                })
                .ok_or(BookmarkAddError::IdNotFound(id.to_string()))?;

            match item {
                XbelItem::Folder(f) => &mut f.items,
                _ => return Err(
                    BookmarkAddError::IdNotFound(id.to_string())
                ),
            }
        }
        XbelPath::Path(_) => unimplemented!()
    };
    items.push(bookmark);

    println!("xbel: {:?}", xbel);
    {
        let mut f = std::fs::File::options()
            .write(true)
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

        // returns the object id you can use to lookup the actual tree object
        let new_tree_oid = index.write_tree().unwrap();
        // this is our new tree, i.e. the root directory of the new commit
        let new_tree = repo.find_tree(new_tree_oid).unwrap();

        // for simple commit, use current head as parent
        // TODO: test
        // you need more than one parent if the commit is a merge
        let head = repo.head().unwrap();
        let parent = repo.find_commit(head.target().unwrap()).unwrap();

        repo
            .commit(
                Some("HEAD"),
                &author,
                &author,
                "Floccus bookmarks update",
                &new_tree,
                &[&parent],
            )
            .unwrap();
    }

    Ok(())
}