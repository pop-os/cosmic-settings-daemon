use std::{collections::BTreeMap, io, path::Path, rc::Rc};

use futures::{Stream, StreamExt};
pub use geonames::GeoPosition;
use notify::{RecursiveMode, Watcher};

static GEODATA: &'static [u8] = include_bytes!("../data/timezone-geodata.bitcode-v0-6");

pub fn decode_geodata() -> BTreeMap<String, GeoPosition> {
    match geonames::bitcode::decode(GEODATA) {
        Ok(ok) => ok,
        Err(err) => {
            eprintln!("failed to decode timezone geodata: {}", err.to_string());
            BTreeMap::new()
        }
    }
}

pub fn receive_timezones() -> impl Stream<Item = io::Result<String>> {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if matches!(
            result.map(|event| event.kind),
            Ok(notify::EventKind::Modify(notify::event::ModifyKind::Data(
                _
            )))
        ) {
            futures::executor::block_on(async {
                _ = tx.send(()).await;
            })
        }
    })
    .unwrap();

    let timezone_path = Rc::from(Path::new("/etc/timezone").canonicalize().unwrap());

    _ = watcher.watch(&timezone_path, RecursiveMode::NonRecursive);

    tokio_stream::wrappers::ReceiverStream::new(rx)
        .then(move |_| {
            let timezone_path = timezone_path.clone();
            async move { std::fs::read_to_string(&timezone_path).map(|contents| contents.trim().to_owned()) }
        })
}
