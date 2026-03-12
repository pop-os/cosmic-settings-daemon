// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

//! Varlink frontend for cosmic-settings-daemon

// TODO:
// - com.system76.CosmicConfig config, set_config, watch_config, state, set_state, watch_state,
// - com.system76.CosmicSettings.Display increase_brightness, decrease_brightness, set_brightness, recv_brightness,
// - com.system76.CosmicSettings.Keyboard increase_brightness, decrease_brightness, set_brightness, recv_brightness,
// - com.system76.CosmicSettings.InputSources switch

use cosmic_settings_audio_core as audio;
use cosmic_settings_audio_server as audio_server;
use std::{os::fd::OwnedFd, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

pub async fn init() -> (Daemon, impl Future<Output = ()> + 'static + Send) {
    let (audio_ctx, audio_ctx_rx) = audio_server::Context::new().await;

    let daemon = Daemon(Arc::new(Mutex::new(DaemonInner {
        audio_server: audio_server::Server::new(audio_ctx.clone()).await,
    })));

    (daemon, audio_ctx.run(audio_ctx_rx))
}

fn socket_path() -> PathBuf {
    dirs::runtime_dir()
        .expect("runtime dir required by varlink service")
        .join("com.system76.CosmicSettings")
}

pub struct Daemon(pub Arc<Mutex<DaemonInner>>);

impl Daemon {
    pub async fn run(self) {
        let socket_path = socket_path();
        let _ = tokio::fs::remove_file(&socket_path).await;
        let listener =
            zlink::unix::bind(&socket_path).expect("zlink service failed to bind unix socket");

        if let Err(why) = zlink::Server::new(listener, self).run().await {
            tracing::error!("zlink service failed: {}", why);
        }
    }
}

#[zlink::service(interface = "com.system76.CosmicSettings")]
impl<Sock> Daemon
where
    Sock::ReadHalf: zlink::connection::socket::FetchPeerCredentials,
{
    #[zlink(interface = "com.system76.CosmicSettings.Audio", return_fds)]
    pub async fn recv_events(&mut self) -> (Result<(), audio::Error>, Vec<OwnedFd>) {
        let mut fds = Vec::new();
        let mut this = self.0.lock().await;
        let reply = match this.audio_server.recv_events().await {
            Ok(fd) => {
                fds.push(fd);
                Ok(())
            }
            Err(why) => Err(why),
        };

        (reply, fds)
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn default_sink(&mut self) -> Result<audio::Node, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .default_sink()
            .await
            .ok_or(audio::Error::NoActiveSink)
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn default_source(&mut self) -> Result<audio::Node, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .default_sink()
            .await
            .ok_or(audio::Error::NoActiveSource)
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn sink_mute_toggle(&mut self) -> Result<audio::Mute, audio::Error> {
        self.0.lock().await.audio_server.sink_mute_toggle().await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn sink_volume_lower(&mut self, step: u32) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .sink_volume_lower(step)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn sink_volume_raise(&mut self, step: u32) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .sink_volume_raise(step)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn source_mute_toggle(&mut self) -> Result<audio::Mute, audio::Error> {
        self.0.lock().await.audio_server.source_mute_toggle().await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn source_volume_lower(&mut self, step: u32) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .source_volume_lower(step)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn source_volume_raise(&mut self, step: u32) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .source_volume_raise(step)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn set_default(&mut self, node_id: u32, save: bool) -> Result<(), audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_default(node_id, save)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn set_profile(
        &mut self,
        device_id: u32,
        profile_index: u32,
        save: bool,
    ) -> Result<(), audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_profile(device_id, profile_index, save)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn set_sink_volume(&mut self, volume: u32) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_sink_volume(volume)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn set_source_volume(&mut self, volume: u32) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_source_volume(volume)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn set_node_mute(
        &mut self,
        node_id: u32,
        mute: bool,
    ) -> Result<audio::Mute, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_node_mute(node_id, mute)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn set_node_volume(
        &mut self,
        node_id: u32,
        volume: u32,
    ) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_node_volume(node_id, volume)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio")]
    pub async fn set_node_volume_balance(
        &mut self,
        node_id: u32,
        balance: Option<f32>,
    ) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_node_volume_balance(node_id, balance)
            .await
    }
}

pub struct DaemonInner {
    pub audio_server: audio_server::Server,
}
