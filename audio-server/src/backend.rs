// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic_config::ConfigSet;
use cosmic_pipewire::{self as pipewire, Direction, NodeProps, PortType, ProfileClass};
use cosmic_settings_audio_core::Event;
use cosmic_settings_daemon_config::{CosmicSettingsDaemonConfig, CosmicSettingsDaemonState};
use futures_util::{SinkExt, StreamExt};
use intmap::IntMap;
use pipewire::Availability;
use std::{
    process::Stdio, sync::{Arc, OnceLock}, time::Instant
};
use tokio::net::unix::pipe;
use tokio_util::codec::FramedWrite;

pub type DeviceId = u32;
pub type NodeId = u32;
pub type ProfileId = i32;
pub type RouteId = u32;

pub static INITIATED_TIME: OnceLock<std::time::Instant> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeviceProfileKind {
    Headphone,
    Headset,
}

#[derive(Debug, Default)]
struct HeadsetProfiles {
    /// High-fidelity audio quality
    headphone: Option<HeadsetProfile>,
    /// Optimized for microphones
    headset: Option<HeadsetProfile>,
}

#[derive(Clone, Copy, Debug)]
pub struct HeadsetProfile {
    priority: u32,
    card_profile_device: u32,
    index: u32,
    route: u32,
}

pub struct Model {
    /// Varlink clients that are actively listening for events through Unix pipes.
    subscribers: Arc<tokio::sync::Mutex<Vec<pipe::Sender>>>,

    /// The sending half of a channel for sending requests to cosmic-pipewire.
    pipewire_sender: Option<pipewire::Sender>,

    /** Settings daemon state */
    pub daemon_config_context: cosmic_config::Config,
    pub daemon_state_context: cosmic_config::Config,

    /** Node object information */

    /// Maps node IDs to their corresponding device IDs (if they have one).
    node_devices: IntMap<NodeId, DeviceId>,
    /// Information about nodes that are shared with subscribed varlink clients.
    node_info: IntMap<NodeId, cosmic_settings_audio_core::NodeInfo>,
    /// Track mute status of nodes.
    pub node_mute: IntMap<NodeId, bool>,
    /// Track volume and volume balance of nodes.
    pub node_volumes: IntMap<NodeId, (u32, Option<f32>)>,
    /// Node IDs for sinks
    sink_node_ids: Vec<NodeId>,
    /// Node IDs for sources
    source_node_ids: Vec<NodeId>,

    /** Device object information */

    /// Information about devices that are shared with subscribed varlink clients.
    device_info: IntMap<DeviceId, cosmic_settings_audio_core::DeviceInfo>,
    /// All known device profiles that devices can be assigned.
    device_profiles: IntMap<DeviceId, Vec<pipewire::Profile>>,
    /// Tracking headset and headphone profiles
    device_headset_profiles: IntMap<DeviceId, HeadsetProfiles>,
    /// Check if a newly-added device requires an OSD dialog.
    device_headset_check: IntMap<DeviceId, Option<i32>>,
    /// Track which profile is currently assigned to each device.
    active_profiles: IntMap<DeviceId, pipewire::Profile>,
    /// Track which routes are currently active on each device.
    active_routes: IntMap<DeviceId, Vec<pipewire::Route>>,
    /// All known routes that devices can input/output to.
    device_routes: IntMap<DeviceId, Vec<pipewire::Route>>,
    /// Set when a headset/headphone profile is being applied.
    applying_device_profile: Option<(DeviceProfileKind, DeviceId)>,

    /** Active sink state */

    /// Node ID of active sink device.
    pub active_sink_node: Option<NodeId>,
    /// Device identifier of the default sink.
    pub active_sink_node_name: String,
    /// The default sink's node name was found but the node ID is not yet known.
    pub active_sink_not_found: bool,
    /// The active sink's current volume balance.
    pub sink_balance: Option<f32>,
    /// The active sink's current volume.
    pub sink_volume: u32,
    /// The active sink's mute status.
    pub sink_mute: bool,

    /** Active source state */

    /// Node ID of active source device.
    pub active_source_node: Option<NodeId>,
    /// Node identifier of the default source.
    pub active_source_node_name: String,
    /// The default source's node name was found but the node ID is not yet known.
    pub active_source_not_found: bool,
    /// The active source's current volume.
    pub source_volume: u32,
    /// The active source's mute status.
    pub source_mute: bool,
}

