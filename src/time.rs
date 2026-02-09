use tokio_stream::StreamExt;
use zbus::fdo::PropertiesProxy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeChange {
    /// Wall clock changed (manual change, NTP step, etc).
    WallClock,
    /// System resumed from suspend.
    Resume,
}

#[zbus::proxy(
    default_service = "org.freedesktop.login1",
    interface = "org.freedesktop.login1.Manager",
    default_path = "/org/freedesktop/login1"
)]
trait LogindManager {
    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

pub struct TimeWatcher {
    _conn: zbus::Connection,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for TimeWatcher {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub async fn watch_time_changes() -> anyhow::Result<(
    TimeWatcher,
    tokio_stream::wrappers::ReceiverStream<TimeChange>,
)> {
    let conn = zbus::Connection::system().await?;

    // Small buffer; time-change events can be coalesced.
    let (tx, rx) = tokio::sync::mpsc::channel(4);

    let tasks = vec![
        {
            let conn = conn.clone();
            let tx = tx.clone();
            tokio::task::spawn_local(async move {
                let proxy = match LogindManagerProxy::builder(&conn).build().await {
                    Ok(proxy) => proxy,
                    Err(err) => {
                        log::warn!("Failed to connect to logind Manager proxy: {err:?}");
                        return;
                    }
                };

                let mut stream = match proxy.receive_prepare_for_sleep().await {
                    Ok(stream) => stream,
                    Err(err) => {
                        log::warn!("Failed to subscribe to PrepareForSleep: {err:?}");
                        return;
                    }
                };

                loop {
                    tokio::select! {
                        _ = tx.closed() => break,
                        signal = stream.next() => {
                            let Some(signal) = signal else { break };
                            let Ok(args) = signal.args() else { continue };
                            if !args.start {
                                // Best-effort: if the receiver is lagging, coalesce.
                                let _ = tx.try_send(TimeChange::Resume);
                            }
                        }
                    }
                }
            })
        },
        {
            let conn = conn.clone();
            let tx = tx.clone();
            tokio::task::spawn_local(async move {
                let builder = match PropertiesProxy::builder(&conn)
                    .destination("org.freedesktop.timedate1")
                    .and_then(|b| b.path("/org/freedesktop/timedate1"))
                {
                    Ok(builder) => builder,
                    Err(err) => {
                        log::warn!("Failed to build timedate1 Properties proxy: {err:?}");
                        return;
                    }
                };

                let proxy = match builder.build().await {
                    Ok(proxy) => proxy,
                    Err(err) => {
                        log::warn!("Failed to connect to timedate1 Properties proxy: {err:?}");
                        return;
                    }
                };

                let mut stream = match proxy.receive_properties_changed().await {
                    Ok(stream) => stream,
                    Err(err) => {
                        log::warn!("Failed to subscribe to timedate1 PropertiesChanged: {err:?}");
                        return;
                    }
                };

                loop {
                    tokio::select! {
                        _ = tx.closed() => break,
                        signal = stream.next() => {
                            let Some(signal) = signal else { break };
                            let Ok(args) = signal.args() else { continue };

                            if args.interface_name == "org.freedesktop.timedate1" {
                                // Best-effort: if the receiver is lagging, coalesce.
                                let _ = tx.try_send(TimeChange::WallClock);
                            }
                        }
                    }
                }
            })
        },
    ];

    Ok((
        TimeWatcher { _conn: conn, tasks },
        tokio_stream::wrappers::ReceiverStream::new(rx),
    ))
}
