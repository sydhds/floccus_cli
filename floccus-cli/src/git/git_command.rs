// std
use std::cell::RefCell;
use std::path::{Path, PathBuf};
// third-party
use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{Cred, FetchOptions, Progress, RemoteCallbacks, Repository};
use tracing::{debug, info, warn};
use url::Url;

struct State {
    progress: Option<Progress<'static>>,
    total: usize,
    current: usize,
    path: Option<PathBuf>,
    // newline: bool,
}

pub fn git_clone(
    url: &Url,
    to_path: &Path,
    ssh_key: Option<&Path>,
) -> Result<Repository, git2::Error> {
    let state = RefCell::new(State {
        progress: None,
        total: 0,
        current: 0,
        path: None,
        // newline: false,
    });
    let mut cb = RemoteCallbacks::new();

    if url.scheme() == "ssh" {
        if let Some(ssh_key) = ssh_key {
            cb.credentials(|_url, username_from_url, _allowed_types| {
                Cred::ssh_key(
                    username_from_url.unwrap(), // Safe to unwrap - as url is of Url type
                    None,
                    ssh_key,
                    None,
                )
            });
        }
    }

    cb.transfer_progress(|stats| {
        let mut state = state.borrow_mut();
        state.progress = Some(stats.to_owned());
        // TODO
        // print(&mut *state);
        true
    });

    let mut co = CheckoutBuilder::new();
    co.progress(|path, cur, total| {
        let mut state = state.borrow_mut();
        state.path = path.map(|p| p.to_path_buf());
        state.current = cur;
        state.total = total;
        // print(&mut *state);
    });

    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(cb);
    let repo = RepoBuilder::new()
        .fetch_options(fetch_opts)
        .with_checkout(co)
        .clone(url.to_string().as_str(), to_path)?;

    Ok(repo)
}

pub fn git_fetch<'a>(
    repo: &'a git2::Repository,
    refs: &[&str],
    remote: &'a mut git2::Remote,
) -> Result<git2::AnnotatedCommit<'a>, git2::Error> {
    /*
    let mut cb = git2::RemoteCallbacks::new();

    // Print out our transfer progress.
    cb.transfer_progress(|stats| {
        if stats.received_objects() == stats.total_objects() {
            print!(
                "Resolving deltas {}/{}\r",
                stats.indexed_deltas(),
                stats.total_deltas()
            );
        } else if stats.total_objects() > 0 {
            print!(
                "Received {}/{} objects ({}) in {} bytes\r",
                stats.received_objects(),
                stats.total_objects(),
                stats.indexed_objects(),
                stats.received_bytes()
            );
        }
        io::stdout().flush().unwrap();
        true
    });
    */

    let mut fetch_opts = git2::FetchOptions::new();
    // fetch_opts.remote_callbacks(cb);

    // Always fetch all tags.
    // Perform a download and also update tips
    fetch_opts.download_tags(git2::AutotagOption::All);
    debug!("Fetching {} for repo", remote.name().unwrap());
    remote.fetch(refs, Some(&mut fetch_opts), None)?;

    // If there are local objects (we got a thin pack), then tell the user
    // how many objects we saved from having to cross the network.
    let stats = remote.stats();
    if stats.local_objects() > 0 {
        info!(
            "Received {}/{} objects in {} bytes (used {} local \
             objects)",
            stats.indexed_objects(),
            stats.total_objects(),
            stats.received_bytes(),
            stats.local_objects()
        );
    } else {
        info!(
            "Received {}/{} objects in {} bytes",
            stats.indexed_objects(),
            stats.total_objects(),
            stats.received_bytes()
        );
    }

    let fetch_head = repo.find_reference("FETCH_HEAD")?;
    repo.reference_to_annotated_commit(&fetch_head)
}

fn fast_forward(
    repo: &Repository,
    lb: &mut git2::Reference,
    rc: &git2::AnnotatedCommit,
) -> Result<(), git2::Error> {
    let name = match lb.name() {
        Some(s) => s.to_string(),
        None => String::from_utf8_lossy(lb.name_bytes()).to_string(),
    };
    let msg = format!("Fast-Forward: Setting {} to id: {}", name, rc.id());
    info!("{}", msg);
    lb.set_target(rc.id(), &msg)?;
    repo.set_head(&name)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            // For some reason the force is required to make the working directory actually get updated
            // I suspect we should be adding some logic to handle dirty working directory states
            // but this is just an example so maybe not.
            .force(),
    ))?;
    Ok(())
}

fn normal_merge(
    repo: &Repository,
    local: &git2::AnnotatedCommit,
    remote: &git2::AnnotatedCommit,
) -> Result<(), git2::Error> {
    let local_tree = repo.find_commit(local.id())?.tree()?;
    let remote_tree = repo.find_commit(remote.id())?.tree()?;
    let ancestor = repo
        .find_commit(repo.merge_base(local.id(), remote.id())?)?
        .tree()?;
    let mut idx = repo.merge_trees(&ancestor, &local_tree, &remote_tree, None)?;

    if idx.has_conflicts() {
        warn!("Merge conflicts detected...");
        repo.checkout_index(Some(&mut idx), None)?;
        return Ok(());
    }
    let result_tree = repo.find_tree(idx.write_tree_to(repo)?)?;
    // now create the merge commit
    let msg = format!("Merge: {} into {}", remote.id(), local.id());
    let sig = repo.signature()?;
    let local_commit = repo.find_commit(local.id())?;
    let remote_commit = repo.find_commit(remote.id())?;
    // Do our merge commit and set current branch head to that commit.
    let _merge_commit = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &msg,
        &result_tree,
        &[&local_commit, &remote_commit],
    )?;
    // Set working tree to match head.
    repo.checkout_head(None)?;
    Ok(())
}

pub fn git_merge<'a>(
    repo: &'a Repository,
    remote_branch: &str,
    fetch_commit: git2::AnnotatedCommit<'a>,
) -> Result<(), git2::Error> {
    // 1. do a merge analysis
    let analysis = repo.merge_analysis(&[&fetch_commit])?;

    // 2. Do the appropriate merge
    if analysis.0.is_fast_forward() {
        debug!("Doing a fast forward");
        // do a fast forward
        let refname = format!("refs/heads/{}", remote_branch);
        match repo.find_reference(&refname) {
            Ok(mut r) => {
                fast_forward(repo, &mut r, &fetch_commit)?;
            }
            Err(_) => {
                // The branch doesn't exist so just set the reference to the
                // commit directly. Usually this is because you are pulling
                // into an empty repository.
                repo.reference(
                    &refname,
                    fetch_commit.id(),
                    true,
                    &format!("Setting {} to {}", remote_branch, fetch_commit.id()),
                )?;
                repo.set_head(&refname)?;
                repo.checkout_head(Some(
                    git2::build::CheckoutBuilder::default()
                        .allow_conflicts(true)
                        .conflict_style_merge(true)
                        .force(),
                ))?;
            }
        };
    } else if analysis.0.is_normal() {
        // do a normal merge
        let head_commit = repo.reference_to_annotated_commit(&repo.head()?)?;
        normal_merge(repo, &head_commit, &fetch_commit)?;
    } else {
        info!("Nothing to do...");
    }
    Ok(())
}

pub fn git_push(repo: &Repository, file_to_add: &Path) -> Result<(), git2::Error> {
    // Configured author signature
    let author = repo.signature()?;

    // git add
    let status = repo.status_file(file_to_add)?;
    info!("status: {:?}", status);
    let mut index = repo.index()?;

    index.add_path(file_to_add)?;
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

    Ok(())
}