impl Model {
    pub async fn new() -> Self {
        // Create if missing before creating a cosmic-config context.
        if let Some(state_dir) = dirs::state_dir() {
            _ = std::fs::create_dir_all(&state_dir);
        }

        let daemon_config_context = CosmicSettingsDaemonConfig::config()
            .expect("failed to create context for CosmicSettingsDaemonConfig");

        let daemon_state_context = CosmicSettingsDaemonState::config()
            .expect("failed to create context for CosmicSettingsDaemonState");

        _ = INITIATED_TIME.set(std::time::Instant::now());

        Self {
            daemon_config_context,
            daemon_state_context,
            subscribers: Default::default(),
            pipewire_sender: Default::default(),
            node_devices: Default::default(),
            node_info: Default::default(),
            node_mute: Default::default(),
            node_volumes: Default::default(),
            sink_node_ids: Default::default(),
            source_node_ids: Default::default(),
            device_info: Default::default(),
            device_profiles: Default::default(),
            active_profiles: Default::default(),
            active_routes: Default::default(),
            device_headset_check: Default::default(),
            device_headset_profiles: Default::default(),
            device_routes: Default::default(),
            active_sink_node: Default::default(),
            active_sink_node_name: Default::default(),
            active_sink_not_found: Default::default(),
            sink_balance: Default::default(),
            sink_volume: Default::default(),
            sink_mute: Default::default(),
            active_source_node: Default::default(),
            active_source_node_name: Default::default(),
            active_source_not_found: Default::default(),
            source_volume: Default::default(),
            source_mute: Default::default(),
            applying_device_profile: None,
        }
    }

    pub fn clear(&mut self) {
        if let Some(pipewire) = self.pipewire_sender.take() {
            _ = pipewire.send(pipewire::Request::Quit);
        }
    }

    /// Send events to subscribed clients.
    pub async fn emit_event(&self, event: Event) {
        let subscribers = self.subscribers.clone();
        _ = tokio::task::spawn_local(async move {
            let Ok(serialized) = ron::ser::to_string(&event) else {
                return;
            };

            let serialized_bytes = serialized.as_bytes();

            // Concurrently write event to subscribers and discard those who fail.
            let mut subscribers_guard = subscribers.lock().await;
            let subscribers: Vec<pipe::Sender> = std::mem::take(&mut subscribers_guard);
            *subscribers_guard = subscribers
                .into_iter()
                .map(move |subscriber| async move {
                    let mut writer = FramedWrite::new(subscriber, crate::codec::EventCodec);
                    if writer.send(serialized_bytes).await.is_ok() {
                        Some(writer.into_inner())
                    } else {
                        None
                    }
                })
                .collect::<futures_util::stream::FuturesUnordered<_>>()
                .fold(Vec::new(), |mut retained, result| async move {
                    if let Some(subscriber) = result {
                        retained.push(subscriber);
                    }

                    retained
                })
                .await;
        })
        .await;
    }

    /// Send a message to the pipewire-rs thread.
    pub fn pipewire_send(&self, request: pipewire::Request) {
        if let Some(pipewire) = self.pipewire_sender.as_ref() {
            _ = pipewire.send(request);
        }
    }

    pub fn set_default(&mut self, node_id: NodeId, save: bool) {
        if self.sink_node_ids.contains(&node_id) {
            self.set_default_sink_node_id(node_id, save)
        } else if self.source_node_ids.contains(&node_id) {
            self.set_default_source_node_id(node_id, save)
        } else {
            tracing::warn!(node_id, "set_default node not found");
        }
    }

    /// Selects the headphone profile of a device.
    pub async fn select_headphone_profile(&mut self, device_id: u32) {
        tracing::info!(
            target: "audio-backend",
            device_id,
            "selecting headphone profile"
        );
        if let Some(headset_profiles) = self.device_headset_profiles.get(device_id)
            && let Some(profile) = headset_profiles.headphone
        {
            // If the profile is already active, skip the profile and set the route.
            if let Some(current_profile) = self.active_profiles.get(device_id)
                && current_profile.index == profile.index as i32
            {
                for route in self.device_routes.get(device_id).into_iter().flatten() {
                    if route.index == profile.route as i32 {
                        if matches!(route.available, Availability::Yes | Availability::Unknown) {
                            self.pipewire_send(pipewire::Request::SetRoute(
                                device_id,
                                profile.card_profile_device,
                                profile.route,
                                true,
                            ));
                        }
                    } else if matches!(route.port_type, PortType::Mic)
                        && matches!(route.available, Availability::Yes | Availability::Unknown)
                    {
                        self.pipewire_send(pipewire::Request::SetRoute(
                            device_id,
                            profile.card_profile_device,
                            profile.route,
                            true,
                        ));
                    }
                }

                return;
            }

            self.applying_device_profile = Some((DeviceProfileKind::Headphone, device_id));
            self.pipewire_send(pipewire::Request::SetProfile(
                device_id,
                profile.index,
                true,
            ));
        }
    }

    /// Selects the headset profile of a device.
    pub async fn select_headset_profile(&mut self, device_id: u32) {
        tracing::info!(
            target: "audio-backend",
            device_id,
            "selecting headset profile"
        );

        if let Some(headset_profiles) = self.device_headset_profiles.get(device_id)
            && let Some(profile) = headset_profiles.headset
        {
            // If the profile is already active, skip the profile and set the route.
            if let Some(current_profile) = self.active_profiles.get(device_id)
                && current_profile.index == profile.index as i32
            {
                for route in self.device_routes.get(device_id).into_iter().flatten() {
                    if route.index == profile.route as i32 {
                        if matches!(route.available, Availability::Yes | Availability::Unknown) {
                            self.pipewire_send(pipewire::Request::SetRoute(
                                device_id,
                                profile.card_profile_device,
                                profile.route,
                                true,
                            ));
                        }
                        break;
                    }
                }

                return;
            }

            self.applying_device_profile = Some((DeviceProfileKind::Headset, device_id));
            self.pipewire_send(pipewire::Request::SetProfile(
                device_id,
                profile.index,
                true,
            ));
        }
    }

