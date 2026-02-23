use std::env;
use std::path::PathBuf;

pub fn find_root() -> Option<PathBuf> {
    let mut current_dir = env::current_dir().ok()?;
    loop {
        if current_dir.join(".jj").is_dir() {
            return Some(current_dir);
        }
        if !current_dir.pop() {
            return None;
        }
    }
}
