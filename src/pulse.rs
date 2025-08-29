use std::{io, process::ExitStatus, time::Duration};

use cosmic_settings_daemon_config::{CosmicSettingsDaemonConfig, CosmicSettingsDaemonState};

use cosmic_config::CosmicConfigEntry;
use cosmic_settings_subscriptions::pulse::{self, Availability, PortType};
use futures::StreamExt;

pub const VIRT_MONO: &'static str = "COSMIC_mono_sink";

async fn load_virt_mono(sink: &str) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("pactl")
        .arg("load-module")
        .arg("module-remap-sink")
        .arg(format!("sink_name=\"{VIRT_MONO}\""))
        .arg(format!("master=\"{sink}\""))
        .arg("channels=1")
        .arg("channel_map=mono")
        .arg("sink_properties=device.description=\"MONO\"")
        .output()
        .await?;

    let id = String::from_utf8(output.stdout)?;
    tokio::process::Command::new("pactl")
        .arg("set-default-sink")
        .arg(VIRT_MONO)
        .spawn()?
        .wait()
        .await?;

    Ok(id)
}
async fn get_virt_sink_id() -> anyhow::Result<Option<String>> {
    let output = tokio::process::Command::new("pactl")
        .arg("list")
        .arg("modules")
        .arg("short")
        .output()
        .await?;
    let output = String::from_utf8(output.stdout)?;
    Ok(output.lines().into_iter().find_map(|l| {
        let mut split = l.split_whitespace();
        let Some(id) = split.next() else {
            return None;
        };
        split
            .nth(1)
            .is_some_and(|name| name.contains(VIRT_MONO))
            .then(|| id.to_string())
    }))
}

async fn unload_virt_mono(id: &str, old_sink: Option<&str>) -> io::Result<ExitStatus> {
    if let Some(old_sink) = old_sink {
        tokio::process::Command::new("pactl")
            .arg("set-default-sink")
            .arg(old_sink)
            .spawn()?
            .wait()
            .await?;
    }

    let res = tokio::process::Command::new("pactl")
        .arg("unload-module")
        .arg(id)
        .spawn()?
        .wait()
        .await?;

    Ok(res)
}

