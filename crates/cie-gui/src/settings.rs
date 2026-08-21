//! What the user has decided once and does not want to decide again.
//!
//! Everything else in this program is about *this* document: which file, which
//! page, where the box goes. What lives here outlasts the window - the scan of
//! a handwritten signature, and where it sits in the stamp - so it goes in a
//! small JSON file beside the desktop's other configuration.
//!
//! The picture is kept by **path**, not copied into the settings. Re-scanning
//! over the same file then updates the stamp on the next run, and a settings
//! file that has to be readable stays a few lines long.
//!
//! Nothing here is a secret. The PIN lives in the system keyring
//! (`cie_firma::keyring`) and must not be tempted into this file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cie_firma::{StampLayout, Template};
use rust_i18n::t;
use serde::{Deserialize, Serialize};

use crate::theme::Mode;

/// The file, under the platform's configuration directory.
const FILE: &str = "gui.json";
const DIR: &str = "cie";

/// The defaults are spelled out per field rather than with `#[serde(default)]`
/// on the struct, and the difference matters: on the struct, a missing
/// `templates` key would be filled from [`Settings::default`] - which has one -
/// and [`Settings::settle`] would never see the empty list that tells it a file
/// predates templates and still has a picture to carry over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The boxes the user has composed. Never empty once it has been through
    /// [`Settings::settle`]: an editor that opens on nothing is an editor
    /// nobody knows how to start.
    #[serde(default)]
    pub templates: Vec<Template>,
    /// Which one gets used, as an index into `templates`. Kept in range rather
    /// than trusted: the file is editable by hand.
    #[serde(default)]
    pub active: usize,

    /// Which look the window wears.
    ///
    /// Here rather than in `State` alone because it is exactly the kind of
    /// thing this file is for: decided once, and not worth deciding again on
    /// every start. Ctrl+Z does not reach it - see `State::settings_replaced`.
    #[serde(default)]
    pub theme: Mode,

    /// Whether a dragged piece latches on to the lines the others make.
    ///
    /// On by default, because most of the time lining things up by eye is the
    /// harder way to do it. Off is not a lesser mode: a signature that wants
    /// to sit a hair off a line has nowhere else to be.
    #[serde(default = "yes")]
    pub snapping: bool,

    /// Where the picture used to live, before there were templates.
    ///
    /// Read once, to carry an existing setting into a template, and never
    /// written again - so the file tidies itself up the first time it is
    /// saved. Same for the layout beside it.
    #[serde(default, skip_serializing)]
    signature_image: Option<PathBuf>,
    #[serde(default, skip_serializing)]
    stamp_layout: StampLayout,
}

/// A setting added after there were settings files defaults to what those
/// files behaved like, so reading one back changes nothing.
fn yes() -> bool {
    true
}

impl Default for Settings {
    /// One template, so [`Settings::active`] always has something to hand
    /// back. An empty list is a state only a settings file can be in, and only
    /// until `settle` has looked at it.
    fn default() -> Self {
        Self {
            templates: vec![Template::classic(t!("template.plain"))],
            active: 0,
            theme: Mode::default(),
            snapping: true,
            signature_image: None,
            stamp_layout: StampLayout::default(),
        }
    }
}

impl Settings {
    /// The template in use.
    pub fn active(&self) -> &Template {
        let index = self.active.min(self.templates.len().saturating_sub(1));
        self.templates
            .get(index)
            .expect("load and every edit leave at least one template")
    }

    /// The template in use, to be changed.
    pub fn active_mut(&mut self) -> &mut Template {
        let index = self.active.min(self.templates.len().saturating_sub(1));
        self.templates
            .get_mut(index)
            .expect("load and every edit leave at least one template")
    }

    /// Fills in what a settings file cannot be trusted to have.
    ///
    /// A first run has no templates at all; a file from before templates
    /// existed has a picture and a layout instead, and losing that would mean
    /// somebody who had already set their signature up finding it gone. A file
    /// somebody has edited by hand can have an `active` pointing anywhere.
    fn settle(&mut self) {
        if self.templates.is_empty() {
            self.templates.push(Template::classic(t!("template.plain")));

            if let Some(picture) = self.signature_image.take() {
                self.templates
                    .push(Template::handwritten(t!("template.handwritten"), picture));
                // What they had before was a stamp with their signature in it,
                // so that is the one to open on.
                self.active = self.templates.len() - 1;
            }
        }
        self.active = self.active.min(self.templates.len() - 1);
    }
}

