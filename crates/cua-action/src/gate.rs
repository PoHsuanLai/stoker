//! The computer-use gate's vocabulary. Exhaustive on purpose: a new variant must force every gate match to be revisited.

use serde::{Deserialize, Serialize};

/// How a run reaches its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// On the person's own seat.
    InPlace,
    /// In an agent workspace out of sight (the compositor fork).
    AgentWorkspace,
    /// In a nested session.
    NestedSession,
}

/// How far a window can be trusted to describe itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowTrust {
    /// A quire app: typed actions and a11y.
    Quire,
    /// A Flatpak: the sandbox names it.
    Flatpak,
    /// A native app.
    Native,
    /// The shell itself.
    Shell,
}

/// What kind of window a step acts in, which sets the default effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowClass {
    /// Anything else.
    Ordinary,
    /// A mail compose window: typing is outbound.
    MailCompose,
    /// A terminal.
    Terminal,
    /// A payment form.
    Payments,
    /// A system administration surface.
    Admin,
    /// A password manager.
    PasswordManager,
    /// Banking.
    Banking,
}
