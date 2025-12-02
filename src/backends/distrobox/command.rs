use crate::fakers::Command;

/// A factory that returns a base `Command` value for running `distrobox`.
/// Invariant: the factory must return the base program only (e.g. `Command::new("distrobox")`).
/// Any Flatpak wrapping should be applied by `CommandRunner` implementations.
pub type CmdFactory = Box<dyn Fn() -> Command + 'static>;

pub fn default_cmd_factory() -> CmdFactory {
    Box::new(|| Command::new("distrobox"))
}
