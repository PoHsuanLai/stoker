//! Which of a remote entry's reaches to use, given what the person granted and what this build
//! can speak.

use crate::entry::{ProviderId, Reach, Wire};
use crate::{Locality, ModelEntry};

impl Reach {
    /// Whether this reach goes through a gateway rather than the model's own company (so a
    /// footer can say "via OpenRouter"). The gateway's id is `openrouter`.
    pub fn is_via_gateway(&self) -> bool {
        self.provider.0 == "openrouter"
    }
}

/// The reach to use for an entry: among its reaches whose provider is in `granted` and whose wire
/// is in `wires`, a direct one before a gateway one, and otherwise the first listed. `None` for an
/// on-device entry and when nothing is granted or speakable. The chosen reach comes back whole, so
/// the caller can name the provider and show the price.
pub fn reachable<'a>(
    entry: &'a ModelEntry,
    granted: &[ProviderId],
    wires: &[Wire],
) -> Option<&'a Reach> {
    let Locality::Remote { reach } = &entry.locality else {
        return None;
    };
    let usable = |r: &&Reach| granted.contains(&r.provider) && wires.contains(&r.wire);
    reach
        .iter()
        .filter(usable)
        .min_by_key(|r| r.is_via_gateway())
}
