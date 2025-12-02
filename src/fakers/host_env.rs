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
}
