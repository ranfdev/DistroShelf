use crate::fakers::Command;

pub fn map_flatpak_spawn_host(mut command: Command) -> Command {
    let mut args = vec!["--host".into(), command.program];
    args.extend(command.args);

    command.args = args;
    command.program = "flatpak-spawn".into();
    command
}
