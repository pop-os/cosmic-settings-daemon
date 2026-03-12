// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic_settings_audio_client::CosmicAudioProxy;
use futures_util::StreamExt;
use tracing_subscriber::prelude::*;

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_format = tracing_subscriber::fmt::format()
        .pretty()
        .without_time()
        .with_line_number(true)
        .with_file(true)
        .with_target(false)
        .with_thread_names(true);

    let log_layer = tracing_subscriber::fmt::Layer::default()
        .with_writer(std::io::stderr)
        .event_format(log_format);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_env("RUST_LOG"))
        .with(log_layer)
        .init();

    let mut client = cosmic_settings_audio_client::connect().await.unwrap();

    println!(
        "default source: {:?}",
        client.conn.default_source().await.unwrap()
    );
    println!(
        "default sink: {:?}",
        client.conn.default_source().await.unwrap()
    );

    client
        .recv_events()
        .await??
        .for_each(|result| async move {
            eprintln!("{:?}", result.unwrap());
        })
        .await;

    Ok(())
}
