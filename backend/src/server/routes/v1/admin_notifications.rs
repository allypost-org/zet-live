use std::{
    net::IpAddr,
    sync::{Arc, LazyLock},
};

use serde::Deserialize;
use tokio::sync::broadcast;
use tracing::debug;

use super::{Broadcast, ToastData, Versioned};

pub static ADMIN_NOTIFICATION_TX: LazyLock<broadcast::Sender<Arc<AdminNotification>>> =
    LazyLock::new(|| {
        let (tx, _) = broadcast::channel(256);
        tx
    });

pub fn get_admin_notification_receiver() -> broadcast::Receiver<Arc<AdminNotification>> {
    ADMIN_NOTIFICATION_TX.subscribe()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToastPayload {
    pub message: String,
    #[serde(rename = "type", default = "default_toast_type")]
    pub toast_type: ToastType,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default = "default_target")]
    pub target: NotificationTarget,
    #[serde(default)]
    pub ips: Vec<IpAddr>,
    #[serde(default)]
    pub account: Option<String>,
}

const fn default_toast_type() -> ToastType {
    ToastType::Info
}

const fn default_target() -> NotificationTarget {
    NotificationTarget::All
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToastType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotificationTarget {
    All,
    Ips,
    Account,
}

pub enum AdminNotification {
    Toast {
        bytes: Vec<u8>,
        target: NotificationTarget,
        ips: Vec<IpAddr>,
        account: Option<String>,
    },
    SessionRevoked {
        text: String,
        user_id: String,
        session_id: String,
    },
}

pub async fn send_notification(payload: ToastPayload) {
    let Some(bytes) = serialize_toast(&payload.message, payload.toast_type, payload.duration)
    else {
        tracing::warn!("Failed to serialize toast payload");
        return;
    };

    let notification = Arc::new(AdminNotification::Toast {
        bytes,
        target: payload.target,
        ips: payload.ips,
        account: payload.account,
    });

    let receiver_count = ADMIN_NOTIFICATION_TX.send(notification).unwrap_or(0);
    debug!(receiver_count, "Admin notification sent");
}

fn serialize_toast(message: &str, toast_type: ToastType, duration: Option<u32>) -> Option<Vec<u8>> {
    let type_str = match toast_type {
        ToastType::Info => "info",
        ToastType::Success => "success",
        ToastType::Warning => "warning",
        ToastType::Error => "error",
    };

    let versioned = Versioned::new(
        1,
        Broadcast::Toast(ToastData {
            message: message.to_string(),
            toast_type: type_str.to_string(),
            duration,
        }),
    );
    minicbor_serde::to_vec(&versioned).ok()
}
