//! Where the Help menu's links go: the ArtCraft website, the Craft wiki and this app's repository

/// Help menu ids and the pages they open
pub(crate) const HELP_LINKS: &[(&str, &str)] = &[
    ("ui.help.tutorials", "https://craft.wiki/apps/materialcraft/getting-started/"),
    ("ui.help.releaseNotes", "https://github.com/storytold/materialcraft/releases"),
    ("ui.help.requirements", "https://craft.wiki/apps/materialcraft/install-update/"),
    ("ui.help.docs", "https://craft.wiki/apps/materialcraft/overview/"),
    ("ui.help.automation", "https://github.com/storytold/materialcraft/blob/main/docs/control-protocol.md"),
    ("ui.help.troubleshooting", "https://craft.wiki/apps/materialcraft/troubleshooting/"),
    ("ui.help.discord", "https://discord.gg/artcraft"),
    ("ui.help.bug", "https://github.com/storytold/materialcraft/issues/new"),
    ("ui.help.feedback", "https://getartcraft.com/support"),
    ("ui.help.apps", "https://getartcraft.com/apps"),
    ("ui.help.website", "https://getartcraft.com/apps/materialcraft"),
    ("ui.help.news", "https://github.com/storytold/materialcraft/releases"),
];

/// The page a Help menu id opens
pub(crate) fn url(id: &str) -> Option<&'static str> {
    HELP_LINKS.iter().find(|(link, _)| *link == id).map(|(_, url)| *url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_link_is_an_https_page_of_artcraft_or_this_repository() {
        for (id, url) in HELP_LINKS {
            let host = url.strip_prefix("https://").and_then(|rest| rest.split('/').next()).unwrap_or_default();
            assert!(["getartcraft.com", "craft.wiki", "github.com", "discord.gg"].contains(&host), "{id}: {url}");
            assert!(!url.contains("github.com") || url.contains("/storytold/materialcraft"), "{id}: {url}");
        }
        assert_eq!(url("ui.help.docs"), Some("https://craft.wiki/apps/materialcraft/overview/"));
        assert_eq!(url("ui.about"), None);
    }
}
