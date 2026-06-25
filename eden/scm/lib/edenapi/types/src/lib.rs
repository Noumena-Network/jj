/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Wire representation types for the JJ EdenAPI.
//!
//! Convention (copied from Sapling EdenAPI types):
//! 1. Every field renamed to a unique natural number (`#[serde(rename = "0")]`).
//! 2. Every enum has an `Unknown` variant as the last variant.
//! 3. Fields should use `#[serde(default, skip_serializing_if = "is_default")]`.
//! 4. All fields wrapped in `Option` or a container that may be empty.

pub mod wire;

pub use wire::ToApi;
pub use wire::ToWire;
