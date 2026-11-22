//! Watches every worktree on disk and asks the app to refresh when something that can
//! change `git status` happens: an edited file in a worktree, or git's own index, HEAD
//! or refs (a stage, commit or checkout made from a terminal).

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::{Subscription, stream};
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, DebouncedEventBatch, new_debouncer};

use super::Message;

/// How long the disk has to be quiet before a refresh: a save, a branch switch or a
/// `git add` touch many files at once, and they should cost one refresh, not dozens.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// After asking for a refresh, the watcher waits this long before asking again. Changes
/// keep queuing meanwhile and arrive afterwards, so none is lost; this only caps a
/// constant writer (a build, a dev server) at one refresh per second.
const MIN_INTERVAL: Duration = Duration::from_secs(1);

/// Generated folders that change constantly during builds but are gitignored in
/// practice, so they never change `git status`. Skipping them keeps a `cargo build` or
/// `npm install` from refreshing the app all the time. (If one of these is tracked in
/// some repo, its changes still show up on the next Refresh or any other change.)
const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".next",
    ".turbo",
    ".cache",
    "dist",
    "build",
    "out",
    "coverage",
    ".gradle",
    ".idea",
    "__pycache__",
    ".venv",
];

/// Files editors and the OS write on their own: swap and backup files, Finder metadata.
const IGNORED_FILE_SUFFIXES: &[&str] = &[".swp", ".swx", ".swo", "~", ".DS_Store"];

/// Inside `.git`, only these change what the app shows; `objects/`, `logs/` and the
/// `*.lock` files churn on every git command.
const RELEVANT_GIT_FILES: &[&str] = &["index", "HEAD", "packed-refs"];

/// A subscription watching `paths` (the worktrees). iced restarts it whenever the list
/// changes, so adding or removing a worktree re-targets the watcher automatically.
pub(super) fn subscription(paths: Vec<PathBuf>) -> Subscription<Message> {
    Subscription::run_with(paths, watch)
}

// `use<>`: the stream must not borrow `paths` (iced needs a `fn(&D) -> S` with one `S`),
// so it clones what it needs up front.
fn watch(paths: &Vec<PathBuf>) -> impl Stream<Item = Message> + use<> {
    let paths = paths.clone();

    stream::channel(1, async move |mut output: mpsc::Sender<Message>| {
        // notify calls the handler on its own thread; this channel carries "something
        // changed" from there into this async task.
        let (changed_tx, mut changed_rx) = mpsc::unbounded::<()>();
        let roots = paths.clone();

        // Runs on the debouncer's own thread, once per quiet period.
        let debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            let relevant = match result {
                Ok(DebouncedEventBatch::Events(events)) => events
                    .iter()
                    .any(|event| is_relevant(relative_to_root(&event.path, &roots))),
                // A `Rescan`, or an error such as an overflowed OS buffer: events were
                // lost, so refresh to be safe.
                _ => true,
            };
            if relevant && changed_tx.unbounded_send(()).is_ok() {
                // Blocking here is the rate cap: new events queue up in the debouncer
                // and are handled when this returns.
                thread::sleep(MIN_INTERVAL);
            }
        });

        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(err) => {
                let _ = output.send(Message::WatchFailed(err.to_string())).await;
                return;
            }
        };

        // A worktree whose folder is gone (deleted by hand, waiting for `git worktree
        // prune`) has nothing to watch; anything else that fails is reported once.
        let failures: Vec<String> = paths
            .iter()
            .filter(|path| path.exists())
            .filter_map(|path| {
                debouncer
                    .watcher()
                    .watch(path, RecursiveMode::Recursive)
                    .err()
                    .map(|err| format!("{}: {err}", path.display()))
            })
            .collect();
        if !failures.is_empty() {
            let _ = output.send(Message::WatchFailed(failures.join("; "))).await;
        }

        // Runs until iced drops the subscription; `debouncer` stays alive (and keeps
        // watching) for exactly that long.
        while changed_rx.next().await.is_some() {
            // If several changes piled up while the app was busy refreshing, one more
            // refresh covers all of them.
            while changed_rx.try_recv().is_ok() {}

            if output.send(Message::FilesChanged).await.is_err() {
                break;
            }
        }
    })
}

/// `path` relative to the watched worktree it's in, so a worktree that itself lives
/// under a folder named like an ignored one (`~/build/repo`) isn't ignored wholesale.
fn relative_to_root<'a>(path: &'a Path, roots: &[PathBuf]) -> &'a Path {
    roots
        .iter()
        .find_map(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
}

/// Whether a change at `path` (relative to its worktree) can affect what the app shows.
fn is_relevant(path: &Path) -> bool {
    let is_noise_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            IGNORED_FILE_SUFFIXES
                .iter()
                .any(|suffix| name.ends_with(suffix))
        });
    if is_noise_file {
        return false;
    }

    let names: Vec<&OsStr> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();

    match names.iter().position(|name| *name == ".git") {
        Some(git_dir) => {
            let inside = &names[git_dir + 1..];
            let file_name = inside.last().and_then(|name| name.to_str());
            file_name.is_some_and(|name| RELEVANT_GIT_FILES.contains(&name))
                || inside.iter().any(|name| *name == "refs")
        }
        None => !names
            .iter()
            .any(|name| IGNORED_DIRS.iter().any(|ignored| *name == *ignored)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_tree_files_are_relevant() {
        assert!(is_relevant(Path::new("/repo/src/main.rs")));
        assert!(is_relevant(Path::new("/repo/README.md")));
    }

    #[test]
    fn test_build_output_is_ignored() {
        assert!(!is_relevant(Path::new("/repo/target/debug/app.exe")));
        assert!(!is_relevant(Path::new(
            "/repo/web/node_modules/react/index.js"
        )));
        assert!(!is_relevant(Path::new("/repo/web/dist/bundle.js")));
    }

    #[test]
    fn test_editor_and_os_noise_is_ignored() {
        assert!(!is_relevant(Path::new("/repo/src/.main.rs.swp")));
        assert!(!is_relevant(Path::new("/repo/src/main.rs~")));
        assert!(!is_relevant(Path::new("/repo/.DS_Store")));
    }

    #[test]
    fn test_ignored_names_only_count_inside_the_worktree() {
        let roots = [PathBuf::from("/home/me/build/repo")];
        let changed = Path::new("/home/me/build/repo/src/main.rs");
        assert!(is_relevant(relative_to_root(changed, &roots)));

        let built = Path::new("/home/me/build/repo/build/out.js");
        assert!(!is_relevant(relative_to_root(built, &roots)));
    }

    #[test]
    fn test_git_dir_only_reacts_to_index_head_and_refs() {
        assert!(is_relevant(Path::new("/repo/.git/index")));
        assert!(is_relevant(Path::new("/repo/.git/HEAD")));
        assert!(is_relevant(Path::new("/repo/.git/refs/heads/main")));
        assert!(is_relevant(Path::new("/repo/.git/packed-refs")));
        // A linked worktree's index and HEAD live under the main repo's .git/worktrees/.
        assert!(is_relevant(Path::new("/repo/.git/worktrees/feature/index")));
        assert!(is_relevant(Path::new("/repo/.git/worktrees/feature/HEAD")));

        assert!(!is_relevant(Path::new("/repo/.git/objects/ab/cdef0123")));
        assert!(!is_relevant(Path::new("/repo/.git/index.lock")));
        assert!(!is_relevant(Path::new("/repo/.git/logs/HEAD.lock")));
        assert!(!is_relevant(Path::new("/repo/.git/FETCH_HEAD")));
    }
}
