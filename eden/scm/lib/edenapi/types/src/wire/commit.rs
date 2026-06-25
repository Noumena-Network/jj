/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Wire types for commit read/write.

use serde::Deserialize;
use serde::Serialize;

use crate::wire::is_default;

/// Wire representation of a JJ signature.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjSignature {
    #[serde(rename = "0")]
    pub name: String,
    #[serde(rename = "1")]
    pub email: String,
    #[serde(rename = "2")]
    pub timestamp: i64,
    #[serde(rename = "3")]
    pub tz_offset: i32,
}

/// Wire representation of a secure signature.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjSecureSig {
    #[serde(rename = "0")]
    pub data: Vec<u8>,
    #[serde(rename = "1")]
    pub sig: Vec<u8>,
}

/// Wire representation of a JJ file change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireJjFileChange {
    #[serde(rename = "0")]
    Addition(WireJjFileAddition),
    #[serde(rename = "1")]
    Deletion(WireJjFileDeletion),
    #[serde(rename = "2")]
    Unknown,
}

impl Default for WireJjFileChange {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Wire representation of a file addition.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjFileAddition {
    #[serde(rename = "0")]
    pub path: String,
    #[serde(rename = "1")]
    pub file_type: WireJjFileType,
    #[serde(rename = "2")]
    pub content: Vec<u8>,
}

/// Wire representation of a file deletion.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjFileDeletion {
    #[serde(rename = "0")]
    pub path: String,
}

/// Wire representation of a JJ file type.
#[derive(Clone, Debug, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireJjFileType {
    #[serde(rename = "0")]
    Regular,
    #[serde(rename = "1")]
    Executable,
    #[serde(rename = "2")]
    Symlink,
    #[serde(rename = "3")]
    Submodule,
    #[serde(rename = "4")]
    #[default]
    Unknown,
}

/// Wire representation of a JJ commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjCommit {
    #[serde(rename = "0")]
    pub commit_id: Vec<u8>,
    #[serde(rename = "1")]
    pub change_id: Vec<u8>,
    #[serde(rename = "2")]
    pub parents: Vec<Vec<u8>>,
    #[serde(rename = "3")]
    pub root_tree: Vec<u8>,
    #[serde(rename = "4")]
    pub author: WireJjSignature,
    #[serde(rename = "5")]
    pub committer: WireJjSignature,
    #[serde(rename = "6")]
    pub description: String,
    #[serde(default, skip_serializing_if = "is_default", rename = "7")]
    pub secure_sig: Option<WireJjSecureSig>,
    #[serde(default, skip_serializing_if = "is_default", rename = "8")]
    pub file_changes: Vec<WireJjFileChange>,
}

/// Request to read a commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadCommitRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub commit_id: Vec<u8>,
}

/// Response from reading a commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadCommitResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub commit: Option<WireJjCommit>,
}

/// Request to write a commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteCommitRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub commit: WireJjCommit,
}

/// Response from writing a commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteCommitResponse {
    #[serde(rename = "0")]
    pub commit_id: Vec<u8>,
}

// Transparent ToWire/ToApi for all types in this module since wire and API are the same.

impl crate::wire::ToWire for WireJjSignature {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjSignature {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjSecureSig {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjSecureSig {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjFileAddition {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjFileAddition {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjFileDeletion {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjFileDeletion {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjFileChange {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjFileChange {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjFileType {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjFileType {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjCommit {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjCommit {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadCommitRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadCommitRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadCommitResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadCommitResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteCommitRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteCommitRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteCommitResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteCommitResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}
