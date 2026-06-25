/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Wire types for bookmark resolve/list and mutation.

use serde::Deserialize;
use serde::Serialize;

use crate::wire::is_default;

/// Wire representation of a JJ bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjBookmark {
    #[serde(rename = "0")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default", rename = "1")]
    pub commit_id: Option<Vec<u8>>,
}

/// Request to resolve a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireResolveBookmarkRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub name: String,
}

/// Response from resolving a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireResolveBookmarkResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub commit_id: Option<Vec<u8>>,
}

/// Request to list bookmarks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireListBookmarksRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(default, skip_serializing_if = "is_default", rename = "1")]
    pub prefix: Option<String>,
}

/// Response from listing bookmarks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireListBookmarksResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub bookmarks: Vec<WireJjBookmark>,
}

impl crate::wire::ToWire for WireJjBookmark {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjBookmark {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

/// Request to create a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCreateBookmarkRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub name: String,
    #[serde(rename = "2")]
    pub target_commit_id: Vec<u8>,
}

/// Response from creating a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCreateBookmarkResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub bookmark: Option<WireJjBookmark>,
}

/// Request to move a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireMoveBookmarkRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub name: String,
    #[serde(rename = "2")]
    pub old_target_commit_id: Vec<u8>,
    #[serde(rename = "3")]
    pub new_target_commit_id: Vec<u8>,
}

/// Response from moving a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireMoveBookmarkResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub bookmark: Option<WireJjBookmark>,
}

/// Request to delete a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireDeleteBookmarkRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub name: String,
    #[serde(rename = "2")]
    pub old_target_commit_id: Vec<u8>,
}

/// Response from deleting a bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireDeleteBookmarkResponse;

impl crate::wire::ToWire for WireResolveBookmarkRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireResolveBookmarkRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireResolveBookmarkResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireResolveBookmarkResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireListBookmarksRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireListBookmarksRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireListBookmarksResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireListBookmarksResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireCreateBookmarkRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireCreateBookmarkRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireCreateBookmarkResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireCreateBookmarkResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireMoveBookmarkRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireMoveBookmarkRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireMoveBookmarkResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireMoveBookmarkResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireDeleteBookmarkRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireDeleteBookmarkRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireDeleteBookmarkResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireDeleteBookmarkResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}
