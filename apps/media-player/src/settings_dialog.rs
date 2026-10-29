//! Settings as a card over the window: the groups listed on the left, the
//! chosen group's options on the right, and a search across all of them.

use crate::sections::settings;
use crate::{prefs, widgets, window};
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};

pub use nyx_ui::settings_card::{escape, fit, focus_search, is_open};

/// Each settings group's icon and one-line summary in the dialog's list.
const GROUPS: &[(&str, &str, &str)] = &[
    ("Library", "folder-videos-symbolic", "Folders, rescans"),
    ("Playback", "media-playback-start-symbolic", "Resume, skips, next episode"),
    ("Subtitles and sound", "nmp-subtitles-symbolic", "Languages, size, night mode"),
    ("Online details", "network-workgroup-symbolic", "Posters and details from TMDB"),
    ("Seek bar previews", "nmp-video-symbolic", "Thumbnails while seeking"),
];

type Shortcut = (&'static [&'static str], &'static str);

/// Every shortcut in one list, for the card's Keyboard group.
fn shortcuts() -> &'static [Shortcut] {
    static ALL: std::sync::OnceLock<Vec<Shortcut>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| window::SHORTCUTS.iter().flat_map(|(_, keys)| keys.iter().copied()).collect())
}

fn spec() -> Spec {
    Spec {
        groups: GROUPS,
        search_text: None,
        standard: Some(Standard {
            app: "media-player",
            about: About {
                name: "Media Player",
                icon: "io.github.design_nexus.nyx.MediaPlayer",
                version: env!("CARGO_PKG_VERSION"),
                description: "Your movies and TV shows, with subtitles, playlists and an equalizer.",
                licence: "MIT",
                website: "https://github.com/design-nexus/nyx-media-player",
                issues: "https://github.com/design-nexus/nyx-media-player/issues",
                credits: &[("mpv", "playback"), ("FFmpeg", "previews"), ("TMDB", "posters and details")],
            },
            shortcuts: shortcuts(),
            reduce_motion: || prefs::get().reduce_motion,
            set_reduce_motion: |on| prefs::update(|p| p.reduce_motion = on),
        }),
    }
}

pub fn open() {
    let (Some(overlay), Some(window)) = (window::overlay(), window::window()) else { return };
    settings_card::open(&overlay, &window, spec(), || {
        let page = widgets::page("settings");
        settings::build(&page);
        (page.root.upcast(), page.body)
    });
}
