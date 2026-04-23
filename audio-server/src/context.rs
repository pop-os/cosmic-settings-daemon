// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::backend::*;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Context {
    pub model: Arc<tokio::sync::Mutex<Model>>,
    pub sender: mpsc::UnboundedSender<Message>,
}

impl Context {
    pub async fn new() -> (Self, mpsc::UnboundedReceiver<Message>) {
        let (tx, rx) = mpsc::unbounded_channel();

        let manager = Context {
            model: Arc::new(tokio::sync::Mutex::new(Model::new().await)),
            sender: tx,
        };

        (manager, rx)
    }

    pub async fn run(self, mut rx: mpsc::UnboundedReceiver<Message>) {
        let tx = self.sender.clone();

        let pipewire_backend = Box::pin(async move {
            loop {
                let sender = Arc::new((Mutex::new(Vec::new()), tokio::sync::Notify::const_new()));
                let receiver = sender.clone();

                _ = tx.send(Message::Init(Arc::new(cosmic_pipewire::run(
                    move |event| {
                        sender.0.lock().unwrap().push(event);
                        sender.1.notify_one();
                    },
                ))));

                let forwarder = Box::pin(async {
                    loop {
                        _ = receiver.1.notified().await;
                        let events = std::mem::take(&mut *receiver.0.lock().unwrap());
                        if !events.is_empty() {
                            _ = tx.send(Message::Server(Arc::from(events)));
                            tokio::time::sleep(Duration::from_millis(64)).await;
                        }
                    }
                });

                forwarder.await
            }
        });

        let frontend_fut = Box::pin(async move {
            while let Some(message) = rx.recv().await {
                self.model.lock().await.update(message).await;
            }
        });

        futures_util::future::select(frontend_fut, pipewire_backend).await;
    }
}
