/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Wire types for tree read/write.

use serde::Deserialize;
use serde::Serialize;

use crate::wire::is_default;

/// Wire representation of a tree entry for a file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjTreeEntryFile {
    #[serde(rename = "0")]
    pub file_id: Vec<u8>,
    #[serde(rename = "1")]
    pub executable: bool,
}

/// Wire representation of a tree entry for a directory.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjTreeEntryDirectory {
    #[serde(rename = "0")]
    pub tree_id: Vec<u8>,
}

/// Wire representation of a tree entry for a symlink.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjTreeEntrySymlink {
    #[serde(rename = "0")]
    pub file_id: Vec<u8>,
}

/// Wire representation of a tree entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireJjTreeEntry {
    #[serde(rename = "0")]
    File(WireJjTreeEntryFile),
    #[serde(rename = "1")]
    Directory(WireJjTreeEntryDirectory),
    #[serde(rename = "2")]
    Symlink(WireJjTreeEntrySymlink),
    #[serde(rename = "3")]
    Unknown,
}

impl Default for WireJjTreeEntry {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Wire representation of a JJ tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjTree {
    #[serde(rename = "0")]
    pub tree_id: Vec<u8>,
    #[serde(default, skip_serializing_if = "is_default", rename = "1")]
    pub entries: std::collections::HashMap<String, WireJjTreeEntry>,
}

/// Request to read a tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadTreeRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub tree_id: Vec<u8>,
}

/// Response from reading a tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadTreeResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub tree: Option<WireJjTree>,
}

/// Request to write a tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteTreeRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub tree: WireJjTree,
}

/// Response from writing a tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteTreeResponse {
    #[serde(rename = "0")]
    pub tree_id: Vec<u8>,
}

impl crate::wire::ToWire for WireJjTreeEntryFile {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjTreeEntryFile {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjTreeEntryDirectory {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjTreeEntryDirectory {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjTreeEntrySymlink {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjTreeEntrySymlink {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjTreeEntry {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjTreeEntry {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjTree {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjTree {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadTreeRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadTreeRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadTreeResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadTreeResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteTreeRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteTreeRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteTreeResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteTreeResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}
