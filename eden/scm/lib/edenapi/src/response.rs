/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Response wrappers for the JJ EdenAPI client.
//!
//! These types mirror `SaplingRemoteApi`'s `Response<T>`+`Entries<T>`+`ResponseMeta`
//! pattern. CBOR data arrives as a newline-delimited or length-delimited streaming
//! blob; `Entries` wraps a `tokio::sync::mpsc::UnboundedReceiver<Result<T, E>>`
//! so callers can `next().await` items.

use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use futures::Stream;
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::mpsc::unbounded;
use serde::Serialize;

use crate::errors::JjRemoteApiError;

#[derive(Debug, Default, Serialize)]
pub struct ResponseMeta {
    pub server_timestamp: u64,
}

#[derive(Debug)]
pub struct Entries<T> {
    recv: UnboundedReceiver<T>,
}

impl<T> Entries<T> {
    pub fn new(rx: UnboundedReceiver<T>) -> Self {
        Self { recv: rx }
    }

    /// Collect all entries into a `Vec`.
    pub async fn into_vec(mut self) -> Vec<T> {
        let mut out = Vec::new();
        while let Some(item) = self.recv.next().await {
            out.push(item);
        }
        out
    }
}

impl<'a, T> Stream for Entries<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.recv).poll_next(cx)
    }
}

impl<'a, T> Entries<Result<T, JjRemoteApiError>> {
    /// Convenience adapter that polls the next item and returns Ok/Err directly.
    pub fn try_poll_next(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<T, JjRemoteApiError>>> {
        Pin::new(&mut self.recv).poll_next(cx)
    }
}

/// Response containing an async stream of entries plus metadata.
#[derive(Debug)]
pub struct Response<T> {
    pub entries: Entries<Result<T, JjRemoteApiError>>,
}

impl<T> Response<T> {
    pub fn empty() -> Self {
        let (tx, rx) = unbounded();
        drop(tx);
        Self { entries: Entries::new(rx) }
    }

    /// Collect every entry into a `Vec`, aborting on any stream-level error.
    pub async fn flatten(mut self) -> Result<Vec<T>, JjRemoteApiError> {
        let mut out = Vec::new();
        while let Some(result) = self.entries.next().await {
            match result {
                Ok(entry) => out.push(entry),
                Err(err) => return Err(err),
            }
        }
        Ok(out)
    }
}
