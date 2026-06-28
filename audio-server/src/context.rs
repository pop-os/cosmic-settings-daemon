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
            let mut attempt: u32 = 0;
            
            loop {
                attempt += 1;
                
                // Calculate delay for this attempt (0ms for first attempt)
                let retry_delay: Duration = calculate_retry_delay(attempt);
                
                let sender = Arc::new((Mutex::new(Vec::new()), tokio::sync::Notify::const_new()));
                let receiver = sender.clone();

                // Create oneshot channel for initialization callback
                let (init_tx, init_rx) = tokio::sync::oneshot::channel();
                
                // Start PipeWire thread with initialization callback
                let handle = cosmic_pipewire::run(
                    move |event| {
                        sender.0.lock().unwrap().push(event);
                        sender.1.notify_one();
                    },
                    move |result: Result<(), String>| {
                        let _ = init_tx.send(result);
                    },
                );
                
                _ = tx.send(Message::Init(Arc::new(handle)));

                // Wait for initialization result (with timeout)
                match tokio::time::timeout(Duration::from_millis(500), init_rx).await {
                    Ok(Ok(Ok(()))) => {
                        // Initialization succeeded!
                        if attempt > 1 {
                            tracing::info!("PipeWire connected successfully after {} attempts", attempt);
                        } else {
                            tracing::info!("PipeWire connected successfully");
                        }
                        
                        // Run the event forwarder (this runs indefinitely)
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

                        forwarder.await;
                        
                        // If forwarder exits (shouldn't happen normally), restart from attempt 1
                        tracing::warn!("PipeWire forwarder exited unexpectedly, restarting...");
                        attempt = 0;
                    }
                    Ok(Ok(Err(err))) => {
                        // Initialization failed with specific error
                        tracing::warn!(
                            "PipeWire initialization failed (attempt {}): {}. Retrying in {:?}",
                            attempt, err, retry_delay
                        );
                        tokio::time::sleep(retry_delay).await;
                    }
                    Ok(Err(_)) => {
                        // Channel closed - thread exited without calling callback
                        tracing::warn!(
                            "PipeWire initialization callback channel closed (attempt {}). Retrying in {:?}",
                            attempt, retry_delay
                        );
                        tokio::time::sleep(retry_delay).await;
                    }
                    Err(_) => {
                        // Timeout - initialization is taking too long
                        tracing::warn!(
                            "PipeWire initialization timeout (attempt {}). Retrying in {:?}",
                            attempt, retry_delay
                        );
                        tokio::time::sleep(retry_delay).await;
                    }
                }
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

/// Calculate the retry delay for a given attempt number.
/// 
/// - Attempt 1: 0ms (immediate)
/// - Attempt 2: 100ms
/// - Attempt 3: 200ms
/// - Attempt 4: 400ms
/// - ... doubles each time
/// - Attempt 9+: 12,800ms (capped after 8 doublings)
fn calculate_retry_delay(attempt: u32) -> Duration {
    const BASE_DELAY_MS: u64 = 100;
    const MAX_DOUBLINGS: u32 = 7;
    
    if attempt <= 1 {
        Duration::from_millis(0)
    } else {
        let doublings: u32 = (attempt - 2).min(MAX_DOUBLINGS);
        let delay_ms: u64 = BASE_DELAY_MS * 2u64.pow(doublings);
        Duration::from_millis(delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_delay_first_attempt_immediate() {
        let delay: Duration = calculate_retry_delay(1);
        assert_eq!(delay, Duration::from_millis(0));
    }

    #[test]
    fn test_retry_delay_second_attempt_100ms() {
        let delay: Duration = calculate_retry_delay(2);
        assert_eq!(delay, Duration::from_millis(100));
    }

    #[test]
    fn test_retry_delay_third_attempt_200ms() {
        let delay: Duration = calculate_retry_delay(3);
        assert_eq!(delay, Duration::from_millis(200));
    }

    #[test]
    fn test_retry_delay_fourth_attempt_400ms() {
        let delay: Duration = calculate_retry_delay(4);
        assert_eq!(delay, Duration::from_millis(400));
    }

    #[test]
    fn test_retry_delay_fifth_attempt_800ms() {
        let delay: Duration = calculate_retry_delay(5);
        assert_eq!(delay, Duration::from_millis(800));
    }

    #[test]
    fn test_retry_delay_sixth_attempt_1600ms() {
        let delay: Duration = calculate_retry_delay(6);
        assert_eq!(delay, Duration::from_millis(1600));
    }

    #[test]
    fn test_retry_delay_seventh_attempt_3200ms() {
        let delay: Duration = calculate_retry_delay(7);
        assert_eq!(delay, Duration::from_millis(3200));
    }

    #[test]
    fn test_retry_delay_eighth_attempt_6400ms() {
        let delay: Duration = calculate_retry_delay(8);
        assert_eq!(delay, Duration::from_millis(6400));
    }

    #[test]
    fn test_retry_delay_ninth_attempt_12800ms() {
        let delay: Duration = calculate_retry_delay(9);
        assert_eq!(delay, Duration::from_millis(12800));
    }

    #[test]
    fn test_retry_delay_tenth_attempt_capped_at_12800ms() {
        let delay: Duration = calculate_retry_delay(10);
        assert_eq!(delay, Duration::from_millis(12800));
    }

    #[test]
    fn test_retry_delay_hundredth_attempt_still_capped() {
        let delay: Duration = calculate_retry_delay(100);
        assert_eq!(delay, Duration::from_millis(12800));
    }

    #[tokio::test]
    async fn test_context_with_retry_mechanism() {
        // This test verifies that the context compiles and can be created
        // with the retry mechanism in place. It doesn't actually connect to PipeWire.
        let (_ctx, _rx) = Context::new().await;
        // Success: context created without errors
    }
}
