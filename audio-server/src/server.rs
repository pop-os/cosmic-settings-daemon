// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

//! Interfaces for implementing the varlink methods for `com.system76.CosmicSettings.Audio`.

use cosmic_settings_audio_core::{Error, Mute, Node, Volume};
use std::{os::fd::OwnedFd, sync::Arc};

use crate::{config, context};

pub struct Server {
    pub backend: context::Context,
}

impl Server {
    pub async fn new(context: context::Context) -> Self {
        Self { backend: context }
    }

    /// Request a non-blocking anonymous pipe for receiving audio events from the server.
    pub async fn recv_events(&mut self) -> Result<OwnedFd, Error> {
        // Create an anonymous block
        let (writer, reader) = tokio::net::unix::pipe::pipe().map_err(|why| Error::IO {
            code: why.raw_os_error(),
            why: format!("{}", why),
        })?;

        let reader = reader.into_nonblocking_fd().map_err(|why| Error::IO {
            code: why.raw_os_error(),
            why: format!("{}", why),
        })?;

        if let Err(_) = self
            .backend
            .sender
            .send(crate::backend::Message::Subscribe(Arc::new(writer)))
        {
            return Err(Error::ChannelSend);
        }

        Ok(reader)
    }

    pub async fn default_sink(&self) -> Option<Node> {
        let model = self.backend.model.lock().await;
        model.active_sink_node.map(|id| Node {
            id,
            name: model.active_sink_node_name.clone(),
        })
    }

    pub async fn default_source(&self) -> Option<Node> {
        let model = self.backend.model.lock().await;
        model.active_source_node.map(|id| Node {
            id,
            name: model.active_source_node_name.clone(),
        })
    }

    pub async fn set_default(&mut self, node_id: u32, save: bool) -> Result<(), Error> {
        let mut model = self.backend.model.lock().await;
        model.set_default(node_id, save);
        Ok(())
    }

    pub async fn set_profile(
        &mut self,
        device_id: u32,
        profile_index: u32,
        save: bool,
    ) -> Result<(), Error> {
        let mut model = self.backend.model.lock().await;
        model.set_profile(device_id, profile_index, save).await;
        Ok(())
    }

    pub async fn set_mono(&mut self, enabled: bool) -> Result<(), Error> {
        self.backend.model.lock().await.pipewire_send(
            cosmic_pipewire::Request::SetMetadataProperty {
                name: "sm-settings".to_owned(),
                subject: 0,
                key: "node.features.audio.mono".to_owned(),
                type_: Some("Spa:String:JSON".to_owned()),
                value: Some(format!("{{ \"value\": {enabled}, \"save\": true }}")),
            },
        );
        Ok(())
    }

    pub async fn source_mute_toggle<'a>(&'a mut self) -> Result<Mute, Error> {
        let mut model = self.backend.model.lock().await;
        let Some(node_id) = model.active_source_node else {
            return Err(Error::NoActiveSource);
        };

        let mute = !model.source_mute;
        set_node_mute(&mut model, node_id, mute)
    }

    pub async fn source_volume_lower<'a>(&'a mut self, step: u32) -> Result<Volume, Error> {
        let mut model = self.backend.model.lock().await;
        let Some(id) = model.active_source_node else {
            return Err(Error::NoActiveSource);
        };

        if model.source_volume == 0 {
            return Ok(Volume {
                id,
                volume: 0,
                balance: None,
            });
        }

        let volume = round_down(model.source_volume, step);
        set_node_volume(&mut model, id, volume, None)
    }

    pub async fn source_volume_raise<'a>(&'a mut self, step: u32) -> Result<Volume, Error> {
        let mut model = self.backend.model.lock().await;
        let Some(id) = model.active_source_node else {
            return Err(Error::NoActiveSource);
        };

        if model.source_volume == 100 {
            return Ok(Volume {
                id,
                volume: 100,
                balance: None,
            });
        }

        let volume = 100.min(round_up(model.source_volume, step));
        set_node_volume(&mut model, id, volume, None)
    }

    pub async fn sink_mute_toggle<'a>(&'a mut self) -> Result<Mute, Error> {
        let mut model = self.backend.model.lock().await;
        let Some(node_id) = model.active_sink_node else {
            return Err(Error::NoActiveSink);
        };

        let mute = !model.sink_mute;
        set_node_mute(&mut model, node_id, mute)
    }

    pub async fn sink_volume_lower(&mut self, step: u32) -> Result<Volume, Error> {
        let mut model = self.backend.model.lock().await;
        let Some(id) = model.active_sink_node else {
            return Err(Error::NoActiveSink);
        };

        if model.sink_volume == 0 {
            return Ok(Volume {
                id,
                volume: 0,
                balance: model.sink_balance,
            });
        }

        let volume = round_down(model.sink_volume, step);
        let balance = model.sink_balance;
        set_node_volume(&mut model, id, volume, balance)
    }

    pub async fn sink_volume_raise(&mut self, step: u32) -> Result<Volume, Error> {
        let max_volume = if config::amplification_sink().await {
            150
        } else {
            100
        };
        let mut model = self.backend.model.lock().await;
        let Some(id) = model.active_sink_node else {
            return Err(Error::NoActiveSink);
        };

        if model.sink_volume == max_volume {
            return Ok(Volume {
                id,
                volume: max_volume,
                balance: model.sink_balance,
            });
        }

        let volume = max_volume.min(round_up(model.sink_volume, step));
        let balance = model.sink_balance;
        set_node_volume(&mut model, id, volume, balance)
    }

    pub async fn set_sink_volume(&mut self, mut volume: u32) -> Result<Volume, Error> {
        volume = volume.min(if config::amplification_sink().await {
            150
        } else {
            100
        });

        let mut model = self.backend.model.lock().await;
        let node_id = model.active_sink_node.clone().ok_or(Error::NoActiveSink)?;
        let entry = model.node_volumes.entry(node_id).or_insert((100, None));

        entry.0 = volume;
        let (volume, balance) = *entry;
        set_node_volume(&mut model, node_id, volume, balance)
    }

    pub async fn set_source_volume(&mut self, volume: u32) -> Result<Volume, Error> {
        let mut model = self.backend.model.lock().await;
        let node_id = model
            .active_source_node
            .clone()
            .ok_or(Error::NoActiveSink)?;

        let entry = model.node_volumes.entry(node_id).or_insert((100, None));

        entry.0 = volume;
        let (volume, balance) = *entry;
        set_node_volume(&mut model, node_id, volume, balance)
    }

    pub async fn set_node_mute<'a>(&'a mut self, node_id: u32, mute: bool) -> Result<Mute, Error> {
        let mut model = self.backend.model.lock().await;
        set_node_mute(&mut model, node_id, mute)
    }

    pub async fn set_node_volume<'a>(
        &'a mut self,
        node_id: u32,
        volume: u32,
    ) -> Result<Volume, Error> {
        let mut model = self.backend.model.lock().await;
        let entry = model.node_volumes.entry(node_id).or_insert((100, None));

        entry.0 = volume;
        let (volume, balance) = *entry;
        set_node_volume(&mut model, node_id, volume, balance)
    }

    pub async fn set_node_volume_balance<'a>(
        &'a mut self,
        node_id: u32,
        balance: Option<f32>,
    ) -> Result<Volume, Error> {
        let mut model = self.backend.model.lock().await;
        let entry = model.node_volumes.entry(node_id).or_insert((100, None));

        entry.1 = balance;
        let (volume, balance) = *entry;
        set_node_volume(&mut model, node_id, volume, balance)
    }
}

