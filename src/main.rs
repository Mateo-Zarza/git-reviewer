use crate::git::{Git, GitError};

mod git;

fn main() {
    let git = Git::new(".");

    match git.run(&["status"]) {
        Ok(output) => println!("Git status:\n{}", output),
        Err(GitError::Io(e)) => eprintln!("I/O error: {:?}", e),
        Err(GitError::CommandFailed(stderr)) => eprintln!("Error running git command: {}", stderr),
    }
}
