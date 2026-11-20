//! Known apps that matter for gaming performance, matched by their installed name
//! (the `DisplayName` in Windows' list of installed programs).

use super::AppKind;

enum Match {
    /// The whole name, ignoring case.
    Exact(&'static str),
    /// The name contains this text, ignoring case.
    Contains(&'static str),
}

const KNOWN: &[(&str, Match, AppKind)] = &[
    // Other optimizers: their tweaks can overlap or fight with ours.
    (
        "razer-cortex",
        Match::Contains("razer cortex"),
        AppKind::Optimizer,
    ),
    (
        "advanced-systemcare",
        Match::Contains("advanced systemcare"),
        AppKind::Optimizer,
    ),
    (
        "wise-game-booster",
        Match::Contains("wise game booster"),
        AppKind::Optimizer,
    ),
    (
        "smart-game-booster",
        Match::Contains("smart game booster"),
        AppKind::Optimizer,
    ),
    (
        "game-fire",
        Match::Contains("game fire"),
        AppKind::Optimizer,
    ),
    (
        "process-lasso",
        Match::Contains("process lasso"),
        AppKind::Optimizer,
    ),
    ("ccleaner", Match::Contains("ccleaner"), AppKind::Optimizer),
    (
        "ashampoo-winoptimizer",
        Match::Contains("winoptimizer"),
        AppKind::Optimizer,
    ),
    (
        "glary-utilities",
        Match::Contains("glary utilities"),
        AppKind::Optimizer,
    ),
    (
        "avg-tuneup",
        Match::Contains("avg tuneup"),
        AppKind::Optimizer,
    ),
    (
        "avast-cleanup",
        Match::Contains("avast cleanup"),
        AppKind::Optimizer,
    ),
    // Apps that draw an overlay over games.
    ("discord", Match::Exact("discord"), AppKind::Overlay),
    ("steam", Match::Exact("steam"), AppKind::Overlay),
    (
        "nvidia-app",
        Match::Contains("nvidia app"),
        AppKind::Overlay,
    ),
    (
        "geforce-experience",
        Match::Contains("geforce experience"),
        AppKind::Overlay,
    ),
    (
        "amd-software",
        Match::Contains("amd software"),
        AppKind::Overlay,
    ),
    (
        "msi-afterburner",
        Match::Contains("msi afterburner"),
        AppKind::Overlay,
    ),
    (
        "rivatuner",
        Match::Contains("rivatuner statistics server"),
        AppKind::Overlay,
    ),
    ("overwolf", Match::Contains("overwolf"), AppKind::Overlay),
];

/// `(id, kind)` if `display_name` is a known app.
pub fn identify(display_name: &str) -> Option<(&'static str, AppKind)> {
    let name = display_name.trim().to_lowercase();
    KNOWN.iter().find_map(|(id, m, kind)| {
        let hit = match m {
            Match::Exact(s) => name == *s,
            Match::Contains(s) => name.contains(s),
        };
        hit.then_some((*id, *kind))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_known_apps_only() {
        assert_eq!(identify("Discord"), Some(("discord", AppKind::Overlay)));
        assert_eq!(identify("Steam"), Some(("steam", AppKind::Overlay)));
        assert_eq!(
            identify("NVIDIA App 11.0.1.163"),
            Some(("nvidia-app", AppKind::Overlay))
        );
        assert_eq!(
            identify("Razer Cortex"),
            Some(("razer-cortex", AppKind::Optimizer))
        );
        assert_eq!(identify("CCleaner"), Some(("ccleaner", AppKind::Optimizer)));
        // Not the app itself.
        assert_eq!(identify("Steamworks Common Redistributables"), None);
        assert_eq!(identify("Discord PTB Helper"), None);
        assert_eq!(identify("Microsoft Edge"), None);
    }
}
