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
    InvalidPlayback,
    InvalidSink,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IO { code, why } => write!(f, "I/O error (code {code:?}): {why}"),
            Error::ChannelSend => f.write_str("internal error: channel send failed"),
            Error::NoActiveSink => f.write_str("no active sink device to apply operation to"),
            Error::NoActiveSource => f.write_str("no active source to apply operation to"),
            Error::InvalidPlayback => f.write_str("node is not an audio playback stream"),
            Error::InvalidSink => f.write_str("node is not an audio sink"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, zlink::introspect::Type)]
pub struct Node {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, zlink::introspect::Type)]
pub struct Profile {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, zlink::introspect::Type)]
pub struct Mute {
    pub id: u32,
    pub mute: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, zlink::introspect::Type)]
pub struct Volume {
    pub id: u32,
    pub volume: u32,
    pub balance: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[deprecated(note = "use Event with the v2 event subscription instead")]
pub enum EventV1 {
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
#[serde(tag = "t", content = "v")]
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
    /// Add an audio playback stream.
    Playback(u32, PlaybackInfo),
    /// The sink explicitly targeted by a playback stream, or `None` to follow the default sink.
    PlaybackTarget(u32, Option<u32>),
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

macro_rules! convert_event {
    ($event:expr, $source:ident, $target:ident) => {
        match $event {
            $source::ActiveProfile(id, info) => $target::ActiveProfile(id, info),
            $source::ActiveRoute(id, index, info) => $target::ActiveRoute(id, index, info),
            $source::DefaultSink(id) => $target::DefaultSink(id),
            $source::DefaultSource(id) => $target::DefaultSource(id),
            $source::Device(id, info) => $target::Device(id, info),
            $source::MonoAudio(enabled) => $target::MonoAudio(enabled),
            $source::Node(id, info) => $target::Node(id, info),
            $source::NodeMute(id, mute) => $target::NodeMute(id, mute),
            $source::NodeVolume(id, volume, balance) => $target::NodeVolume(id, volume, balance),
            $source::Profile(id, index, info) => $target::Profile(id, index, info),
            $source::Route(id, index, info) => $target::Route(id, index, info),
            $source::RemoveDevice(id) => $target::RemoveDevice(id),
            $source::RemoveNode(id) => $target::RemoveNode(id),
            $source::Unknown => $target::Unknown,
        }
    };
}

#[allow(deprecated)]
impl From<EventV1> for Event {
    fn from(event: EventV1) -> Self {
        convert_event!(event, EventV1, Event)
    }
}

#[allow(deprecated)]
impl From<Event> for EventV1 {
    fn from(event: Event) -> Self {
        match event {
            Event::ActiveProfile(id, info) => Self::ActiveProfile(id, info),
            Event::ActiveRoute(id, index, info) => Self::ActiveRoute(id, index, info),
            Event::DefaultSink(id) => Self::DefaultSink(id),
            Event::DefaultSource(id) => Self::DefaultSource(id),
            Event::Device(id, info) => Self::Device(id, info),
            Event::MonoAudio(enabled) => Self::MonoAudio(enabled),
            Event::Node(id, info) => Self::Node(id, info),
            Event::Playback(..) | Event::PlaybackTarget(..) => Self::Unknown,
            Event::NodeMute(id, mute) => Self::NodeMute(id, mute),
            Event::NodeVolume(id, volume, balance) => Self::NodeVolume(id, volume, balance),
            Event::Profile(id, index, info) => Self::Profile(id, index, info),
            Event::Route(id, index, info) => Self::Route(id, index, info),
            Event::RemoveDevice(id) => Self::RemoveDevice(id),
            Event::RemoveNode(id) => Self::RemoveNode(id),
            Event::Unknown => Self::Unknown,
        }
    }
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
pub struct PlaybackInfo {
    pub application_id: Option<String>,
    pub application_name: Option<String>,
    pub icon_name: Option<String>,
    pub media_name: Option<String>,
    pub node_name: String,
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

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::{Event, EventV1};

    #[test]
    fn event_wire_formats_are_versioned() {
        assert_eq!(
            ron::to_string(&EventV1::DefaultSink(42)).unwrap(),
            "DefaultSink(42)"
        );
        assert_eq!(
            ron::to_string(&Event::DefaultSink(42)).unwrap(),
            "(t:DefaultSink,v:42)"
        );
    }

    #[test]
    fn events_convert_between_versions() {
        let v2 = Event::from(EventV1::NodeVolume(42, 75, Some(0.25)));
        let Event::NodeVolume(id, volume, balance) = v2 else {
            panic!("unexpected event variant");
        };
        assert_eq!((id, volume, balance), (42, 75, Some(0.25)));
    }
}