    /// Sets and applies a profile to a device with wpctl.
    ///
    /// Requires using the device ID rather than a node ID.
    pub async fn set_profile(&mut self, device_id: DeviceId, index: u32, save: bool) {
        self.pipewire_send(pipewire::Request::SetProfile(device_id, index, save));
    }

    /// Changes the active route of a device.
    ///
    /// Requires using the device ID rather than a node ID.
    pub async fn set_route(
        &mut self,
        device_id: DeviceId,
        card_profile_device: u32,
        route_index: u32,
        save: bool,
    ) {
        tracing::info!(target: "audio-backend", device_id, card_profile_device, route_index, save, "set_route");
        self.pipewire_send(pipewire::Request::SetRoute(
            device_id,
            card_profile_device,
            route_index,
            save,
        ));
    }

    pub fn set_default_sink_node_id(&mut self, node_id: NodeId, save: bool) {
        tracing::debug!(target: "audio-backend", "set default sink node {node_id}");

        // Use pactl if the node is not a device node.
        let virtual_sink_name: Option<String> =
            if let Some(device_id) = self.node_devices.get(node_id).cloned() {
                // Get route index of the selected node and apply it to the device.
                if let Some(card_profile_device) = self
                    .node_info
                    .get(node_id)
                    .and_then(|n| n.card_profile_device)
                    && let Some(routes) = self.device_routes.get(device_id)
                    && let Some(route) = routes
                        .iter()
                        .find(|r| r.device as u32 == card_profile_device)
                {
                    self.pipewire_send(pipewire::Request::SetRoute(
                        device_id,
                        card_profile_device,
                        route.device as u32,
                        save,
                    ));
                }

                None
            } else {
                self.node_info.get(node_id).map(|node| node.name.clone())
            };

        tokio::task::spawn(async move {
            if let Some(node_name) = virtual_sink_name {
                pactl_set_default_sink(&node_name).await
            } else {
                set_default(node_id).await
            }
        });
    }

    pub fn set_default_source_node_id(&mut self, node_id: NodeId, save: bool) {
        tracing::debug!(target: "audio-backend", "set default source node {node_id}");

        // Use pactl if the node is not a device node.
        let virtual_source_name: Option<String> = if let Some(device_id) =
            self.node_devices.get(node_id).cloned()
        {
            // Get route index of the selected node and apply it to the device.
            if let Some(card_profile_device) = self
                .node_info
                .get(node_id)
                .and_then(|n| n.card_profile_device)
                && let Some(routes) = self.device_routes.get(device_id)
                && let Some(route) = routes
                    .iter()
                    .find(|r| r.device as u32 == card_profile_device)
            {
                tracing::debug!(target: "audio-backend", "set route of {device_id} to {card_profile_device} {}", route.device);
                self.pipewire_send(pipewire::Request::SetRoute(
                    device_id,
                    card_profile_device,
                    route.device as u32,
                    save,
                ));
            }

            None
        } else {
            self.node_info.get(node_id).map(|node| node.name.clone())
        };

        tokio::task::spawn(async move {
            if let Some(node_name) = virtual_source_name {
                pactl_set_default_source(&node_name).await
            } else {
                set_default(node_id).await
            }
        });
    }

