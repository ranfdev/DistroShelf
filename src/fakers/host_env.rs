use crate::fakers::{Command, CommandRunner};
use std::io;

/// Resolve an environment variable on the host via a `CommandRunner`.
///
/// Returns `Ok(Some(value))` if present, `Ok(None)` if unset/empty, or `Err(e)` on I/O/command errors.
pub async fn resolve_host_env_via_runner(
    runner: &CommandRunner,
    key: &str,
) -> io::Result<Option<String>> {
    // Try `printenv KEY` first — simpler when available
    let cmd = Command::new_with_args("printenv", [key]);
    match runner.output_string(cmd.clone()).await {
        Ok(out) => {
            let trimmed = out.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed));
            }
            // fallthrough to sh -c printf if empty
        }
        Err(_) => {
            // fallback below
        }
    }

    // Fallback: sh -c 'printf "%s" "$KEY"'
    let mut cmd = Command::new("sh");
    cmd.arg("-c");
    // Only interpolate the variable name to avoid injection
    cmd.arg(format!("printf '%s' \"${}\"", key));

    let out = runner.output_string(cmd).await?;
    let trimmed = out.trim().to_string();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

/// Resolve all host environment variables via a `CommandRunner` as `KEY=VALUE` entries.
///
/// This is useful when spawning processes that need the host `PATH` and other vars, e.g. VTE.
pub async fn resolve_host_env_list_via_runner(runner: &CommandRunner) -> io::Result<Vec<String>> {
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
                    if s.contains('=') { Some(s) } else { None }
                })
                .collect::<Vec<_>>();
            if !vars.is_empty() {
                return Ok(vars);
            }
        }
    }

    // Fallback: newline-separated output.
    let output = runner.output(Command::new("env")).await?;
    if !output.status.success() {
        return Err(io::Error::other("failed to resolve host environment via `env`"));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains('='))
        .map(ToString::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakers::NullCommandRunnerBuilder;
    use smol::block_on;

    #[test]
    fn test_resolve_host_env_with_null_runner() {
        let runner = NullCommandRunnerBuilder::new()
            .cmd(&["printenv", "HOME"], "/fake/home")
            .build();

        let res = block_on(resolve_host_env_via_runner(&runner, "HOME")).unwrap();
        assert_eq!(res, Some("/fake/home".to_string()));
    }

    #[test]
    fn test_resolve_host_env_unset_with_null_runner() {
        let runner = NullCommandRunnerBuilder::new().build();
        let res = block_on(resolve_host_env_via_runner(&runner, "HOME")).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn test_resolve_host_env_list_with_null_runner_nul_separated() {
        let mut builder = NullCommandRunnerBuilder::new();
        let mut cmd = Command::new("env");
        cmd.arg("-0");
        builder.cmd_full(cmd, || Ok("HOME=/fake/home\0PATH=/nix/store/bin\0".to_string()));
        let runner = builder.build();

        let res = block_on(resolve_host_env_list_via_runner(&runner)).unwrap();
        assert_eq!(res, vec!["HOME=/fake/home", "PATH=/nix/store/bin"]);
    }
}
