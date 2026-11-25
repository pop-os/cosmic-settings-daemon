// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use cosmic_config::ConfigGet;
use std::borrow::Cow;
use std::collections::{HashSet, VecDeque};
use std::ops::ControlFlow;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use walkdir::WalkDir;

const SETTINGS_CONFIG: &str = "com.system76.CosmicSettings";

/// Create a new [`Sound`] for selecting and playing a sound from a FD.o sound theme.
pub const fn select<'a>(names: &'a [&'a str]) -> Sound<'a> {
    Sound::new(names)
}

/// A builder for selecting and playing a sound from a FD.o sound theme.
///
/// # Example
///
/// ```
/// async fn low_battery_notification() {
///     cosmic_sound_theme::select(&["battery-low"])
///         // Override the sound themes searched
///         .themes(&["cosmic", "freedesktop"])
///         // Equivalent to `media_role("Notification")`
///         .notification_role()
///         // Defaults to `Stereo`
///         .output_profile(cosmic_sound_theme::OutputProfile::Surround51)
///         // Play with `pw-play`
///         .play()
///         .await;
/// }
/// ```
#[must_use]
pub struct Sound<'a> {
    names: &'a [&'a str],
    output_profile: OutputProfile,
    media_role: Option<&'a str>,
    themes: Option<&'a [&'a str]>,
}

impl<'a> Sound<'a> {
    /// Create a new [`Sound`] for selecting and playing a sound from a FD.o sound theme.
    #[inline]
    pub const fn new(names: &'a [&'a str]) -> Self {
        Self {
            names,
            output_profile: OutputProfile::Stereo,
            media_role: None,
            themes: None,
        }
    }

    /// Choose from an output profile if a theme supports it.
    ///
    /// Defaults to [`OutputProfile::Stereo`].
    #[inline]
    pub fn output_profile(mut self, profile: OutputProfile) -> Self {
        self.output_profile = profile;
        self
    }

    /// Search these themes instead of the system's default preference
    #[inline]
    pub fn themes(mut self, themes: &'a [&'a str]) -> Self {
        self.themes = Some(themes);
        self
    }

    /// Assign a pulseaudio/pipewire media role to the sound being played
    #[inline]
    pub fn media_role(mut self, role: &'a str) -> Self {
        self.media_role = Some(role);
        self
    }

    /// Shorthand for `media_role("A11y")`
    #[inline]
    pub fn a11y_role(self) -> Self {
        self.media_role("A11y")
    }

    /// Shorthand for `media_role("Notification")`
    #[inline]
    pub fn notification_role(self) -> Self {
        self.media_role("Notification")
    }

    /// Select the sound file and play it with `pw-play`.
    pub async fn play(self) {
        let themes: Vec<Cow<'a, str>> = match self.themes {
            Some(themes) => themes.iter().cloned().map(Cow::Borrowed).collect(),
            None => sound_theme().into_iter().map(Cow::Owned).collect(),
        };

        for sound in self.names {
            match sound_path(self.output_profile, &themes, sound).await {
                ControlFlow::Continue(None) => continue,

                ControlFlow::Continue(Some(path)) => {
                    play_file(&path, self.output_profile, self.media_role).await;
                    return;
                }

                ControlFlow::Break(()) => {
                    return;
                }
            }
        }
    }
}

/// Selects sounds that match a given audio channel configuration
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum OutputProfile {
    Stereo,
    Surround40,
    Surround51,
    Surround71,
}

/// Plays a sound file with `pw-play` with the given [`OutputProfile`] and optional media role.
pub async fn play_file(path: &Path, output_profile: OutputProfile, role: Option<&str>) {
    tracing::debug!("pw-play --media-role={role:?} {}", path.display());
    let mut cmd = tokio::process::Command::new("pw-play");

    if let Some(role) = role {
        cmd.arg(["--media-role=", role].concat().as_str());
    }

    match output_profile {
        OutputProfile::Stereo => (),
        OutputProfile::Surround40 => {
            cmd.arg("--channels=4").arg("--channel-map=surround-40");
        }
        OutputProfile::Surround51 => {
            cmd.arg("--channels=6").arg("--channel-map=surround-51");
        }
        OutputProfile::Surround71 => {
            cmd.arg("--channels=8").arg("--channel-map=surround-71");
        }
    }

    _ = cmd
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .await;
}

