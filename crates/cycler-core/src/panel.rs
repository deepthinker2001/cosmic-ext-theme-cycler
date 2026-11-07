//! Whether the applet is still in a COSMIC panel, so it can put the user's
//! original theme back when it is removed from the panel.
//!
//! cosmic-panel keeps each panel's applets in its config
//! (`com.system76.CosmicPanel.<Panel>`, keys `plugins_center` and
//! `plugins_wings`); the panels are listed in `com.system76.CosmicPanel`'s
//! `entries`. Removing an applet in COSMIC Settings takes it out of these
//! lists; logging out doesn't, so a theme isn't put back at every logout.
//! These configs are only read, never written.

use cosmic_config::{ConfigGet, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// The config listing the panels.
pub const PANELS_ID: &str = "com.system76.CosmicPanel";
/// The panels' config version.
pub const PANEL_VERSION: u64 = 1;

/// One panel's applets, as cosmic-panel stores them.
#[derive(Clone, CosmicConfigEntry, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[version = 1]
#[serde(default)]
pub struct PanelApplets {
    pub plugins_center: Option<Vec<String>>,
    pub plugins_wings: Option<(Vec<String>, Vec<String>)>,
}

impl PanelApplets {
    /// Whether the panel holds the applet `app_id` anywhere.
    #[must_use]
    pub fn holds(&self, app_id: &str) -> bool {
        let center = self.plugins_center.iter().flatten();
        let wings = self
            .plugins_wings
            .iter()
            .flat_map(|(left, right)| left.iter().chain(right));
        center.chain(wings).any(|applet| applet == app_id)
    }
}

/// The config ID of each panel (`com.system76.CosmicPanel.Panel` and so
/// on); `None` if the panel list can't be read.
#[must_use]
pub fn panel_ids() -> Option<Vec<String>> {
    let entries = cosmic_config::Config::new(PANELS_ID, PANEL_VERSION)
        .ok()?
        .get::<Vec<String>>("entries")
        .ok()?;
    Some(
        entries
            .into_iter()
            .map(|name| format!("{PANELS_ID}.{name}"))
            .collect(),
    )
}

/// Whether any panel holds the applet `app_id`, read from the panels'
/// configs now. A panel list or panel config that can't be read counts as
/// holding it, so a read failure never makes the applet think it was
/// removed. Blocking.
#[must_use]
pub fn in_any_panel(app_id: &str) -> bool {
    let Some(ids) = panel_ids() else {
        return true;
    };
    ids.iter().any(|id| {
        cosmic_config::Config::new(id, PANEL_VERSION).map_or(true, |config| {
            PanelApplets::get_entry(&config).map_or(true, |applets| applets.holds(app_id))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "io.github.deepthinker2001.CosmicThemeCycler";

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|&name| name.to_owned()).collect()
    }

    #[test]
    fn found_in_the_center_or_either_wing() {
        let center = PanelApplets {
            plugins_center: Some(names(&[ME])),
            ..PanelApplets::default()
        };
        let left = PanelApplets {
            plugins_wings: Some((names(&[ME]), Vec::new())),
            ..PanelApplets::default()
        };
        let right = PanelApplets {
            plugins_wings: Some((Vec::new(), names(&["other", ME]))),
            ..PanelApplets::default()
        };
        assert!(center.holds(ME) && left.holds(ME) && right.holds(ME));
    }

    #[test]
    fn not_found_when_absent_or_unset() {
        let others = PanelApplets {
            plugins_center: Some(names(&["com.system76.CosmicAppletTime"])),
            plugins_wings: Some((names(&["other"]), Vec::new())),
        };
        assert!(!others.holds(ME));
        assert!(!PanelApplets::default().holds(ME));
    }

    #[test]
    fn reads_cosmic_panel_s_own_format() {
        let wings: Option<(Vec<String>, Vec<String>)> = ron::from_str(
            r#"Some(([
    "com.system76.CosmicPanelWorkspacesButton",
    "io.github.deepthinker2001.CosmicThemeCycler",
], [
    "com.system76.CosmicAppletPower",
]))"#,
        )
        .unwrap();
        let panel = PanelApplets {
            plugins_wings: wings,
            ..PanelApplets::default()
        };
        assert!(panel.holds(ME));
    }
}