    pub async fn update(&mut self, message: Message) {
        match message {
            Message::Server(events) => {
                for event in Arc::into_inner(events).into_iter().flatten() {
                    self.pipewire_update(event).await;
                }
            }

            Message::Subscribe(socket_path) => {
                tracing::debug!(target: "audio-backend", "subscribing client");
                let writer = Arc::into_inner(socket_path).unwrap();
                let subscribers = self.subscribers.clone();

                let devices = self.device_info.clone();
                let nodes = self.node_info.clone();
                let profiles = self.device_profiles.clone();
                let routes = self.device_routes.clone();
                let active_profiles = self.active_profiles.clone();
                let active_routes = self.active_routes.clone();
                let default_sink = self.active_sink_node;
                let default_source = self.active_source_node;
                let node_volumes = self.node_volumes.clone();
                let node_mute = self.node_mute.clone();

                // Emit current state to the newly-subscribed client before adding it to the subscriber queue.
                tokio::task::spawn(async move {
                    let mut subscribers = subscribers.lock().await;
                    let mut writer = FramedWrite::new(writer, crate::codec::EventCodec);

                    let current_events = devices
                        .into_iter()
                        .map(|(device_id, device)| Event::Device(device_id, device))
                        .chain(routes.into_iter().flat_map(|(device_id, routes)| {
                            routes.into_iter().enumerate().map(move |(index, route)| {
                                Event::Route(
                                    device_id,
                                    index as u32,
                                    pipewire_route_to_cosmic(&route),
                                )
                            })
                        }))
                        .chain(active_routes.into_iter().flat_map(|(device_id, routes)| {
                            routes.into_iter().enumerate().map(move |(index, route)| {
                                Event::ActiveRoute(
                                    device_id,
                                    index as u32,
                                    pipewire_route_to_cosmic(&route),
                                )
                            })
                        }))
                        .chain(profiles.into_iter().flat_map(|(device_id, profiles)| {
                            profiles
                                .into_iter()
                                .enumerate()
                                .map(move |(index, profile)| {
                                    Event::Profile(
                                        device_id,
                                        index as u32,
                                        pipewire_profile_to_cosmic(&profile),
                                    )
                                })
                        }))
                        .chain(active_profiles.into_iter().map(|(device_id, profile)| {
                            Event::ActiveProfile(device_id, pipewire_profile_to_cosmic(&profile))
                        }))
                        .chain(
                            nodes
                                .into_iter()
                                .map(|(node_id, node)| Event::Node(node_id, node)),
                        )
                        .chain(default_sink.into_iter().map(Event::DefaultSink))
                        .chain(default_source.into_iter().map(Event::DefaultSource))
                        .chain(
                            node_volumes
                                .into_iter()
                                .map(|(id, (vol, bal))| Event::NodeVolume(id, vol, bal)),
                        )
                        .chain(
                            node_mute
                                .into_iter()
                                .map(|(id, mute)| Event::NodeMute(id, mute)),
                        )
                        .filter_map(|event| ron::ser::to_string(&event).ok());

                    for event in current_events {
                        if writer.send(event.as_bytes()).await.is_err() {
                            return;
                        }
                    }

                    subscribers.push(writer.into_inner());
                });
            }

            Message::Init(handle) => {
                if let Some(handle) = Arc::into_inner(handle) {
                    self.pipewire_sender = Some(handle);
                }
            }
        }
    }

