mod xbel;
mod git;

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
// internal
use crate::xbel::{XbelItem, Xbel, XbelPath};
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
            _ => unreachable!()
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
pub struct RemoveArgs {}


fn main() -> Result<(), Box<dyn Error>> {

    let cli = Cli::parse();
    println!("cli: {:?}", cli);

    let repository_folder = if let Some(repository_url) = cli.repository_url {
        
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

        repository_folder

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

        repository_folder
    };
    
    match cli.command {
        Commands::Print(print_args) => {
            bookmark_print(&print_args, repository_folder)?;
        }
        Commands::Add(add_args) => {
            bookmark_add(&add_args, repository_folder)?;
        }
        Commands::Rm(_) => {
            unimplemented!()
        }
    }

    Ok(())
}

fn bookmark_print(
    _print_args: &PrintArgs,
    repository_folder: PathBuf,
) -> Result<(), Box<dyn Error>> {
    let bookmark_file_path = repository_folder.join("bookmarks.xbel");
    let xbel_ = std::fs::File::open(bookmark_file_path)?;
    let xbel: Xbel = from_reader(BufReader::new(xbel_))?;
    
    // TODO: use iterator here?
    let indent_spaces = 0;
    for item in xbel.items.iter() {
        print_xbel_item(item, indent_spaces);
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

fn bookmark_add(add_args: &AddArgs, repository_folder: PathBuf) -> Result<(), Box<dyn Error>> {
    let bookmark_file_path = repository_folder.join("bookmarks.xbel");
    let xbel_ = std::fs::File::open(bookmark_file_path.as_path())?;
    let mut xbel: Xbel = from_reader(BufReader::new(xbel_))?;

    let highest_id = xbel.get_highest_id();
    let xbel_path = XbelPath::from(&add_args.under);

    let items_ = xbel.get_items_mut(xbel_path);
    let bookmark = XbelItem::new_bookmark(
        (highest_id + 1).to_string().as_str(), add_args.url.as_str(), add_args.title.as_str()
    );
    
    if let Some(items) = items_ {
        items.push(bookmark);
    }
    
    println!("xbel: {:?}", xbel);
    {
        let mut f = std::fs::File::options()
            .write(true)
            .open(bookmark_file_path)?;

        let buffer = xbel.write_to_string();
        f.write_all(buffer.as_bytes())?;
    }

    if !add_args.disable_push {
        panic!("TODO git push");
    }

    Ok(())
}