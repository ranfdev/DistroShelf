use crate::fakers::Command;

pub fn map_flatpak_spawn_host(mut command: Command) -> Command {
    let mut args = vec!["--host".into(), command.program];
    args.extend(command.args);

    command.args = args;
    command.program = "flatpak-spawn".into();
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_flatpak_spawn_host_simple() {
        let cmd = Command::new("ls");
        let mapped = map_flatpak_spawn_host(cmd);
        
        assert_eq!(mapped.program.to_string_lossy(), "flatpak-spawn");
        assert_eq!(mapped.args.len(), 2);
        assert_eq!(mapped.args[0].to_string_lossy(), "--host");
        assert_eq!(mapped.args[1].to_string_lossy(), "ls");
    }

    #[test]
    fn test_map_flatpak_spawn_host_with_args() {
        let mut cmd = Command::new("distrobox");
        cmd.args(["ls", "--no-color"]);
        
        let mapped = map_flatpak_spawn_host(cmd);
        
        assert_eq!(mapped.program.to_string_lossy(), "flatpak-spawn");
        assert_eq!(mapped.args.len(), 4);
        assert_eq!(mapped.args[0].to_string_lossy(), "--host");
        assert_eq!(mapped.args[1].to_string_lossy(), "distrobox");
        assert_eq!(mapped.args[2].to_string_lossy(), "ls");
        assert_eq!(mapped.args[3].to_string_lossy(), "--no-color");
    }

    #[test]
    fn test_map_flatpak_spawn_host_display() {
        let mut cmd = Command::new("podman");
        cmd.args(["ps", "-a"]);
        
        let mapped = map_flatpak_spawn_host(cmd);
        let display = format!("{}", mapped);
        
        assert_eq!(display, "flatpak-spawn --host podman ps -a");
    }

    #[test]
    fn test_map_flatpak_spawn_host_preserves_fd_modes() {
        use crate::fakers::FdMode;
        
        let mut cmd = Command::new("echo");
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;
        
        let mapped = map_flatpak_spawn_host(cmd);
        
        // FdMode should be preserved
        matches!(mapped.stdout, FdMode::Pipe);
        matches!(mapped.stderr, FdMode::Pipe);
    }
}

