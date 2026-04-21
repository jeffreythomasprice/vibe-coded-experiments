use crate::protocol::Request;

pub struct SlashCommand {
    pub name: &'static str,
    /// Alternate names the user can type that resolve to this command (exact
    /// and prefix matching both honor aliases). The autocomplete popup still
    /// shows only the canonical `name`.
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub action: Action,
}

#[derive(Clone, Copy)]
pub enum Action {
    /// Exit the TUI immediately.
    Quit,
    /// Build a server `Request` from the command's argument string.
    Send(fn(&str) -> Request),
    /// Swap in a different screen. The event loop looks up the intent in
    /// `screens::open` and performs the transition + initial data fetch.
    OpenScreen(ScreenIntent),
    /// Reset the current chat screen's local state — clear the transcript and
    /// drop `conversation_id` so the next user message mints a new
    /// conversation. No server RPC; the prior conversation is preserved in
    /// SQLite and reachable via `/resume`.
    ClearChat,
}

/// Screens that a slash command can open. Each variant is resolved to a
/// concrete [`Screen`](super::screens::Screen) + initial [`Request`] by the
/// outer event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenIntent {
    ConversationList,
    DocumentList,
}

pub fn registry() -> &'static [SlashCommand] {
    static R: &[SlashCommand] = &[
        SlashCommand {
            name: "ping",
            aliases: &[],
            description: "ping the server",
            action: Action::Send(|_| Request::Ping),
        },
        SlashCommand {
            name: "resume",
            aliases: &["conversations"],
            description: "list and resume past conversations",
            action: Action::OpenScreen(ScreenIntent::ConversationList),
        },
        SlashCommand {
            name: "documents",
            aliases: &[],
            description: "list, ingest, and search documents",
            action: Action::OpenScreen(ScreenIntent::DocumentList),
        },
        SlashCommand {
            name: "clear",
            aliases: &[],
            description: "start a new conversation",
            action: Action::ClearChat,
        },
        SlashCommand {
            name: "quit",
            aliases: &[],
            description: "exit the TUI",
            action: Action::Quit,
        },
    ];
    R
}

fn name_or_alias_starts_with(cmd: &SlashCommand, prefix: &str) -> bool {
    cmd.name.starts_with(prefix) || cmd.aliases.iter().any(|a| a.starts_with(prefix))
}

fn name_or_alias_eq(cmd: &SlashCommand, name: &str) -> bool {
    cmd.name == name || cmd.aliases.contains(&name)
}

pub fn filter(prefix: &str) -> Vec<&'static SlashCommand> {
    registry()
        .iter()
        .filter(|c| name_or_alias_starts_with(c, prefix))
        .collect()
}

pub fn find(name: &str) -> Option<&'static SlashCommand> {
    registry().iter().find(|c| name_or_alias_eq(c, name))
}
