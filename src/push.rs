// Copyright 2021-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0

//! Extends base context with remote Push Notifications registration support.
//!
//! On Apple platforms, when enabled, APNS registration is triggered at app startup. Once a push
//! token is obtained, it is emitted via an app event to Tauri. The same flow occurs if registration
//! fails, but with a different event.
//!
//! These types must remain generic to platform naming ("APNS," "FCM," etc.) to allow for future
//! expansion to other platforms.
//!

/// The push token type.
pub type PushToken = Vec<u8>;

/// Push notifications features and utilities. On most platforms, this consists of obtaining
/// the token (which may be triggered at app start), then exposing it to the developer.
pub trait PushNotifications {}