pub(crate) async fn pulse(
    mut sigterm_rx: tokio::sync::broadcast::Receiver<()>,
    mut mono_rx: tokio::sync::mpsc::Receiver<()>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = futures::channel::mpsc::channel(1);
    let (kill_tx, kill_rx) = futures::channel::oneshot::channel();
    _ = std::thread::spawn(move || {
        pulse::thread(tx);
        _ = kill_tx.send(());
    });

    let state_helper = CosmicSettingsDaemonState::config()?;
    let mut state = CosmicSettingsDaemonState::get_entry(&state_helper).unwrap_or_default();
    if state.default_sink_name == VIRT_MONO {
        state.default_sink_name = String::new();
    }
    let config_helper = CosmicSettingsDaemonConfig::config()?;
    let mut config = CosmicSettingsDaemonConfig::get_entry(&config_helper).unwrap_or_default();

    let mut mono_change = config.mono_sound;
    let mut sink_change = false;
    let mut retry = 0;
    loop {
        if retry > 0 {
            tokio::time::sleep(Duration::from_millis(2_u64.saturating_pow(retry))).await;
        }
        if !state.default_sink_name.is_empty() || (mono_change || sink_change) {
            let virt_mono_id = match get_virt_sink_id().await {
                Ok(v) => v,
                Err(err) => {
                    retry += 1;
                    log::error!("{err:?}");
                    continue;
                }
            };

            let cur_state = State::new(virt_mono_id, config.mono_sound);
            if mono_change {
                if config.mono_sound {
                    if let Err(err) = cur_state.enable_mono(&state.default_sink_name).await {
                        retry += 1;
                        log::error!("Failed to enable mono sound: {err:?}");
                        continue;
                    }
                } else {
                    if let Err(err) = cur_state.disable_mono(&state.default_sink_name).await {
                        retry += 1;
                        log::error!("Failed to disable mono sound: {err:?}");
                        continue;
                    }
                }
                mono_change = false;
            } else if sink_change {
                if let Err(err) = cur_state.sink_change(&state.default_sink_name).await {
                    retry += 1;
                    log::error!("Failed to handle sink change: {err:?}");
                    continue;
                }
                sink_change = false;
            }
        }

        let mono_toggle = mono_rx.recv();
        let pulse_msg = rx.next();
        let exit = sigterm_rx.recv();
        tokio::select! {
            enabled = mono_toggle => {
                if enabled.is_none() {
                    anyhow::bail!("Mono config receiver channel closed exited");
                };
                mono_change = true;
                match CosmicSettingsDaemonConfig::get_entry(&config_helper) {
                    Ok(c) => {
                        config = c;
                    }
                    Err(err) => {
                        log::error!("Failed to load daemon config: {err:?}");
                        retry += 1;
                        continue;
                    }
                }
            }
            msg = pulse_msg => {
                let Some(msg) = msg else {
                    anyhow::bail!("Pulse thread exited");
                };
                match msg {
                    pulse::Event::DefaultSink(name) => {
                        if name != VIRT_MONO {
                            if let Err(err) = state.set_default_sink_name(&state_helper, name) {
                                log::error!("{err:?}");
                            }
                            sink_change = true;
                        }
                    },
                    pulse::Event::CardInfo(info) => {

                        if info.ports.iter().any(|port| matches!(port.port_type, PortType::Headphones) && matches!(port.availability, Availability::Yes | Availability::Unknown)) &&
                            info.ports.iter().any(|port| matches!(port.port_type, PortType::Headset) && matches!(port.availability, Availability::Unknown))
                        {
                            let card_name = &info.name;
                            let Some(headphone_profile) = info.ports.iter().find(|port| matches!(port.port_type, PortType::Headphones)).and_then(|port| port.profiles.iter().max_by_key(|p| p.priority)) else {
                                log::error!("No headphone profile found for card: {}", card_name);
                                continue;
                            };
                            let Some(headset_profile) = info.ports.iter().find(|port| matches!(port.port_type, PortType::Headset)).and_then(|port| port.profiles.iter().max_by_key(|p| p.priority)) else {
                                log::error!("No headset profile found for card: {}", card_name);
                                continue;
                            };
                            let old_card = state.last_card.replace((card_name.clone(), headphone_profile.name.clone(), headset_profile.name.clone()));
                            if state.last_card == old_card {
                                log::trace!("Skipping update for tracked card and ports");
                                continue;
                            }

                            tokio::spawn({
                                let card_name = card_name.clone();
                                let headphone_profile = headphone_profile.name.clone();
                                let headset_profile = headset_profile.name.clone();
                                async move {
                                    for retry in 1..5 {
                                    let c = tokio::process::Command::new("cosmic-osd")
                                        .arg("confirm-headphones")
                                        .arg("--card-name")
                                        .arg(&card_name)
                                        .arg("--headphone-profile")
                                        .arg(&headphone_profile)
                                        .arg("--headset-profile")
                                        .arg(&headset_profile)
                                        .spawn();
                                        match c {
                                            Ok(mut child) => {
                                                match child.wait().await {
                                                    Ok(status) if !status.success() => {
                                                        _ = tokio::time::sleep(Duration::from_secs(retry)).await;
                                                    }
                                                    Err(err) => {
                                                        _ = tokio::time::sleep(Duration::from_secs(retry)).await;
                                                        log::warn!("Failed to wait for cosmic-osd process: {err:?}");
                                                    }
                                                    _ => {
                                                        break;
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                log::warn!("Failed to spawn cosmic-osd: {e:?}");
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                    // don't need to know any of this info
                    pulse::Event::Balance(_)
                    | pulse::Event::DefaultSource(_)
                    | pulse::Event::SinkVolume(_)
                    | pulse::Event::Channels(..)
                    | pulse::Event::SinkMute(_)
                    | pulse::Event::SourceVolume(_)
                    | pulse::Event::SourceMute(_) => {},
                };
            }
            _ = exit => {
                break;
            }
        };

        retry = 0;
    }
    if let Err(_) = tokio::time::timeout(Duration::from_secs(10), kill_rx).await {
        log::error!("Pulse thread did not exit...");
        std::process::exit(1);
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub enum State {
    NoVirtMonoDisabledMono,
    DisabledMono(String),
    EnabledMono(String),
    NoVirtMonoEnabledMono,
}
impl State {
    fn new(virt_sink: Option<String>, mono_enabled: bool) -> State {
        if mono_enabled {
            if let Some(virt_sink) = virt_sink {
                State::EnabledMono(virt_sink)
            } else {
                State::NoVirtMonoEnabledMono
            }
        } else {
            if let Some(virt_sink) = virt_sink {
                State::DisabledMono(virt_sink)
            } else {
                State::NoVirtMonoDisabledMono
            }
        }
    }

    async fn enable_mono(&self, sink_name: &str) -> anyhow::Result<()> {
        match self {
            State::DisabledMono(virt_sink) | State::EnabledMono(virt_sink) => {
                // Perhaps there is something wrong if this is the case
                if let Some(code) = unload_virt_mono(virt_sink, None)
                    .await?
                    .code()
                    .filter(|c| *c != 0)
                {
                    anyhow::bail!("Failed to unload virtual mono sink module: {code:?}")
                }
                if let Err(err) = load_virt_mono(sink_name).await {
                    anyhow::bail!("Failed to load virtual mono sink module: {err:?}")
                }
            }
            State::NoVirtMonoEnabledMono | State::NoVirtMonoDisabledMono => {
                if let Err(err) = load_virt_mono(sink_name).await {
                    anyhow::bail!("Failed to load virtual mono sink module: {err:?}")
                }
            }
        }
        Ok(())
    }

    async fn disable_mono(&self, sink_name: &str) -> anyhow::Result<()> {
        match self {
            State::DisabledMono(virt_sink) | State::EnabledMono(virt_sink) => {
                if let Some(code) = unload_virt_mono(virt_sink, Some(sink_name))
                    .await?
                    .code()
                    .filter(|c| *c != 0)
                {
                    anyhow::bail!("Failed to unload virtual mono sink module: {code:?}")
                }
            }
            State::NoVirtMonoDisabledMono | State::NoVirtMonoEnabledMono => {}
        }
        Ok(())
    }

    async fn sink_change(&self, sink_name: &str) -> anyhow::Result<()> {
        match self {
            State::NoVirtMonoDisabledMono => {}
            State::DisabledMono(virt_sink) => {
                if let Some(code) = unload_virt_mono(virt_sink, Some(sink_name))
                    .await?
                    .code()
                    .filter(|c| *c != 0)
                {
                    anyhow::bail!("Failed to unload virtual mono sink module: {code:?}")
                }
            }
            State::EnabledMono(virt_sink) => {
                // Perhaps there is something wrong if this is the case
                if let Some(code) = unload_virt_mono(virt_sink, None)
                    .await?
                    .code()
                    .filter(|c| *c != 0)
                {
                    anyhow::bail!("Failed to unload virtual mono sink module: {code:?}")
                }
                if let Err(err) = load_virt_mono(sink_name).await {
                    anyhow::bail!("Failed to load virtual mono sink module: {err:?}")
                }
            }
            State::NoVirtMonoEnabledMono => {
                if let Err(err) = load_virt_mono(sink_name).await {
                    anyhow::bail!("Failed to load virtual mono sink module: {err:?}")
                }
            }
        }
        Ok(())
    }
}