impl Settings {
    /// Reads the settings, or hands back the defaults.
    ///
    /// A missing file is the normal case on a first run, and a corrupt one is
    /// not worth a dialog: the defaults work, and the next save rewrites it.
    pub fn load(path: &Path) -> Self {
        let mut settings: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        settings.settle();
        settings
    }

    /// Writes the settings, creating the directory if this is the first time.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creazione di {}", dir.display()))?;
        }
        let raw = serde_json::to_string_pretty(self).context("serializzazione")?;
        std::fs::write(path, raw).with_context(|| format!("scrittura di {}", path.display()))
    }
}

/// Where the settings file belongs on this platform.
///
/// Hand-rolled rather than pulled from a crate: three environment variables
/// against one more dependency in a program that is already careful about how
/// many it carries. `None` means we could not work out a home directory, and
/// then nothing is remembered rather than something being written to a guess.
pub fn path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        // Roaming, so the signature follows the user between machines on a
        // domain - which is exactly the kind of place a scanned signature is
        // set up once.
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?
    };

    Some(base.join(DIR).join(FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is written has to come back: a setting that does not survive a
    /// restart is a setting the user has to make every time.
    #[test]
    fn the_settings_survive_the_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        // A path two levels deep, so the missing directory is created too.
        let path = dir.path().join("cie").join("gui.json");

        let settings = Settings {
            templates: vec![
                Template::classic("uno"),
                Template::handwritten("due", PathBuf::from("/tmp/firma.png")),
            ],
            active: 1,
            theme: Mode::Dark,
            ..Settings::default()
        };
        settings.save(&path).unwrap();

        assert_eq!(Settings::load(&path), settings);
    }

    /// Somebody who had already set their signature up must not open the new
    /// version and find it gone. The old picture becomes a template with the
    /// signature in it, and that is the one they land on.
    #[test]
    fn a_settings_file_from_before_templates_keeps_its_picture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.json");
        std::fs::write(
            &path,
            r#"{"signature_image": "/tmp/firma.png", "stamp_layout": "image-left"}"#,
        )
        .unwrap();

        let settings = Settings::load(&path);

        assert_eq!(settings.templates.len(), 2, "{:?}", settings.templates);
        assert!(settings.active().holds(&cie_firma::Piece::Image));
        assert_eq!(
            settings.active().image.as_deref(),
            Some(Path::new("/tmp/firma.png"))
        );

        // And the old keys go on the next save, rather than lingering to be
        // migrated a second time.
        settings.save(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("signature_image"), "{raw}");
        assert!(!raw.contains("stamp_layout"), "{raw}");
    }

    /// A file somebody has edited by hand can point `active` anywhere. It must
    /// not be able to point it off the end.
    #[test]
    fn an_active_out_of_range_is_pulled_back_in() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.json");
        std::fs::write(&path, r#"{"templates": [], "active": 47}"#).unwrap();

        let settings = Settings::load(&path);

        assert_eq!(settings.active, 0);
        assert_eq!(settings.templates.len(), 1);
    }

    /// A first run, and a file somebody has edited into nonsense, both have to
    /// end up with a working program rather than an error.
    #[test]
    fn a_missing_or_broken_file_gives_the_defaults() {
        let dir = tempfile::tempdir().unwrap();

        let missing = dir.path().join("mai-scritto.json");
        assert_eq!(Settings::load(&missing), Settings::default());

        let broken = dir.path().join("rotto.json");
        std::fs::write(&broken, "{ questo non e' JSON").unwrap();
        assert_eq!(Settings::load(&broken), Settings::default());
    }

    /// There is always a template to sign with, from the very first run:
    /// `active()` has no way to say "none" and nothing should have to ask.
    #[test]
    fn there_is_always_a_template() {
        for settings in [Settings::default(), Settings::load(Path::new("/mai/esistito"))] {
            assert_eq!(settings.templates.len(), 1);
            assert!(!settings.active().holds(&cie_firma::Piece::Image));
            assert_eq!(settings.active().image, None);
        }
    }
}
