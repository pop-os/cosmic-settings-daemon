// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use futures_util::StreamExt;
use timedate_zbus::TimeDateProxy;
use tzf_rs::DefaultFinder;

pub enum Message {
    AutoTimezone(bool),
}

pub async fn daemon(auto_timezone: bool, mut receiver: tokio::sync::mpsc::Receiver<Message>) {
    let mut automatic_timezone_service = auto_timezone.then(|| tokio::task::spawn_local(watch()));

    while let Some(message) = receiver.recv().await {
        match message {
            Message::AutoTimezone(enable) => {
                if let Some(task) = automatic_timezone_service.take() {
                    task.abort();
                }

                if enable {
                    automatic_timezone_service = Some(tokio::task::spawn_local(watch()));
                }
            }
        }
    }
}

pub async fn watch() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("automatic timezone enabled");
    let zone_finder = DefaultFinder::new();

    let conn = zbus::Connection::system().await?;

    let manager = geoclue2::ManagerProxy::new(&conn).await?;
    let client = manager.get_client().await?;

    client.set_desktop_id("cosmic-settings-daemon").await?;
    client.set_distance_threshold(10000).await?;
    client
        .set_requested_accuracy_level(geoclue2::Accuracy::City as u32)
        .await?;

    let timedate = TimeDateProxy::new(&conn).await?;

    let mut location_updated = client.receive_location_updated().await?;

    client.start().await?;

    while let Some(signal) = location_updated.next().await {
        let args = signal.args()?;

        let location = geoclue2::LocationProxy::builder(&conn)
            .path(args.new())?
            .build()
            .await?;

        let latitude = location.latitude().await?;
        let longitude = location.longitude().await?;

        let timezone = zone_finder.get_tz_name(longitude, latitude);

        if timezone.is_empty() {
            continue;
        }

        tracing::info!("setting timezone to {timezone}");
        timedate.set_timezone(timezone, false).await?;
    }

    Ok(())
}
