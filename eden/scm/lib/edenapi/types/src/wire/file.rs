/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Wire types for file read/write and symlink read/write.

use serde::Deserialize;
use serde::Serialize;

use crate::wire::is_default;

/// Request to read a file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadFileRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub file_id: Vec<u8>,
}

/// Response from reading a file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadFileResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub content: Option<Vec<u8>>,
}

/// Request to write a file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteFileRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub content: Vec<u8>,
}

/// Response from writing a file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteFileResponse {
    #[serde(rename = "0")]
    pub file_id: Vec<u8>,
}

// --- symlink -----------------------------------------------------------

/// Request to read a symbolic link target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadSymlinkRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub file_id: Vec<u8>,
}

/// Response from reading a symbolic link target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadSymlinkResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub target: Option<String>,
}

/// Request to write a symbolic link target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteSymlinkRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub target: String,
}

/// Response from writing a symbolic link target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteSymlinkResponse {
    #[serde(rename = "0")]
    pub file_id: Vec<u8>,
}

// --- existing ToWire/ToApi impls remain below or after this block ---

impl crate::wire::ToWire for WireReadFileRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadFileRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadFileResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadFileResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteFileRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteFileRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteFileResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteFileResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadSymlinkRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadSymlinkRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadSymlinkResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadSymlinkResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteSymlinkRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteSymlinkRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteSymlinkResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteSymlinkResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}
