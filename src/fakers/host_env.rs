use crate::fakers::{Command, CommandRunner};
use std::collections::HashMap;
use std::io;

/// Resolve all host environment variables via a `CommandRunner`.
pub async fn resolve_host_env(runner: &CommandRunner) -> io::Result<HashMap<String, String>> {
    // Prefer NUL-separated output to preserve values containing newlines.
    let mut cmd = Command::new("env");
    cmd.arg("-0");

    if let Ok(output) = runner.output(cmd).await {
        if output.status.success() {
            let vars = output
                .stdout
                .split(|b| *b == 0)
                .filter_map(|entry| {
                    if entry.is_empty() {
                        return None;
                    }
                    let s = String::from_utf8_lossy(entry).to_string();
                    s.split_once('=')
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                })
                .collect::<HashMap<_, _>>();
            if !vars.is_empty() {
                return Ok(vars);
            }
        }
    }

    // Fallback: newline-separated output.
    let output = runner.output(Command::new("env")).await?;
    if !output.status.success() {
        return Err(io::Error::other(
            "failed to resolve host environment via `env`",
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            line.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect())
}

pub fn host_env_to_list(env: &HashMap<String, String>) -> Vec<String> {
    let mut list = env
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    list.sort_unstable();
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakers::NullCommandRunnerBuilder;
    use smol::block_on;

    #[test]
    fn test_resolve_host_env_empty_with_null_runner() {
        let runner = NullCommandRunnerBuilder::new().build();
        let res = block_on(resolve_host_env(&runner)).unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn test_resolve_host_env_with_null_runner_nul_separated() {
        let mut builder = NullCommandRunnerBuilder::new();
        let mut cmd = Command::new("env");
        cmd.arg("-0");
        builder.cmd_full(cmd, || {
            Ok("HOME=/fake/home\0PATH=/nix/store/bin\0".to_string())
        });
        let runner = builder.build();

        let res = block_on(resolve_host_env(&runner)).unwrap();
        assert_eq!(res.get("HOME"), Some(&"/fake/home".to_string()));
        assert_eq!(res.get("PATH"), Some(&"/nix/store/bin".to_string()));
    }

    #[test]
    fn test_host_env_to_list() {
        let env = HashMap::from([
            ("PATH".to_string(), "/nix/store/bin".to_string()),
            ("HOME".to_string(), "/fake/home".to_string()),
        ]);
        let list = host_env_to_list(&env);
        assert_eq!(list, vec!["HOME=/fake/home", "PATH=/nix/store/bin"]);
    }
}
