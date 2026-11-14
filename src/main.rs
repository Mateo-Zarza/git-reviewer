use std::env::args;

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
                    println!(
                        "  Worktree: {:?}\n  HEAD: {}\n  Branch: {:?}\n  Main: {}",
                        worktree.path(),
                        worktree.head(),
                        worktree.branch(),
                        worktree.is_main()
                    );
                }
            }
        }
        Err(err) => eprintln!("Error scanning workspace: {}", err),
    }
}
