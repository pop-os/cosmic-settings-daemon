// Copyright 2025 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use crate::Channel;
use crate::spa_utils::array_from_pod;
use float_cmp::{ApproxEq, F32Margin};
use libspa::pod::Pod;
use libspa::utils::Id;
use pipewire::node::{NodeInfoRef, NodeState};
use std::ffi::c_float;

/// Node information
#[must_use]
#[derive(Clone, Debug)]
pub struct Node {
    pub object_id: u32,
    pub audio_channels: u32,
    pub audio_position: String,
    pub card_profile_device: Option<u32>,
    pub description: String,
    pub device_id: Option<u32>,
    pub device_profile_description: String,
    pub device_profile_pro: bool,
    pub icon_name: String,
    pub media_class: MediaClass,
    pub node_name: String,
    pub state: State,
    /// `application.name` — human-readable app name, set on stream nodes.
    pub application_name: Option<String>,
    /// `application.process.binary` — binary name, set on stream nodes.
    pub application_binary: Option<String>,
    /// `application.icon-name` — preferred icon name, set on stream nodes.
    pub application_icon_name: Option<String>,
    /// `media.name` — current media title (e.g. track/tab title), set on stream nodes.
    pub media_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Running,
    Creating,
    Suspended,
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaClass {
    Source,
    Sink,
    /// `Stream/Output/Audio` — a per-application playback stream (sink-input equivalent).
    StreamOutput,
}

impl Node {
    /// Attains process info from a pipewire info node.
    #[must_use]
    pub fn from_node(info: &NodeInfoRef) -> Option<Self> {
        let props = info.props()?;

        let mut audio_channels = 1;
        let mut audio_position = String::new();
        let mut card_profile_device = None;
        let mut device_id = None;
        let mut device_profile_description: &str = "";
        let mut device_profile_pro = false;
        let mut icon_name = String::new();
        let mut media_class = None;
        let mut node_description: &str = "";
        let mut node_name = String::new();
        let mut object_id = None;
        let mut application_name = None;
        let mut application_binary = None;
        let mut application_icon_name = None;
        let mut application_process_id: Option<u32> = None;
        let mut media_name = None;

        for (entry, value) in props.iter() {
            match entry {
                "device.id" => device_id = value.parse::<u32>().ok(),
                "object.id" => object_id = Some(value.parse::<u32>().ok()?),

                // 2
                "audio.channels" => audio_channels = value.parse::<u32>().unwrap_or(1),

                // FL,FR
                "audio.position" => audio_position = value.to_owned(),

                // 0
                "card.profile.device" => card_profile_device = Some(value.parse::<u32>().ok()?),

                // Analog Stereo (ALSA only)
                "device.profile.description" => {
                    device_profile_description = value;
                }

                // false
                "device.profile.pro" => device_profile_pro = value == "true",

                // audio-card-analog
                "device.icon-name" => icon_name = value.to_owned(),

                "media.class" => {
                    media_class = Some(match value {
                        "Audio/Sink" => MediaClass::Sink,
                        "Audio/Source" => MediaClass::Source,
                        "Stream/Output/Audio" => MediaClass::StreamOutput,
                        _ => return None,
                    })
                }

                // alsa_input.pci-0000_66_00.6.analog-stereo
                "node.name" => node_name = value.to_owned(),

                // Family 17h/19h HD Audio Controller Analog Stereo
                "node.description" => node_description = value,

                // Firefox, Spotify (set on stream nodes)
                "application.name" => application_name = Some(value.to_owned()),

                // firefox, spotify (set on stream nodes)
                "application.process.binary" => application_binary = Some(value.to_owned()),

                // Present on streams created by a user application, but absent
                // from PipeWire's own loopback and combined-output nodes.
                "application.process.id" => application_process_id = value.parse().ok(),

                // application-set icon name, preferred over icon-by-binary-name lookup
                "application.icon-name" => application_icon_name = Some(value.to_owned()),

                // current track/tab title (set on stream nodes)
                "media.name" => media_name = Some(value.to_owned()),

                _ => (),
            }
        }

        // Playback controls should enumerate user applications, not PipeWire's
        // internal loopback or combined-output streams.
        if matches!(media_class, Some(MediaClass::StreamOutput)) && application_process_id.is_none()
        {
            return None;
        }

        let device = Node {
            object_id: object_id?,
            device_id,
            card_profile_device,
            media_class: media_class?,
            description: if device_profile_description.is_empty() {
                node_description.to_owned()
            } else {
                let device_name = node_description
                    .strip_suffix(device_profile_description)
                    .unwrap_or(node_description)
                    .trim_ascii_end();
                device_name.to_owned()
            },
            device_profile_description: device_profile_description.to_owned(),
            device_profile_pro,
            icon_name,
            audio_channels,
            audio_position,
            node_name,
            state: match info.state() {
                NodeState::Idle => State::Idle,
                NodeState::Running => State::Running,
                NodeState::Creating => State::Creating,
                NodeState::Suspended => State::Suspended,
                NodeState::Error(why) => State::Error(why.to_owned()),
            },
            application_name,
            application_binary,
            application_icon_name,
            media_name,
        };

        Some(device)
    }
}

#[derive(Clone, Debug, Default)]
pub struct NodeProps {
    pub mute: Option<bool>,
    pub monitor_mute: Option<bool>,
    pub channel_map: Option<Vec<Channel>>,
    pub channel_volumes: Option<Vec<f32>>,
}

impl std::cmp::PartialEq for NodeProps {
    fn eq(&self, other: &Self) -> bool {
        self.mute == other.mute
            && self.monitor_mute == other.mute
            && self.channel_map == other.channel_map
            && match (
                self.channel_volumes.as_deref(),
                other.channel_volumes.as_deref(),
            ) {
                (None, None) => true,
                (Some(_), None) => true,
                (None, Some(_)) => true,
                (Some(prev), Some(other)) => {
                    prev.len() == other.len()
                        && prev
                            .iter()
                            .zip(other.iter())
                            .all(|(&p, &o)| p.approx_eq(o, F32Margin::default()))
                }
            }
    }
}

impl std::cmp::Eq for NodeProps {}

impl NodeProps {
    pub fn from_pod(pod: &Pod) -> Option<Self> {
        let props = pod.as_object().ok()?;
        let props = NodeProps {
            mute: props
                .find_prop(Id(libspa_sys::SPA_PROP_mute))
                .and_then(|prop| prop.value().get_bool().ok()),
            monitor_mute: props
                .find_prop(Id(libspa_sys::SPA_PROP_monitorMute))
                .and_then(|prop| prop.value().get_bool().ok()),
            channel_map: props
                .find_prop(Id(libspa_sys::SPA_PROP_channelMap))
                .and_then(|prop| unsafe { array_from_pod::<Channel>(prop.value()) }),
            channel_volumes: props
                .find_prop(Id(libspa_sys::SPA_PROP_channelVolumes))
                .and_then(|prop| unsafe { array_from_pod::<c_float>(prop.value()) }),
        };

        if props.mute.is_none()
            && props.monitor_mute.is_none()
            && props.channel_map.is_none()
            && props.channel_volumes.is_none()
        {
            None
        } else {
            Some(props)
        }
    }

    pub fn merge(&mut self, other: NodeProps) {
        if other.mute.is_some() {
            self.mute = other.mute
        }

        if other.monitor_mute.is_some() {
            self.monitor_mute = other.monitor_mute;
        }

        if other.channel_map.is_some() {
            self.channel_map = other.channel_map;
        }

        if other.channel_volumes.is_some() {
            self.channel_volumes = other.channel_volumes;
        }
    }
}