/// Get the configured sound theme from `com.system76.CosmicSettings/v1/sound_theme`.
pub fn sound_theme() -> Vec<String> {
    cosmic_config::Config::new(SETTINGS_CONFIG, 1)
        .inspect_err(|why| tracing::debug!("failed to read {SETTINGS_CONFIG} config: {why}"))
        .ok()
        .and_then(|config| config.get::<Vec<String>>("sound_theme").ok())
        .unwrap_or_else(|| {
            vec![
                String::from("cosmic"),
                String::from("Pop"),
                String::from("freedesktop"),
            ]
        })
}

async fn sound_path(
    output_profile: OutputProfile,
    themes: &[Cow<'_, str>],
    sound: &str,
) -> ControlFlow<(), Option<PathBuf>> {
    const BASE: &str = ".";
    const DEFAULT: &str = "default";
    const STEREO: &str = "stereo";
    const SURROUND51: &str = "5.1";
    const SURROUND71: &str = "7.1";

    let mut visited = HashSet::new();
    let mut search_paths = VecDeque::new();
    let sound_theme_paths = sound_theme_paths();

    for theme in themes {
        for sound_theme_path in &sound_theme_paths {
            let theme_path = sound_theme_path.join(theme.as_ref());
            let theme_exists = theme_path.exists();
            if theme_exists && visited.insert(theme_path.clone()) {
                search_paths.push_back(theme_path);
            }

            while let Some(theme_path) = search_paths.pop_front() {
                if !theme_path.exists() {
                    continue;
                }

                if let Ok(data) = tokio::fs::read_to_string(&theme_path.join("index.theme")).await {
                    for inherited_theme in read_inherits(&data) {
                        for sound_theme_path in &sound_theme_paths {
                            let inherited = sound_theme_path.join(inherited_theme);
                            if inherited.exists() && visited.insert(inherited.clone()) {
                                search_paths.push_back(inherited);
                                break;
                            }
                        }
                    }
                }

                let output_paths = match output_profile {
                    OutputProfile::Stereo => &[STEREO, DEFAULT, BASE][..],
                    OutputProfile::Surround51 | OutputProfile::Surround40 => {
                        &[SURROUND51, STEREO, DEFAULT, BASE][..]
                    }
                    OutputProfile::Surround71 => {
                        &[SURROUND71, SURROUND51, STEREO, DEFAULT, BASE][..]
                    }
                };

                for output_path in output_paths {
                    let main_dir = theme_path.join(output_path);
                    if !main_dir.exists() {
                        tokio::task::yield_now().await;
                        continue;
                    }

                    let entries = WalkDir::new(main_dir)
                        .follow_links(true)
                        .max_depth(5)
                        .into_iter()
                        .filter_map(Result::ok);

                    for entry in entries {
                        let path = entry.path();
                        if path.is_file()
                            && let Some(ext) = path.extension()
                            && path.file_stem().is_some_and(|stem| stem == sound)
                        {
                            if ext.as_bytes() == b"disabled" {
                                return ControlFlow::Break(());
                            }

                            if matches!(ext.as_bytes(), b"oga" | b"wav") {
                                return ControlFlow::Continue(Some(path.to_owned()));
                            }
                        }

                        tokio::task::yield_now().await;
                    }
                }
            }

            if theme_exists {
                break;
            }
        }
    }

    ControlFlow::Continue(None)
}

fn sound_theme_paths() -> Vec<PathBuf> {
    let base_dirs = xdg::BaseDirectories::new();
    let mut data_dirs: Vec<PathBuf> = vec![];

    for dir in base_dirs
        .get_data_home()
        .into_iter()
        .chain(base_dirs.get_data_dirs())
    {
        let path = dir.join("sounds");
        if path.exists() {
            data_dirs.push(path);
        }
    }

    data_dirs
}

fn read_inherits(data: &str) -> impl Iterator<Item = &str> {
    data.lines()
        .find_map(|line| line.strip_prefix("Inherits="))
        .into_iter()
        .flat_map(|line| line.split(','))
}

#[cfg(test)]
mod tests {
    const EXAMPLE: &str = "[Sound Theme]
Name=Pop
Comment=The Pop Sound Theme
Inherits=cosmic,freedesktop
Directories=stereo/alert stereo/action stereo/notification

[stereo/alert]
Context=Alert
OutputProfile=stereo

[stereo/action]
Context=Action
OutputProfile=stereo

[stereo/notification]
Context=Notification
OutputProfile=stereo";

    #[test]
    fn inherits() {
        assert_eq!(
            super::read_inherits(EXAMPLE).collect::<Vec<_>>(),
            vec!["cosmic", "freedesktop"]
        );
    }
}
