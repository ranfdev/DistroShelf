use crate::fakers::Command;
use std::rc::Rc;

/// A factory that returns a base `Command` value for running `distrobox`.
/// Invariant: the factory must return the base program only (e.g. `Command::new("distrobox")`).
/// Any Flatpak wrapping should be applied by `CommandRunner` implementations.
pub type CmdFactory = Rc<dyn Fn() -> Command + 'static>;

pub fn default_cmd_factory() -> CmdFactory {
    Rc::new(|| Command::new("distrobox"))
}