    async fn pipewire_update(&mut self, event: pipewire::Event) {
        match event {
            pipewire::Event::NodeProperties(id, props) => {
                self.update_node_properties(id, props).await;
            }

            pipewire::Event::ActiveProfile(id, profile) => {
                tracing::info!(
                    target: "audio-backend",
                    device = id,
                    profile = profile.index,
                    name = profile.name,
                    "active profile update"
                );

                self.emit_event(Event::ActiveProfile(
                    id,
                    pipewire_profile_to_cosmic(&profile),
                ))
                .await;

                // Apply a headphone or headset route after its profile has been assigned.
                // But first check if the route is available before attempting to set it.
                if let Some((device_profile_kind, device_id)) = self.applying_device_profile
                    && device_id == id
                {
                    self.applying_device_profile = None;

                    let expected_profile = match device_profile_kind {
                        DeviceProfileKind::Headphone => self
                            .device_headset_profiles
                            .get(device_id)
                            .and_then(|p| p.headphone),

                        DeviceProfileKind::Headset => self
                            .device_headset_profiles
                            .get(device_id)
                            .and_then(|p| p.headset),
                    };

                    if let Some(expected_profile) = expected_profile
                        && expected_profile.index == profile.index as u32
                    {
                        for route in self.device_routes.get(device_id).into_iter().flatten() {
                            if route.index == expected_profile.route as i32 {
                                if matches!(
                                    route.available,
                                    Availability::Yes | Availability::Unknown
                                ) {
                                    self.pipewire_send(pipewire::Request::SetRoute(
                                        device_id,
                                        expected_profile.card_profile_device,
                                        expected_profile.route,
                                        true,
                                    ));
                                }

                                if let DeviceProfileKind::Headset = device_profile_kind {
                                    break;
                                }
                            } else if matches!(device_profile_kind, DeviceProfileKind::Headphone)
                                && matches!(route.port_type, PortType::Mic)
                                && matches!(
                                    route.available,
                                    Availability::Yes | Availability::Unknown
                                )
                            {
                                self.pipewire_send(pipewire::Request::SetRoute(
                                    device_id,
                                    expected_profile.card_profile_device,
                                    expected_profile.route,
                                    true,
                                ));
                            }
                        }
                    }
                }

                if let Some(prev_headset_port) = self.device_headset_check.get(id).cloned()
                    && let Some(headset_profiles) = self.device_headset_profiles.get(id)
                    && let Some((headphone_info, headset_info)) =
                        headset_profiles.headphone.zip(headset_profiles.headset)
                    && let Some(profiles) = self.device_profiles.get(id)
                    && let Some(headset_profile) = profiles
                        .iter()
                        .find(|p| p.index as u32 == headset_info.index)
                    && let Some(routes) = self.device_routes.get(id)
                    && let Some(headset_route) = routes.iter().find(|r| {
                        matches!(r.direction, Direction::Input)
                            && matches!(
                                r.port_type,
                                PortType::Headset | PortType::Handset | PortType::Handsfree
                            )
                            && matches!(r.available, Availability::Yes | Availability::Unknown)
                            && r.profiles.contains(&headset_profile.index)
                    })
                {
                    if prev_headset_port == Some(headset_route.index) {
                        tracing::debug!(
                            target: "audio-backend",
                            "detected headset but ignoring due to previous selection"
                        );

                        self.active_profiles.insert(id, profile);
                        return;
                    }

                    self.device_headset_check
                        .insert(id, Some(headset_route.index));

                    // Avoid headset detections if the session has just started.
                    if Instant::now()
                        .duration_since(*INITIATED_TIME.get().unwrap())
                        .as_secs()
                        > 1
                    {
                        tracing::debug!(
                            target: "audio-backend",
                            ?headphone_info,
                            ?headset_info,
                            "cosmic-osd confirm-headphones {id}"
                        );

                        tokio::spawn(async move {
                            _ = tokio::process::Command::new("cosmic-osd")
                                .arg("confirm-headphones")
                                .arg("--device")
                                .arg(numtoa::BaseN::<10>::u32(id).as_str())
                                .status()
                                .await;
                        });
                    } else {
                        tracing::debug!(
                            target: "audio-backend",
                            "detected headset but ignoring for initial session startup"
                        );
                    }
                }

                self.active_profiles.insert(id, profile);
            }

            pipewire::Event::ActiveRoute(id, index, route) => {
                tracing::debug!(
                    target: "audio-backend",
                    "Device {id} active route {}: {} ({})",
                    route.index,
                    route.name,
                    route.description,
                );

                let routes = self.active_routes.entry(id).or_default();
                if routes.len() < index as usize + 1 {
                    let additional = (index as usize + 1) - routes.capacity();
                    routes.reserve_exact(additional);
                    routes.extend(std::iter::repeat_n(pipewire::Route::default(), additional));
                }

                routes[index as usize] = route.clone();
                self.emit_event(Event::ActiveRoute(
                    id,
                    index,
                    pipewire_route_to_cosmic(&route),
                ))
                .await;
            }

            pipewire::Event::AddProfile(id, index, profile) => {
                tracing::debug!(
                    target: "audio-backend",
                    "Device {id} profile {}: {} ({}): {:?}",
                    profile.index,
                    profile.name,
                    profile.description,
                    profile.classes,
                );

                let mut emit = None;
                let profiles = self.device_profiles.entry(id).or_default();
                if let Some(p) = profiles.get(index as usize) {
                    if p.index != profile.index
                        || p.name != profile.name
                        || p.available != profile.available
                    {
                        emit = Some(Event::Profile(
                            id,
                            index,
                            pipewire_profile_to_cosmic(&profile),
                        ));
                    }
                } else if profiles.len() < index as usize + 1 {
                    let additional = (index as usize + 1) - profiles.capacity();
                    profiles.reserve_exact(additional);
                    profiles.extend(std::iter::repeat_n(
                        pipewire::Profile::default(),
                        additional,
                    ));
                    emit = Some(Event::Profile(
                        id,
                        index,
                        pipewire_profile_to_cosmic(&profile),
                    ));
                }

                // Ignore headset profile detection for devices which do not apply.
                if self.device_headset_check.get(id).is_none() {
                    profiles[index as usize] = profile;
                    if let Some(event) = emit {
                        self.emit_event(event).await;
                    }
                    return;
                }

                let headset_profiles = self.device_headset_profiles.entry(id).or_default();

                // An index of 0 implies that we're reloading device's profiles.
                if index == 0 {
                    headset_profiles.headset = None;
                    headset_profiles.headphone = None;
                }

                // Track headphone and headset profiles
                if matches!(profile.available, Availability::Yes | Availability::Unknown) {
                    let classes = profile.classes.iter().map(|c| match c {
                        ProfileClass::AudioSink {
                            card_profile_devices,
                        } => (true, card_profile_devices),
                        ProfileClass::AudioSource {
                            card_profile_devices,
                        } => (false, card_profile_devices),
                    });
                    let routes = self.device_routes.get(id);
                    for (is_sink, devices) in classes {
                        'outer: for device in devices {
                            for route in routes.into_iter().flatten() {
                                if matches!(
                                    route.available,
                                    Availability::Yes | Availability::Unknown
                                ) && route.devices.contains(device)
                                    && route.profiles.contains(&profile.index)
                                {
                                    if route.icon_name.starts_with("audio-headphones") {
                                        let current = &mut headset_profiles.headphone;
                                        if current
                                            .as_ref()
                                            .is_none_or(|c| c.priority < profile.priority as u32)
                                        {
                                            let profile = HeadsetProfile {
                                                priority: profile.priority as u32,
                                                card_profile_device: *device as u32,
                                                index: profile.index as u32,
                                                route: route.index as u32,
                                            };
                                            tracing::debug!(target: "audio-backend", ?profile, "selecting headphone profile candidate");
                                            *current = Some(profile);
                                        }
                                        break 'outer;
                                    } else if !is_sink
                                        && route.icon_name.starts_with("audio-headset")
                                    {
                                        let current = &mut headset_profiles.headset;
                                        if current
                                            .as_ref()
                                            .is_none_or(|c| c.priority < profile.priority as u32)
                                        {
                                            let profile = HeadsetProfile {
                                                priority: profile.priority as u32,
                                                card_profile_device: *device as u32,
                                                index: profile.index as u32,
                                                route: route.index as u32,
                                            };
                                            tracing::debug!(target: "audio-backend", ?profile, "selecting headset profile candidate");
                                            *current = Some(profile);
                                        }

                                        break 'outer;
                                    } else if is_sink
                                        && matches!(route.port_type, PortType::Headphones)
                                    {
                                        let current = &mut headset_profiles.headphone;
                                        if current
                                            .as_ref()
                                            .is_none_or(|c| c.priority <= profile.priority as u32)
                                        {
                                            let profile = HeadsetProfile {
                                                priority: profile.priority as u32,
                                                card_profile_device: *device as u32,
                                                index: profile.index as u32,
                                                route: route.index as u32,
                                            };
                                            tracing::debug!(target: "audio-backend", ?profile, "selecting headphone profile candidate");
                                            *current = Some(profile);
                                        }
                                        break 'outer;
                                    } else if !is_sink
                                        && matches!(
                                            route.port_type,
                                            PortType::Headset
                                                | PortType::Handset
                                                | PortType::Handsfree
                                        )
                                    {
                                        let current = &mut headset_profiles.headset;
                                        if current
                                            .as_ref()
                                            .is_none_or(|c| c.priority < profile.priority as u32)
                                        {
                                            let profile = HeadsetProfile {
                                                priority: profile.priority as u32,
                                                card_profile_device: *device as u32,
                                                index: profile.index as u32,
                                                route: route.index as u32,
                                            };
                                            tracing::debug!(target: "audio-backend", ?profile, "selecting headset profile candidate");
                                            *current = Some(profile);
                                        }

                                        break 'outer;
                                    }
                                }
                            }
                        }
                    }
                }

                profiles[index as usize] = profile;

                if let Some(event) = emit {
                    self.emit_event(event).await;
                }
            }

            pipewire::Event::AddRoute(id, index, route) => {
                tracing::debug!(target: "audio-backend",
                    "Device {} added route {:?} {} ({}), profiles = {:?}, devices = {:?}",
                    id,
                    route.direction,
                    route.name,
                    route.description,
                    route.profiles,
                    route.devices,
                );

                self.emit_event(Event::Route(id, index, pipewire_route_to_cosmic(&route)))
                    .await;

                let routes = self.device_routes.entry(id).or_default();
                if routes.len() < index as usize + 1 {
                    let additional = (index as usize + 1) - routes.capacity();
                    routes.reserve_exact(additional);
                    routes.extend(std::iter::repeat_n(pipewire::Route::default(), additional));
                }

                if matches!(route.available, Availability::No)
                    && let Some(prev_headset_route_index) = self.device_headset_check.get_mut(id)
                    && prev_headset_route_index.is_some_and(|prev_index| prev_index == route.index)
                {
                    *prev_headset_route_index = None;
                }

                routes[index as usize] = route;
            }

            pipewire::Event::AddDevice(device) => {
                tracing::debug!(target: "audio-backend", "Device {} added: {}", device.id, device.description);
                let info = cosmic_settings_audio_core::DeviceInfo {
                    name: device.name,
                    description: device.description,
                    icon_name: device.icon_name,
                };

                // Ignore headset detection for bluetooth devices.
                if !info.name.starts_with("bluez") {
                    self.device_headset_check.insert(device.id, None);
                }

                self.device_info.insert(device.id, info.clone());
                self.emit_event(Event::Device(device.id, info)).await;
            }

            pipewire::Event::AddNode(node) => {
                tracing::debug!(target: "audio-backend", "Node {} added: {}", node.object_id, node.node_name);

                // Device nodes will have device and card profile device IDs.
                // Virtual sinks/sources do not have these.
                if let Some(device_id) = node.device_id {
                    self.node_devices.insert(node.object_id, device_id);
                }

                let info = cosmic_settings_audio_core::NodeInfo {
                    name: node.node_name.clone(),
                    description: node.description,
                    device_profile_description: node.device_profile_description,
                    device_id: node.device_id,
                    card_profile_device: node.card_profile_device,
                    is_sink: matches!(node.media_class, pipewire::MediaClass::Sink),
                };

                self.emit_event(Event::Node(node.object_id, info.clone()))
                    .await;

                match node.media_class {
                    pipewire::MediaClass::Sink => {
                        self.sink_node_ids.push(node.object_id);

                        // Set the sink as the default if it matches the server.
                        if self.active_sink_node_name == node.node_name {
                            tracing::debug!(
                                target: "audio-backend",
                                "Node {} ({}) was the default sink",
                                node.object_id,
                                node.node_name
                            );
                            self.set_default_sink_id(node.object_id);
                            if self.active_sink_not_found {
                                tracing::warn!(target: "audio-backend", node_id = node.object_id, node_name = node.node_name, "missing default sink node ID found");
                                self.emit_event(Event::DefaultSink(node.object_id)).await;
                                self.active_sink_not_found = false;
                            }
                        }
                    }

                    pipewire::MediaClass::Source => {
                        self.source_node_ids.push(node.object_id);

                        // Set the source as the default if it matches the server.
                        if self.active_source_node_name == node.node_name {
                            tracing::debug!(
                                target: "audio-backend",
                                "Node {} ({}) was the default source",
                                node.object_id,
                                node.node_name
                            );
                            self.set_default_source_id(node.object_id);
                            if self.active_source_not_found {
                                tracing::warn!(target: "audio-backend", node_id = node.object_id, node_name = node.node_name, "missing default source node ID found");
                                self.emit_event(Event::DefaultSource(node.object_id)).await;
                                self.active_source_not_found = false;
                            }
                        }
                    }
                }

                self.node_info.insert(node.object_id, info.clone());
                self.node_volumes.entry(node.object_id).or_insert((0, None));
                self.node_mute.entry(node.object_id).or_insert(true);
            }

            pipewire::Event::MonoAudio(enabled) => {
                self.emit_event(Event::MonoAudio(enabled)).await;
                _ = self
                    .daemon_config_context
                    .set::<bool>("mono_sound", enabled);

                // Configure pipewire-pulse to force enable/disable mono as well.
                tokio::spawn(async move {
                    _ = tokio::process::Command::new("pactl")
                        .args([
                            "send-message",
                            "/core",
                            "pipewire-pulse:force-mono-output",
                            enabled.to_string().as_str(),
                        ])
                        .status()
                        .await;
                });
            }

            pipewire::Event::DefaultSink(node_name) => {
                tracing::debug!(target: "audio-backend", "default sink node changed to {node_name}");
                self.active_sink_node_name = node_name;
                if let Some(id) = self.node_id_from_name(&self.active_sink_node_name, true) {
                    self.set_default_sink_id(id);
                    self.emit_event(Event::DefaultSink(id)).await;
                    tracing::debug!(target: "audio-backend", name = self.active_sink_node_name, "default sink changed");
                    self.active_sink_not_found = false;
                } else {
                    tracing::warn!(target: "audio-backend", node_name = self.active_sink_node_name, "default sink node ID not found");
                    self.active_sink_not_found = true;
                }

                _ = self
                    .daemon_state_context
                    .set::<&str>("default_sink_name", &self.active_sink_node_name);
            }

            pipewire::Event::DefaultSource(node_name) => {
                tracing::debug!(target: "audio-backend", "default source node changed to {node_name}");
                self.active_source_node_name = node_name;
                if let Some(id) = self.node_id_from_name(&self.active_source_node_name, false) {
                    self.set_default_source_id(id);
                    self.emit_event(Event::DefaultSource(id)).await;
                    tracing::debug!(target: "audio-backend", name = self.active_source_node_name, "default source changed");
                    self.active_source_not_found = false;
                } else {
                    tracing::warn!(target: "audio-backend", node_name = self.active_source_node_name, "default source node ID not found");
                    self.active_source_not_found = true;
                }
            }

            pipewire::Event::RemoveDevice(id) => self.remove_device(id).await,
            pipewire::Event::RemoveNode(id) => self.remove_node(id).await,
        }
    }

    fn node_id_from_name(&self, name: &str, is_sink: bool) -> Option<u32> {
        self.node_info.iter().find_map(|(id, n)| {
            if n.name == name && n.is_sink == is_sink {
                Some(id)
            } else {
                None
            }
        })
    }

    async fn remove_device(&mut self, id: DeviceId) {
        tracing::debug!(target: "audio-backend", "Device {id} removed");
        _ = self.device_headset_check.remove(id);
        _ = self.device_headset_profiles.remove(id);
        _ = self.device_info.remove(id);
        _ = self.device_profiles.remove(id);
        _ = self.active_profiles.remove(id);
        _ = self.device_routes.remove(id);
        _ = self.active_routes.remove(id);
        self.emit_event(Event::RemoveDevice(id)).await;
    }

    async fn remove_node(&mut self, id: NodeId) {
        tracing::debug!(target: "audio-backend", "Node {id} removed");
        if self.active_sink_node == Some(id) {
            tracing::info!(target: "audio-backend", "Unsetting active sink node");
            self.active_sink_node = None;
        } else if self.active_source_node == Some(id) {
            tracing::info!(target: "audio-backend", "Unsetting active sink node");
            self.active_source_node = None;
        }

        if let Some(pos) = self.sink_node_ids.iter().position(|&node_id| node_id == id) {
            self.sink_node_ids.remove(pos);
        } else if let Some(pos) = self
            .source_node_ids
            .iter()
            .position(|&node_id| node_id == id)
        {
            self.source_node_ids.remove(pos);
        }

        _ = self.node_info.remove(id);
        _ = self.node_devices.remove(id);
        _ = self.node_mute.remove(id);

        _ = self.node_volumes.remove(id);

        self.emit_event(Event::RemoveNode(id)).await;
    }

    /// Set the default sink device by its the node ID.
    fn set_default_sink_id(&mut self, node_id: NodeId) {
        tracing::debug!(target: "audio-backend", "set_default_sink_id {node_id}");
        self.active_sink_node = Some(node_id);
        self.active_sink_node_name = self
            .node_info
            .get(node_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
    }

    /// Set the default source device by its the node ID.
    fn set_default_source_id(&mut self, node_id: NodeId) {
        tracing::debug!(target: "audio-backend", "set_default_source_id {node_id}");
        self.active_source_node = Some(node_id);
        self.active_source_node_name = self
            .node_info
            .get(node_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();
    }

    async fn update_node_properties(&mut self, id: DeviceId, props: NodeProps) {
        let is_active_sink = self.active_sink_node == Some(id);
        let is_active_source = self.active_source_node == Some(id);

        if let Some(mute) = props.mute {
            if is_active_sink {
                self.sink_mute = mute;
            } else if is_active_source {
                self.source_mute = mute;
            }

            if let Some(value) = self.node_mute.get_mut(id) {
                *value = mute;
                self.emit_event(Event::NodeMute(id, mute)).await;
            }
        }

        if let Some(channel_volumes) = props.channel_volumes {
            if channel_volumes.is_empty() {
                return;
            }

            let (volume, balance) = pipewire::volume::from_channel_volumes(&channel_volumes);
            let volume = (volume * 100.0).round() as u32;

            if is_active_sink {
                self.sink_balance = balance;
                self.sink_volume = volume;
            } else if is_active_source {
                self.source_volume = volume;
            }

            let value = self.node_volumes.entry(id).or_insert((0, None));
            *value = (volume, balance);
            self.emit_event(Event::NodeVolume(id, volume, balance))
                .await;
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    /// Handle messages from the sound server.
    Server(Arc<Vec<pipewire::Event>>),
    /// Pipe for notifying clients about audio events.
    Subscribe(Arc<pipe::Sender>),
    /// On init of the subscription, channels for closing background threads are given to the app.
    Init(Arc<pipewire::Sender>),
}

// TODO: Use pipewire library
pub async fn set_default(id: u32) {
    tracing::debug!(target: "audio-backend", "setting default node {id}");
    let id = numtoa::BaseN::<10>::u32(id);
    _ = tokio::process::Command::new("wpctl")
        .args(["set-default", id.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

/// Use this to set a virtual sink as a default.
/// TODO: We should be able to set this with pipewire-rs somehow.
pub async fn pactl_set_default_sink(node_name: &str) {
    tracing::debug!(target: "audio-backend", "setting default virtual node {node_name}");
    _ = tokio::process::Command::new("pactl")
        .args(["set-default-sink", node_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

/// Use this to set a virtual sink as a default.
/// TODO: We should be able to set this with pipewire-rs somehow.
pub async fn pactl_set_default_source(node_name: &str) {
    _ = tokio::process::Command::new("pactl")
        .args(["set-default-source", node_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

pub fn pipewire_profile_to_cosmic(
    profile: &cosmic_pipewire::Profile,
) -> cosmic_settings_audio_core::ProfileInfo {
    cosmic_settings_audio_core::ProfileInfo {
        name: profile.name.clone(),
        description: profile.description.clone(),
        index: profile.index as u32,
        priority: profile.priority as u32,
        availability: match profile.available {
            Availability::No => cosmic_settings_audio_core::Availability::No,
            Availability::Yes => cosmic_settings_audio_core::Availability::Yes,
            Availability::Unknown => cosmic_settings_audio_core::Availability::Unknown,
        },
        classes: profile
            .classes
            .iter()
            .cloned()
            .map(|class| match class {
                cosmic_pipewire::ProfileClass::AudioSink {
                    card_profile_devices,
                } => cosmic_settings_audio_core::ProfileClass::AudioSink {
                    card_profile_devices,
                },
                cosmic_pipewire::ProfileClass::AudioSource {
                    card_profile_devices,
                } => cosmic_settings_audio_core::ProfileClass::AudioSource {
                    card_profile_devices,
                },
            })
            .collect(),
    }
}

pub fn pipewire_route_to_cosmic(
    route: &cosmic_pipewire::Route,
) -> cosmic_settings_audio_core::RouteInfo {
    cosmic_settings_audio_core::RouteInfo {
        name: route.name.clone(),
        description: route.description.clone(),
        port_type: format!("{:?}", route.port_type),
        icon_name: route.icon_name.clone(),
        devices: route.devices.iter().map(|value| *value as u32).collect(),
        profiles: route.profiles.iter().map(|value| *value as u32).collect(),
        index: route.index as u32,
        priority: route.priority as u32,
        device: route.device as u32,
        profile: route.card_profile_port as u32,
        availability: match route.available {
            Availability::No => cosmic_settings_audio_core::Availability::No,
            Availability::Yes => cosmic_settings_audio_core::Availability::Yes,
            Availability::Unknown => cosmic_settings_audio_core::Availability::Unknown,
        },
        is_sink: matches!(route.direction, Direction::Output),
    }
}
