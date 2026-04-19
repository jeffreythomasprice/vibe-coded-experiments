use crate::protocol::Request;

pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub action: Action,
}

#[derive(Clone, Copy)]
pub enum Action {
    /// Exit the TUI immediately.
    Quit,
    /// Build a server `Request` from the command's argument string.
    Send(fn(&str) -> Request),
}

pub fn registry() -> &'static [SlashCommand] {
    static R: &[SlashCommand] = &[
        SlashCommand {
            name: "ping",
            description: "ping the server",
            action: Action::Send(|_| Request::Ping),
        },
        SlashCommand {
            name: "quit",
            description: "exit the TUI",
            action: Action::Quit,
        },
    ];
    R
}

pub fn filter(prefix: &str) -> Vec<&'static SlashCommand> {
    registry()
        .iter()
        .filter(|c| c.name.starts_with(prefix))
        .collect()
}

pub fn find(name: &str) -> Option<&'static SlashCommand> {
    registry().iter().find(|c| c.name == name)
}
