// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};
use zlink::{ReplyError, introspect};

#[derive(Debug, PartialEq, ReplyError, introspect::ReplyError)]
#[zlink(interface = "com.system76.CosmicSettings.Audio")]
pub enum Error {
    IO { code: Option<i32>, why: String },
    ChannelSend,
    NoActiveSink,
    NoActiveSource,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IO { code, why } => write!(f, "I/O error (code {code:?}): {why}"),
            Error::ChannelSend => f.write_str("internal error: channel send failed"),
            Error::NoActiveSink => f.write_str("no active sink device to apply operation to"),
            Error::NoActiveSource => f.write_str("no active source to apply operation to"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Node {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Profile {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mute {
    pub id: u32,
    pub mute: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Volume {
    pub id: u32,
    pub volume: u32,
    pub balance: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Event {
    /// The active profile of a device may have changed
    ActiveProfile(u32, ProfileInfo),
    /// The active route of a device may have changed
    ActiveRoute(u32, u32, RouteInfo),
    /// Default sink change
    DefaultSink(u32),
    /// Default source change
    DefaultSource(u32),
    /// Add a device
    Device(u32, DeviceInfo),
    /// Mono audio state has changed.
    MonoAudio(bool),
    /// Add a node
    Node(u32, NodeInfo),
    /// Mute status of a node changed.
    NodeMute(u32, bool),
    /// Volume of a node changed.
    NodeVolume(u32, u32, Option<f32>),
    /// A profile on a device may have changed
    Profile(u32, u32, ProfileInfo),
    /// A route on a device may have changed
    Route(u32, u32, RouteInfo),
    /// Remove a device.
    RemoveDevice(u32),
    /// Remove a node.
    RemoveNode(u32),
    /// Serde will fallback to this if a new enum variant was added that is unknown to the client
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub description: String,
    pub icon_name: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ProfileInfo {
    pub name: String,
    pub description: String,
    pub index: u32,
    pub priority: u32,
    pub availability: Availability,
    pub classes: Vec<ProfileClass>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ProfileClass {
    AudioSink { card_profile_devices: Vec<i32> },
    AudioSource { card_profile_devices: Vec<i32> },
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct RouteInfo {
    pub name: String,
    pub description: String,
    pub port_type: String,
    pub icon_name: String,
    pub devices: Vec<u32>,
    pub profiles: Vec<u32>,
    pub index: u32,
    pub priority: u32,
    pub device: u32,
    pub profile: u32,
    pub availability: Availability,
    pub is_sink: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeInfo {
    pub name: String,
    pub description: String,
    pub device_profile_description: String,
    pub device_id: Option<u32>,
    pub card_profile_device: Option<u32>,
    pub is_sink: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeProperties {
    pub volume: u32,
    pub balance: Option<f32>,
    pub mute: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceProfiles {
    pub device_name: String,
    pub active_profile: Option<usize>,
    pub profile_indexes: Vec<u32>,
    pub profile_descriptions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceRoutes {
    pub active_route: Option<usize>,
    pub route_indexes: Vec<u32>,
    pub route_descriptions: Vec<String>,
}

#[derive(Copy, Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize, Serialize)]
pub enum Availability {
    No,
    Yes,
    #[default]
    #[serde(other)]
    Unknown,
}
