// TODO later...
// If configured to, run scripts in XDG_DATA_DIR/dark-mode.d/ or XDG_DATA_DIR/light-mode.d/
// when the theme is set to auto-export color palette, write to gtk3 / gtk4 / kde / ... css files
// read config file for lat/long

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::bail;
use chrono::{DateTime, Days, Local};
use cosmic::{config::CosmicTk, theme::CosmicTheme};
use cosmic_config::CosmicConfigEntry;
use cosmic_theme::{Theme, ThemeMode};

use geonames::GeoPosition;
use sunrise::{Coordinates, SolarDay, SolarEvent};
use tokio::time::Instant;
use tokio_stream::StreamExt;

#[derive(Debug)]
pub struct SunriseSunset {
    last_update: DateTime<Local>,
    sunrise: Instant,
    sunset: Instant,
    lat: f64,
    long: f64,
}

pub enum ThemeMsg {
    ThemeMode(String),
    /// true if dark
    Theme(bool),
    Tk(String),
}

impl SunriseSunset {
    pub fn new(lat: f64, long: f64, t: Option<DateTime<Local>>) -> anyhow::Result<Self> {
        let (system_t, instant_t, t) = if let Some(t) = t {
            let system_t = SystemTime::from(t);
            let system_now = SystemTime::now();
            let delta_t = system_t.duration_since(system_now)?;
            let instant_t = Instant::now()
                .checked_add(delta_t)
                .ok_or(anyhow::anyhow!("Could not calculate instant"))?;

            (system_t, instant_t, t)
        } else {
            (SystemTime::now(), Instant::now(), Local::now())
        };

        let naive_date = t.date_naive();
        let coords = Coordinates::new(lat, long).unwrap();
        let solar_day = SolarDay::new(coords, naive_date);
        let sunrise = solar_day.event_time(SolarEvent::Sunrise).timestamp();
        let sunset = solar_day.event_time(SolarEvent::Sunset).timestamp();

        let Some(sunrise) =
            UNIX_EPOCH.checked_add(std::time::Duration::from_secs(u64::try_from(sunrise)?))
        else {
            bail!("Failed to calculate sunrise time");
        };

        let Some(sunset) =
            UNIX_EPOCH.checked_add(std::time::Duration::from_secs(u64::try_from(sunset)?))
        else {
            bail!("Failed to calculate sunset time");
        };

        let st_to_instant = |now: SystemTime, st: SystemTime| -> anyhow::Result<Instant> {
            Ok(if st > now {
                instant_t
                    .checked_add(st.duration_since(now)?)
                    .ok_or(anyhow::anyhow!("Failed to convert system time to instant"))?
            } else {
                instant_t
                    .checked_sub(now.duration_since(st)?)
                    .ok_or(anyhow::anyhow!("Failed to convert system time to instant"))?
            })
        };

        Ok(Self {
            last_update: t,
            sunrise: st_to_instant(system_t, sunrise)?,
            sunset: st_to_instant(system_t, sunset)?,
            lat,
            long,
        })
    }

    pub fn is_dark(&self) -> anyhow::Result<bool> {
        if self.last_update.date_naive() != Local::now().date_naive() {
            bail!("SunriseSunset out of date");
        }

        let now = Instant::now();
        Ok(now < self.sunrise || now >= self.sunset)
    }

    pub fn next(&self) -> anyhow::Result<Instant> {
        let now = Instant::now();
        if self.sunrise.checked_duration_since(now).is_some() {
            Ok(self.sunrise)
        } else if self.sunset.checked_duration_since(now).is_some() {
            Ok(self.sunset)
        } else {
            bail!("SunriseSunset instants have already passed...");
        }
    }

    pub fn update_next(&mut self) -> anyhow::Result<Instant> {
        match self.next() {
            Ok(i) => Ok(i),
            Err(_) => {
                let Some(tomorrow) = self.last_update.checked_add_days(Days::new(1)) else {
                    bail!("Failed to calculate next date for theme auto-switch.");
                };
                *self = Self::new(self.lat, self.long, Some(tomorrow))?;
                self.next()
            }
        }
    }
}

