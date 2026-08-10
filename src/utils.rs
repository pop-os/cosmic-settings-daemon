// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::OnceLock;

/// Globally-shared dbus system connection.
pub async fn zbus_system_connection() -> Option<zbus::Connection> {
    static CONNECTION: OnceLock<zbus::Connection> = OnceLock::new();

    if let Some(conn) = CONNECTION.get() {
        return Some(conn.clone());
    }

    let conn = zbus::Connection::system().await.ok()?;
    _ = CONNECTION.set(conn.clone());
    Some(conn)
}