fn set_node_mute(
    model: &mut crate::backend::Model,
    node_id: u32,
    mute: bool,
) -> Result<Mute, Error> {
    tracing::debug!(target: "varlink", node_id, mute, "set_node_mute");
    *model.node_mute.entry(node_id).or_default() = mute;

    if model.active_sink_node == Some(node_id) {
        model.sink_mute = mute;
    } else if model.active_source_node == Some(node_id) {
        model.source_mute = mute;
    }

    model.pipewire_send(cosmic_pipewire::Request::SetNodeMute(node_id, mute));

    Ok(Mute { id: node_id, mute })
}

fn set_node_volume<'a>(
    model: &mut crate::backend::Model,
    node_id: u32,
    volume: u32,
    balance: Option<f32>,
) -> Result<Volume, Error> {
    tracing::debug!(target: "varlink", node_id, volume, balance, "set_node_volume");
    let entry = model.node_volumes.entry(node_id).or_default();
    entry.0 = volume;
    entry.1 = balance;

    if model.active_sink_node == Some(node_id) {
        model.sink_volume = volume;
        model.sink_balance = balance;
    } else if model.active_source_node == Some(node_id) {
        model.source_volume = volume;
    }

    model.pipewire_send(cosmic_pipewire::Request::SetNodeVolume(
        node_id,
        volume as f32 / 100.0,
        balance,
    ));

    Ok(Volume {
        id: node_id,
        volume,
        balance,
    })
}

/// Round a volume to the nearest increment above the requested step.
/// ie: 54 + 5 will be 55 while 55 + 5 is 60.
fn round_up(num: u32, step: u32) -> u32 {
    ((num / step) + 1) * step
}

/// Round a volume to the nearest decrement below the requested step.
/// ie: 54 - 5 will be 50 while 50 - 5 is 45.
fn round_down(num: u32, step: u32) -> u32 {
    ((num / step) * step) - if num % step > 0 { 0 } else { step }
}

#[cfg(test)]
mod tests {
    use super::{round_down, round_up};

    #[test]
    fn volume_round_up() {
        assert_eq!(round_up(50, 5), 55);
        assert_eq!(round_up(51, 5), 55);
        assert_eq!(round_up(52, 5), 55);
        assert_eq!(round_up(53, 5), 55);
        assert_eq!(round_up(54, 5), 55);
        assert_eq!(round_up(55, 5), 60);
        assert_eq!(round_up(56, 5), 60);
    }

    #[test]
    fn volume_round_down() {
        assert_eq!(round_down(50, 5), 45);
        assert_eq!(round_down(51, 5), 50);
        assert_eq!(round_down(52, 5), 50);
        assert_eq!(round_down(53, 5), 50);
        assert_eq!(round_down(54, 5), 50);
        assert_eq!(round_down(55, 5), 50);
        assert_eq!(round_down(56, 5), 55);
    }
}
