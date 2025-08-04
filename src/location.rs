use std::{collections::BTreeMap, io, path::Path, rc::Rc, time::Duration};

use futures::{Stream, StreamExt};
pub use geonames::GeoPosition;
use notify::{PollWatcher, RecursiveMode, Watcher};

static GEODATA: &'static [u8] = include_bytes!("../data/timezone-geodata.bitcode-v0-6");

/// Decodes the embedded geodata containing the largest cities nearest each timezone.
pub fn decode_geodata() -> BTreeMap<String, GeoPosition> {
    match geonames::bitcode::decode(GEODATA) {
        Ok(ok) => ok,
        Err(err) => {
            log::error!("failed to decode timezone geodata: {}", err.to_string());
            BTreeMap::new()
        }
    }
}

/// Get a stream of timezone updates backed by a poll watcher.
pub fn receive_timezones() -> (PollWatcher, impl Stream<Item = io::Result<String>>) {
    let (tx, rx) = tokio::sync::mpsc::channel(1);

    let timezone_path: Rc<Path> = Rc::from(Path::new("/etc/localtime"));

    futures::executor::block_on(async {
        _ = tx.send(()).await;
    });

    let event_handler = move |result: notify::Result<notify::Event>| {
        if matches!(
            result.map(|event| event.kind),
            Ok(notify::EventKind::Modify(_))
        ) {
            futures::executor::block_on(async {
                _ = tx.send(()).await;
            })
        }
    };

    // NOTE: Uses a poll watcher because inotify watchers automatically follow symlinks to files.
    // We should switch back to an INotifyWatcher the moment the notify crate supports disabling symlink follows.
    let mut watcher = notify::PollWatcher::new(
        event_handler,
        notify::Config::default()
            .with_follow_symlinks(false) // NOTE: Does not currently do anything
            .with_poll_interval(Duration::from_secs(1)),
    )
    .unwrap();

    _ = watcher.watch(&timezone_path, RecursiveMode::NonRecursive);

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).then(move |_| {
        let timezone_path = timezone_path.clone();
        async move {
            let mut zoneinfo_path = timezone_path.read_link()?;

            // The zoneinfo path may require resolving twice to get the correct geoname timezone.
            if let Ok(path) = zoneinfo_path.read_link() {
                zoneinfo_path = path;
            }

            Ok(timezone_from_path(&zoneinfo_path))
        }
    });

    (watcher, stream)
}

/// Get timezone from a zoneinfo path
fn timezone_from_path(path: &Path) -> String {
    path.components()
        .rev()
        .take_while(|component| {
            component.as_os_str() != "zoneinfo" && component.as_os_str() != ".."
        })
        .fold(String::new(), |buffer, component| {
            let name = component.as_os_str().to_str().unwrap();
            if buffer.is_empty() {
                name.to_owned()
            } else {
                [name, "/", &buffer].concat()
            }
        })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn timezone_from_path() {
        let mut path = Path::new("/usr/share/zoneinfo/America/Denver");
        assert_eq!(
            super::timezone_from_path(path),
            String::from("America/Denver")
        );

        path = Path::new("../Pacific/Honolulu");
        assert_eq!(
            super::timezone_from_path(path),
            String::from("Pacific/Honolulu")
        );
    }
}