pub async fn watch_theme(
    theme_mode_rx: &mut tokio::sync::mpsc::Receiver<ThemeMsg>,
    cleanup_tx: tokio::sync::mpsc::Sender<()>,
    mut sigterm_rx: tokio::sync::broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let mut override_until_next = false;

    let helper = ThemeMode::config()?;
    let mut theme_mode = match ThemeMode::get_entry(&helper) {
        Ok(t) => t,
        Err((errs, t)) => {
            for why in errs {
                eprintln!("{why}");
            }
            t
        }
    };

    let tk_helper = CosmicTk::config()?;
    let mut tk = match CosmicTk::get_entry(&tk_helper) {
        Ok(t) => t,
        Err((errs, t)) => {
            for why in errs {
                eprintln!("{why}");
            }
            t
        }
    };

    set_gnome_button_layout(tk.show_maximize, tk.show_minimize);

    let light_helper = CosmicTheme::light_config()?;
    let dark_helper = CosmicTheme::dark_config()?;

    if tk.apply_theme_global {
        // Write the gtk variables for both themes in case they have changed in the meantime
        let dark = match Theme::get_entry(&dark_helper) {
            Ok(t) => t,
            Err((errs, t)) => {
                for why in errs {
                    eprintln!("{why}");
                }
                t
            }
        };
        _ = dark.write_gtk4();
        let light = match Theme::get_entry(&light_helper) {
            Ok(t) => t,
            Err((errs, t)) => {
                for why in errs {
                    eprintln!("{why}");
                }
                t
            }
        };
        _ = light.write_gtk4();
        _ = std::process::Command::new("flatpak")
            .arg("override")
            .arg("--user")
            .arg("--filesystem=xdg-config/gtk-4.0:ro")
            .spawn();

        if !theme_mode.auto_switch {
            let t = if theme_mode.is_dark { dark } else { light };
            if let Err(err) = Theme::apply_gtk(t.is_dark) {
                eprintln!("Failed to apply the theme to gtk. {err:?}");
            }
        }

        set_gnome_desktop_interface(theme_mode.is_dark);
    } else {
        if let Err(err) = Theme::reset_gtk() {
            eprintln!("Failed to reset the application of the theme to gtk. {err:?}");
        }
    }

    // TODO allow preference for config file instead?
    let geodata = crate::location::decode_geodata();
    let (_location_handle, location_updates) = crate::location::receive_timezones();
    futures::pin_mut!(location_updates);

    let mut sunrise_sunset: Option<SunriseSunset> = None;
    loop {
        let sunset_deadline =
            if let Some(Some(s)) = theme_mode.auto_switch.then(|| sunrise_sunset.as_mut()) {
                Some(s.update_next()?)
            } else {
                None
            };

        let sleep = async move {
            if !theme_mode.auto_switch {
                std::future::pending().await
            } else if let Some(s) = sunset_deadline {
                tokio::time::sleep_until(s).await
            } else {
                std::future::pending().await
            }
        };

        tokio::select! {
            _ = sigterm_rx.recv() => {
                if let Err(err) = Theme::reset_gtk() {
                    eprintln!("Failed to reset the application of the theme to gtk. {err:?}");
                }
                cleanup_tx.send(()).await.unwrap();
            }
            changes = theme_mode_rx.recv() => {
                let Some(changes) = changes else {
                    bail!("Theme mode changes failed");
                };

                match changes {
                    ThemeMsg::ThemeMode(changes) => {
                        let (errs, _) = theme_mode.update_keys(&helper, &[changes]);

                        for err in errs {
                            eprintln!("Error updating the theme mode {err:?}");
                        }

                        if sunrise_sunset.as_ref().is_some_and(|s| s.is_dark().is_ok_and(|s_is_dark| s_is_dark != theme_mode.is_dark)) {
                            override_until_next = true;
                        } else {
                            override_until_next = false;
                        }

                        if theme_mode.auto_switch {
                            let Some(is_dark) = sunrise_sunset.as_ref().and_then(|s| s.is_dark().ok()) else {
                                continue;
                            };

                            if let Err(err) = theme_mode.set_is_dark(&helper, is_dark) {
                                eprintln!("Failed to update theme mode {err:?}");
                            }
                        }

                        if tk.apply_theme_global {
                            let theme = match if theme_mode.is_dark {
                                Theme::get_entry(&dark_helper)
                            } else {
                                Theme::get_entry(&light_helper)
                            } {
                                Ok(t) => t,
                                Err((errs, t)) => {
                                    for err in errs {
                                        eprintln!("{err}");
                                    }
                                    t
                                }
                            };

                            if let Err(err) = Theme::apply_gtk(theme.is_dark) {
                                eprintln!("Failed to apply the theme to gtk. {err:?}");
                            }

                            set_gnome_desktop_interface(theme_mode.is_dark);
                        }
                    },
                    ThemeMsg::Tk(changes) => {
                        let (errs, changes) = tk.update_keys(&tk_helper, &[changes]);

                        for err in errs {
                            eprintln!("Error updating the theme toolkit config {err:?}");
                        }

                        if changes.contains(&"icon_theme") {
                            set_gnome_icon_theme(tk.icon_theme.clone());
                        }

                        if changes.contains(&"show_maximize") || changes.contains(&"show_minimize") {
                            set_gnome_button_layout(tk.show_maximize, tk.show_minimize);
                        }

                        if !changes.contains(&"apply_theme_global") {
                            continue;
                        }

                        if tk.apply_theme_global {
                            // Write the gtk variables for both themes in case they have changed in the meantime
                            let dark = match Theme::get_entry(&dark_helper) {
                                Ok(t) => t,
                                Err((errs, t)) => {
                                    for why in errs {
                                        eprintln!("{why}");
                                    }
                                    t
                                }
                            };
                            _ = dark.write_gtk4();
                            let light = match Theme::get_entry(&light_helper) {
                                Ok(t) => t,
                                Err((errs, t)) => {
                                    for why in errs {
                                        eprintln!("{why}");
                                    }
                                    t
                                }
                            };
                            _ = light.write_gtk4();
                            let _ = std::process::Command::new("flatpak")
                                .arg("override")
                                .arg("--user")
                                .arg("--filesystem=xdg-config/gtk-4.0:ro")
                                .spawn();

                            let t = if theme_mode.is_dark { dark } else { light };
                            if let Err(err) = Theme::apply_gtk(t.is_dark) {
                                eprintln!("Failed to apply the theme to gtk. {err:?}");
                            }

                            set_gnome_desktop_interface(theme_mode.is_dark);
                        } else {
                            if let Err(err) = Theme::reset_gtk() {
                                eprintln!("Failed to reset the application of the theme to gtk. {err:?}");
                            }
                        }
                    },
                    ThemeMsg::Theme(is_dark) => {
                        let t = match Theme::get_entry(if is_dark {
                                &dark_helper
                            } else {
                                &light_helper
                            }) {
                                Ok(t) => t,
                                Err((errs, t)) => {
                                    for err in errs {
                                        eprintln!("Failed to load the theme. {err:?}");
                                    }
                                    t
                                },
                            };
                        if tk.apply_theme_global {
                            if let Err(err) = t.write_gtk4() {
                                eprintln!("Failed to write gtk4 css. {err:?}");
                            }
                            let theme_mode = match ThemeMode::get_entry(&helper) {
                                Ok(t) => t,
                                Err((errs, t)) => {
                                    for err in errs {
                                        eprintln!("Failed to load the theme mode. {err:?}");
                                    }
                                    t
                                },
                            };
                            if theme_mode.is_dark == is_dark {
                                if let Err(err) = Theme::apply_gtk(t.is_dark) {
                                    eprintln!("Failed to apply the theme to gtk. {err:?}");
                                }
                            }

                            set_gnome_desktop_interface(theme_mode.is_dark);
                        }
                    }
                }


            }
            _ = sleep => {
                if !theme_mode.auto_switch || override_until_next {
                    override_until_next = false;
                    continue;
                }
                // update the theme mode
                let Some(is_dark) = sunrise_sunset.as_ref().and_then(|s| s.is_dark().ok()) else {
                    continue;
                };

                if let Err(err) = theme_mode.set_is_dark(&helper, is_dark) {
                    eprintln!("Failed to update theme mode {err:?}");
                }
                if tk.apply_theme_global {
                    let theme = match if theme_mode.is_dark {
                        Theme::get_entry(&dark_helper)
                    } else {
                        Theme::get_entry(&light_helper)
                    } {
                        Ok(t) => t,
                        Err((errs, t)) => {
                            for err in errs {
                                eprintln!("{err}");
                            }
                            t
                        }
                    };
                    if let Err(err) = Theme::apply_gtk(theme.is_dark) {
                        eprintln!("Failed to apply the theme to gtk. {err:?}");
                    }

                    set_gnome_desktop_interface(theme_mode.is_dark);
                }
            }
            location_update = location_updates.next() => {
                if override_until_next {
                    continue;
                }
                // set the next timer
                // update the theme if necessary
                let Some(location_result) = location_update else {
                    continue;
                };

                let Ok(new_timezone) = location_result else {
                    continue;
                };

                let Some(&GeoPosition { latitude, longitude }) = geodata.get(&new_timezone) else {
                    eprintln!("no matching geodata for {new_timezone}");
                    continue;
                };

                match SunriseSunset::new(latitude, longitude, None) {
                    Ok(s) => {
                        sunrise_sunset = Some(s);
                    },
                    Err(err) => {
                        eprintln!("Failed to calculate sunrise and sunset for current location {err:?}");
                        sunrise_sunset = None;
                        continue;
                    },
                };

                if !theme_mode.auto_switch {
                    continue;
                }

                let Some(is_dark) = sunrise_sunset.as_ref().unwrap().is_dark().ok() else {
                    continue;
                };

                if let Err(err) = theme_mode.set_is_dark(&helper, is_dark) {
                    eprintln!("Failed to update theme mode {err:?}");
                }
                if tk.apply_theme_global {
                    let theme = match if theme_mode.is_dark {
                        Theme::get_entry(&dark_helper)
                    } else {
                        Theme::get_entry(&light_helper)
                    } {
                        Ok(t) => t,
                        Err((errs, t)) => {
                            for err in errs {
                                eprintln!("{err}");
                            }
                            t
                        }
                    };
                    if let Err(err) = Theme::apply_gtk(theme.is_dark) {
                        eprintln!("Failed to apply the theme to gtk. {err:?}");
                    }

                    set_gnome_desktop_interface(theme_mode.is_dark);
                }
            }

        }
    }
}

