// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

//! Varlink frontend for cosmic-settings-daemon

// TODO:
// - com.system76.CosmicConfig config, set_config, watch_config, state, set_state, watch_state,
// - com.system76.CosmicSettings.Display increase_brightness, decrease_brightness, set_brightness, recv_brightness,
// - com.system76.CosmicSettings.Keyboard increase_brightness, decrease_brightness, set_brightness, recv_brightness,

use cosmic_settings_audio_core as audio;
use cosmic_settings_audio_server as audio_server;
use cosmic_settings_printers_core as printers;
use cosmic_settings_printers_server as printers_server;
use futures_util::{Stream, StreamExt};
use std::{os::fd::OwnedFd, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

pub async fn init() -> (Daemon, impl Future<Output = ()> + 'static + Send) {
    let (audio_ctx, audio_ctx_rx) = audio_server::Context::new().await;

    let daemon = Daemon(Arc::new(Mutex::new(DaemonInner {
        audio_server: audio_server::Server::new(audio_ctx.clone()).await,
        printers_server: printers_server::Server::new(),
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
    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "RecvEvents",
        return_fds
    )]
    pub async fn audio_recv_events(&mut self) -> (Result<(), audio::Error>, Vec<OwnedFd>) {
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

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "RecvEventsV2",
        return_fds
    )]
    pub async fn audio_recv_events_v2(&mut self) -> (Result<(), audio::Error>, Vec<OwnedFd>) {
        let mut fds = Vec::new();
        let mut this = self.0.lock().await;
        let reply = match this.audio_server.recv_events_v2().await {
            Ok(fd) => {
                fds.push(fd);
                Ok(())
            }
            Err(why) => Err(why),
        };

        (reply, fds)
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "DefaultSink"
    )]
    pub async fn audio_default_sink(&mut self) -> Result<audio::Node, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .default_sink()
            .await
            .ok_or(audio::Error::NoActiveSink)
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "DefaultSource"
    )]
    pub async fn audio_default_source(&mut self) -> Result<audio::Node, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .default_sink()
            .await
            .ok_or(audio::Error::NoActiveSource)
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SelectHeadphoneProfile"
    )]
    pub async fn audio_select_headphone_profile(
        &mut self,
        device_id: u32,
    ) -> Result<(), audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .select_headphone_profile(device_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SelectHeadsetProfile"
    )]
    pub async fn audio_select_headset_profile(
        &mut self,
        device_id: u32,
    ) -> Result<(), audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .select_headset_profile(device_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SinkMuteToggle"
    )]
    pub async fn audio_sink_mute_toggle(&mut self) -> Result<audio::Mute, audio::Error> {
        self.0.lock().await.audio_server.sink_mute_toggle().await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SinkVolumeLower"
    )]
    pub async fn audio_sink_volume_lower(&mut self) -> Result<audio::Volume, audio::Error> {
        self.0.lock().await.audio_server.sink_volume_lower().await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SinkVolumeRaise"
    )]
    pub async fn audio_sink_volume_raise(&mut self) -> Result<audio::Volume, audio::Error> {
        self.0.lock().await.audio_server.sink_volume_raise().await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SourceMuteToggle"
    )]
    pub async fn audio_source_mute_toggle(&mut self) -> Result<audio::Mute, audio::Error> {
        self.0.lock().await.audio_server.source_mute_toggle().await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SourceVolumeLower"
    )]
    pub async fn audio_source_volume_lower(&mut self) -> Result<audio::Volume, audio::Error> {
        self.0.lock().await.audio_server.source_volume_lower().await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SourceVolumeRaise"
    )]
    pub async fn audio_source_volume_raise(&mut self) -> Result<audio::Volume, audio::Error> {
        self.0.lock().await.audio_server.source_volume_raise().await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio", rename = "SetDefault")]
    pub async fn audio_set_default(
        &mut self,
        node_id: u32,
        save: bool,
    ) -> Result<(), audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_default(node_id, save)
            .await
    }

    #[zlink(interface = "com.system76.CosmicSettings.Audio", rename = "SetProfile")]
    pub async fn audio_set_profile(
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

    #[zlink(interface = "com.system76.CosmicSettings.Audio", rename = "SetRoute")]
    pub async fn audio_set_route(
        &mut self,
        device_id: u32,
        card_profile_device: u32,
        route_index: u32,
        save: bool,
    ) -> Result<(), audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_route(device_id, card_profile_device, route_index, save)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SetSinkVolume"
    )]
    pub async fn audio_set_sink_volume(
        &mut self,
        volume: u32,
    ) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_sink_volume(volume)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SetSourceVolume"
    )]
    pub async fn audio_set_source_volume(
        &mut self,
        volume: u32,
    ) -> Result<audio::Volume, audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_source_volume(volume)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SetNodeMute"
    )]
    pub async fn audio_set_node_mute(
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

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SetNodeVolume"
    )]
    pub async fn audio_set_node_volume(
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

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SetNodeVolumeBalance"
    )]
    pub async fn audio_set_node_volume_balance(
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

    #[zlink(
        interface = "com.system76.CosmicSettings.Audio",
        rename = "SetPlaybackSink"
    )]
    pub async fn audio_set_playback_sink(
        &mut self,
        playback_id: u32,
        sink_id: u32,
    ) -> Result<(), audio::Error> {
        self.0
            .lock()
            .await
            .audio_server
            .set_playback_sink(playback_id, sink_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "ListPrinters"
    )]
    pub async fn printers_list_printers(
        &mut self,
    ) -> Result<printers::ListPrintersReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .list_printers()
            .await
            .map(|printers| printers::ListPrintersReply { printers })
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "GetPrinter"
    )]
    pub async fn printers_get_printer(
        &mut self,
        printer_id: String,
    ) -> Result<printers::PrinterEntry, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .get_printer(&printer_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "RefreshAvailableDestinations"
    )]
    pub async fn printers_refresh_available_destinations(&mut self) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .refresh_available_destinations()
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "StartPrinterApplicationDiscovery"
    )]
    pub async fn printers_start_printer_application_discovery(
        &mut self,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .start_printer_application_discovery()
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "StartAddPrinterDiscovery"
    )]
    pub async fn printers_start_add_printer_discovery(
        &mut self,
    ) -> Result<printers::StartAddPrinterDiscoveryReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .start_add_printer_discovery()
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "GetAddPrinterDiscovery"
    )]
    pub async fn printers_get_add_printer_discovery(
        &mut self,
    ) -> Result<printers::AddPrinterDiscoveryReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .get_add_printer_discovery()
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "ConfigureDiscoveredPrinter"
    )]
    pub async fn printers_configure_discovered_printer(
        &mut self,
        request: printers::ConfigureDiscoveredPrinterRequest,
    ) -> Result<printers::ConfigurePrinterReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .configure_discovered_printer(request)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "GetPrinterConfiguration"
    )]
    pub async fn printers_get_printer_configuration(
        &mut self,
        operation_id: String,
    ) -> Result<printers::ConfigurePrinterReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .get_printer_configuration(&operation_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "ListManualSetupPrinterApplications"
    )]
    pub async fn printers_list_manual_setup_printer_applications(
        &mut self,
    ) -> Result<printers::ListManualSetupApplicationsReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .list_manual_setup_printer_applications()
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "ListPrinterApplications"
    )]
    pub async fn printers_list_printer_applications(
        &mut self,
    ) -> Result<printers::ListPrinterApplicationsReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .list_printer_applications()
            .await
            .map(
                |printer_applications| printers::ListPrinterApplicationsReply {
                    printer_applications,
                },
            )
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "WatchPrinters",
        more
    )]
    pub async fn printers_watch_printers(
        &mut self,
        _more: bool,
    ) -> impl Stream<Item = zlink::Reply<printers::PrintersEvent>> + Unpin {
        // Varlink continuation framing belongs at this transport boundary.
        self.0
            .lock()
            .await
            .printers_server
            .watch_printers()
            .map(|event| zlink::Reply::new(Some(event)).set_continues(Some(true)))
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "DeletePrinter"
    )]
    pub async fn printers_delete_printer(
        &mut self,
        printer_id: String,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .delete_printer(&printer_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "SetPrinterAcceptJobs"
    )]
    pub async fn printers_set_printer_accept_jobs(
        &mut self,
        printer_id: String,
        enabled: bool,
        reason: String,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .set_printer_accept_jobs(&printer_id, enabled, &reason)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "SetPrinterDefault"
    )]
    pub async fn printers_set_printer_default(
        &mut self,
        printer_id: String,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .set_printer_default(&printer_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "ClearPrinterDefault"
    )]
    pub async fn printers_clear_printer_default(&mut self) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .clear_printer_default()
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "SetPrinterOptionDefault"
    )]
    pub async fn printers_set_printer_option_default(
        &mut self,
        printer_id: String,
        option: String,
        values: Vec<String>,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .set_printer_option_default(&printer_id, &option, &values)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "SetPrinterEnabled"
    )]
    pub async fn printers_set_printer_enabled(
        &mut self,
        printer_id: String,
        enabled: bool,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .set_printer_enabled(&printer_id, enabled)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "SetPrinterInfo"
    )]
    pub async fn printers_set_printer_info(
        &mut self,
        printer_id: String,
        info: String,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .set_printer_info(&printer_id, &info)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "SetPrinterLocation"
    )]
    pub async fn printers_set_printer_location(
        &mut self,
        printer_id: String,
        location: String,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .set_printer_location(&printer_id, &location)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "PrintTestPage"
    )]
    pub async fn printers_print_test_page(
        &mut self,
        printer_id: String,
    ) -> Result<printers::PrintTestPageReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .print_test_page(&printer_id)
            .await
            .map(|job_id| printers::PrintTestPageReply { job_id })
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "GetPrinterSupplies"
    )]
    pub async fn printers_get_printer_supplies(
        &mut self,
        printer_id: String,
    ) -> Result<printers::GetPrinterSuppliesReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .printer_supplies(&printer_id)
            .await
            .map(|supplies| printers::GetPrinterSuppliesReply { supplies })
    }

    #[zlink(interface = "com.system76.CosmicSettings.Printers", rename = "GetJobs")]
    pub async fn printers_get_jobs(
        &mut self,
        printer_id: String,
        filter: String,
    ) -> Result<printers::GetJobsReply, printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .get_jobs(&printer_id, &filter)
            .await
            .map(|jobs| printers::GetJobsReply { jobs })
    }

    #[zlink(interface = "com.system76.CosmicSettings.Printers", rename = "MoveJob")]
    pub async fn printers_move_job(
        &mut self,
        source_printer_id: String,
        job_id: i32,
        destination_printer_id: String,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .move_job(&source_printer_id, job_id, &destination_printer_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "PauseJob"
    )]
    pub async fn printers_pause_job(
        &mut self,
        printer_id: String,
        job_id: i32,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .pause_job(&printer_id, job_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "ResumeJob"
    )]
    pub async fn printers_resume_job(
        &mut self,
        printer_id: String,
        job_id: i32,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .resume_job(&printer_id, job_id)
            .await
    }

    #[zlink(
        interface = "com.system76.CosmicSettings.Printers",
        rename = "CancelJob"
    )]
    pub async fn printers_cancel_job(
        &mut self,
        printer_id: String,
        job_id: i32,
    ) -> Result<(), printers::Error> {
        self.0
            .lock()
            .await
            .printers_server
            .cancel_job(&printer_id, job_id)
            .await
    }
}

pub struct DaemonInner {
    pub audio_server: audio_server::Server,
    pub printers_server: printers_server::Server,
}
