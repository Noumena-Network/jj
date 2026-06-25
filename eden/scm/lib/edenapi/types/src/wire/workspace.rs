/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Wire types for workspace CRUD.

use serde::Deserialize;
use serde::Serialize;

use crate::wire::is_default;

/// Wire representation of a JJ workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjWorkspace {
    #[serde(rename = "0")]
    pub name: String,
    #[serde(rename = "1")]
    pub repo: String,
    #[serde(rename = "2")]
    pub current_view_id: Vec<u8>,
    #[serde(rename = "3")]
    pub current_operation_id: Vec<u8>,
    #[serde(default, skip_serializing_if = "is_default", rename = "4")]
    pub working_copy_parent: Option<Vec<u8>>,
}

/// Request to read a workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireGetWorkspaceRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub workspace: String,
}

/// Response from reading a workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireGetWorkspaceResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub workspace: Option<WireJjWorkspace>,
}

/// Request to write a workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePutWorkspaceRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub workspace: String,
    #[serde(rename = "2")]
    pub current_view_id: Vec<u8>,
    #[serde(rename = "3")]
    pub current_operation_id: Vec<u8>,
    #[serde(default, skip_serializing_if = "is_default", rename = "4")]
    pub working_copy_parent: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "is_default", rename = "5")]
    pub expected_current_operation_id: Option<Vec<u8>>,
}

/// Response from writing a workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePutWorkspaceResponse {
    #[serde(rename = "0")]
    pub workspace: WireJjWorkspace,
}

/// Request to list workspaces.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireListWorkspacesRequest {
    #[serde(rename = "0")]
    pub repo: String,
}

/// Response from listing workspaces.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireListWorkspacesResponse {
    #[serde(rename = "0")]
    pub workspaces: Vec<WireJjWorkspace>,
}

/// Request to delete a workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireDeleteWorkspaceRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub workspace: String,
    #[serde(default, skip_serializing_if = "is_default", rename = "2")]
    pub expected_current_operation_id: Option<Vec<u8>>,
}

/// Response from deleting a workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireDeleteWorkspaceResponse;

impl crate::wire::ToWire for WireJjWorkspace {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjWorkspace {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireGetWorkspaceRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireGetWorkspaceRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireGetWorkspaceResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireGetWorkspaceResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WirePutWorkspaceRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WirePutWorkspaceRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WirePutWorkspaceResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WirePutWorkspaceResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireListWorkspacesRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireListWorkspacesRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireListWorkspacesResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireListWorkspacesResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireDeleteWorkspaceRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireDeleteWorkspaceRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireDeleteWorkspaceResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireDeleteWorkspaceResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}
