use std::env::args;

use crate::git::Upstream;

mod git;

fn main() {
    let path = args().nth(1).unwrap_or_else(|| ".".into());
    println!("Scanning workspace at: {}", path);
    let workspace = git::Workspace::scan(path);

    match workspace {
        Ok(workspace) => {
            println!("Workspace root: {}", workspace.root().display());
            for repo in workspace.repos() {
                println!("Repo: {}", repo.name());
                for worktree in repo.worktrees() {
                    if worktree.is_main() {
                        println!("  Main Checkout: {}", worktree.path().display());
                    } else {
                        println!("  Worktree: {}", worktree.path().display());
                    }
                    println!("   HEAD: {}", worktree.head());
                    println!("   Branch: {}", worktree.branch().unwrap_or("No branch"));
                    match worktree.status() {
                        Ok(status) => {
                            match status.branch {
                                Some(branch) => println!("   Branch: {}", branch),
                                None => println!("   No Branch"),
                            }
                            match &status.upstream {
                                Some(Upstream {
                                    name,
                                    counts: Some((ahead, behind)),
                                }) => println!("   Upstream: {} ↑{} ↓{}", name, ahead, behind),
                                Some(Upstream { name, counts: None }) => {
                                    println!("  Upstream: {} (Gone)", name)
                                }
                                None => println!("   No Upstream"),
                            }
                            for file in status.files {
                                let x = file.staged.map(|k| k.to_code()).unwrap_or(' ');
                                let y = file.unstaged.map(|k| k.to_code()).unwrap_or(' ');
                                let old_name = file.old_path.as_deref().unwrap_or(" ");

                                println!(" {} {} {} {}", x, y, file.path, old_name);
                            }
                        }
                        Err(err) => eprintln!("Error getting worktree status: {}", err),
                    }
                }
            }
        }
        Err(err) => eprintln!("Error scanning workspace: {}", err),
    }
}
