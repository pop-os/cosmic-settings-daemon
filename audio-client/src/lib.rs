// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

#[cfg(feature = "codec")]
pub mod codec;

pub use zlink;
use zlink::Connection;

pub use cosmic_settings_audio_core::*;
use std::{os::fd::OwnedFd, path::PathBuf};

pub async fn connect() -> zlink::Result<Client> {
    zlink::unix::connect(socket_path())
        .await
        .map(|conn| Client { conn })
}

#[derive(Debug)]
pub struct Client {
    pub conn: Connection<zlink::unix::Stream>,
}

impl Client {
    #[cfg(feature = "codec")]
    pub async fn recv_events(
        &mut self,
    ) -> zlink::Result<
        Result<
            impl futures_util::Stream<Item = Result<Event, codec::Error>> + Sync + Send + 'static,
            Error,
        >,
    > {
        self.conn
            .recv_events()
            .await
            .map(|(result, mut fds)| match result {
                Ok(()) => Ok(tokio_util::codec::FramedRead::new(
                    tokio::net::unix::pipe::Receiver::from_owned_fd(fds.swap_remove(0)).unwrap(),
                    codec::EventDecoder,
                )),
                Err(why) => Err(why),
            })
    }
}

pub fn socket_path() -> PathBuf {
    dirs::runtime_dir()
        .expect("runtime dir required by varlink service")
        .join("com.system76.CosmicSettings")
}

#[zlink::proxy("com.system76.CosmicSettings.Audio")]
pub trait CosmicAudioProxy {
    /// Request to listen to audio events through the returned `OwnedFd`.
    #[zlink(return_fds)]
    async fn recv_events(&mut self) -> zlink::Result<(Result<(), Error>, Vec<OwnedFd>)>;

    async fn default_sink(&mut self) -> zlink::Result<Result<Node, Error>>;

    async fn default_source(&mut self) -> zlink::Result<Result<Node, Error>>;

    async fn sink_mute_toggle(&mut self) -> zlink::Result<Result<Mute, Error>>;

    async fn sink_volume_lower(&mut self, step: u32) -> zlink::Result<Result<Volume, Error>>;

    async fn sink_volume_raise(&mut self, step: u32) -> zlink::Result<Result<Volume, Error>>;

    async fn source_mute_toggle(&mut self) -> zlink::Result<Result<Mute, Error>>;

    async fn source_volume_lower(&mut self, step: u32) -> zlink::Result<Result<Volume, Error>>;

    async fn source_volume_raise(&mut self, step: u32) -> zlink::Result<Result<Volume, Error>>;

    async fn set_default(&mut self, node_id: u32, save: bool) -> zlink::Result<Result<(), Error>>;

    /// Change the active profile of an audio device; changing which routes are active.
    async fn set_profile(
        &mut self,
        device_id: u32,
        profile_index: u32,
        save: bool,
    ) -> zlink::Result<Result<(), Error>>;

    /// Apply a volume to the default sink node.
    async fn set_sink_volume(&mut self, volume: u32) -> zlink::Result<Result<Volume, Error>>;

    /// Apply a volume to the default source node.
    async fn set_source_volume(&mut self, volume: u32) -> zlink::Result<Result<Volume, Error>>;

    /// Apply a mute state to a node by its ID.
    async fn set_node_mute(
        &mut self,
        node_id: u32,
        mute: bool,
    ) -> zlink::Result<Result<Mute, Error>>;

    /// Apply a volume to a node by its ID.
    async fn set_node_volume(
        &mut self,
        node_id: u32,
        volume: u32,
    ) -> zlink::Result<Result<Volume, Error>>;

    /// Set the balance of a node by its ID. 1.
    async fn set_node_volume_balance(
        &mut self,
        node_id: u32,
        balance: Option<f32>,
    ) -> zlink::Result<Result<Volume, Error>>;
}