fn set_gnome_button_layout(show_maximize: bool, show_minimize: bool) {
    tokio::spawn(async move {
        let layout = match (show_maximize, show_minimize) {
            (true, true) => ":minimize,maximize,close",
            (true, false) => ":maximize,close",
            (false, true) => ":minimize,close",
            (false, false) => ":close",
        };

        let _res = tokio::process::Command::new("gsettings")
            .args(&[
                "set",
                "org.gnome.desktop.wm.preferences",
                "button-layout",
                layout,
            ])
            .status()
            .await;
    });
}

fn set_gnome_desktop_interface(is_dark: bool) {
    let (color_scheme, adw_theme, adw_theme_path) = if is_dark {
        (
            "prefer-dark",
            "adw-gtk3-dark",
            "/usr/share/themes/adw-gtk3-dark",
        )
    } else {
        ("prefer-light", "adw-gtk3", "/usr/share/themes/adw-gtk3")
    };

    tokio::spawn(async {
        let _res = tokio::process::Command::new("gsettings")
            .args(&[
                "set",
                "org.gnome.desktop.interface",
                "color-scheme",
                color_scheme,
            ])
            .status()
            .await;
    });

    if Path::new(adw_theme_path).exists() {
        tokio::spawn(async {
            let _res = tokio::process::Command::new("gsettings")
                .args(&["set", "org.gnome.desktop.interface", "gtk-theme", adw_theme])
                .status()
                .await;
        });
    }
}

fn set_gnome_icon_theme(theme: String) {
    tokio::spawn(async move {
        let _res = tokio::process::Command::new("gsettings")
            .args(&[
                "set",
                "org.gnome.desktop.interface",
                "icon-theme",
                theme.as_str(),
            ])
            .status()
            .await;
    });
}
