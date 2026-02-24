use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Default)]
pub struct Config {
    pub auth: Option<String>,
    pub upstream: Option<String>,
    pub extra: HashMap<String, String>,
}

pub fn load(repo_path: &Path) -> Result<Config, String> {
    let output = Command::new("jj")
        .arg("config")
        .arg("list")
        .arg("--repo")
        .arg(repo_path)
        .output()
        .map_err(|e| format!("Failed to execute jj: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "jj config list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut config = Config::default();

    for line in stdout.lines() {
        // Expected format: key = value
        // Note: jj config list output might depend on version, but typically key=value or key = value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let mut value = value.trim();

            if key.starts_with("jellycat.") {
                // Strip quotes if present
                if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    value = &value[1..value.len() - 1];
                }

                if key == "jellycat.auth" {
                    config.auth = Some(value.to_string());
                } else {
                    config.extra.insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    Ok(config)
}
