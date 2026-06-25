/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Wire types for op-store CRUD.
//!
//! These mirror `jj_opstore.thrift` request/response shapes.

use serde::Deserialize;
use serde::Serialize;

use crate::wire::is_default;

/// Wire representation of payload encoding.
#[derive(Clone, Debug, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireJjOpStorePayloadEncoding {
    #[serde(rename = "0")]
    #[default]
    Unknown,
    #[serde(rename = "1")]
    SimpleOpStoreProtoV1,
}

/// Wire representation of a view object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjViewObject {
    #[serde(rename = "0")]
    pub view_id: Vec<u8>,
    #[serde(rename = "1")]
    pub encoding: WireJjOpStorePayloadEncoding,
    #[serde(rename = "2")]
    pub payload: Vec<u8>,
}

/// Wire representation of an operation object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjOperationObject {
    #[serde(rename = "0")]
    pub operation_id: Vec<u8>,
    #[serde(rename = "1")]
    pub encoding: WireJjOpStorePayloadEncoding,
    #[serde(rename = "2")]
    pub payload: Vec<u8>,
}

/// Wire representation of op-head state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireJjOpHeadState {
    #[serde(rename = "0")]
    pub operation_ids: Vec<Vec<u8>>,
    #[serde(rename = "1")]
    pub version: i64,
}

/// Request to read a view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadViewRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub view_id: Vec<u8>,
}

/// Response from reading a view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadViewResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub view: Option<WireJjViewObject>,
}

/// Request to write a view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteViewRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub view: WireJjViewObject,
}

/// Response from writing a view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteViewResponse {
    #[serde(rename = "0")]
    pub view_id: Vec<u8>,
}

/// Request to read an operation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadOperationRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub operation_id: Vec<u8>,
}

/// Response from reading an operation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadOperationResponse {
    #[serde(default, skip_serializing_if = "is_default", rename = "0")]
    pub operation: Option<WireJjOperationObject>,
}

/// Request to write an operation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteOperationRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub operation: WireJjOperationObject,
}

/// Response from writing an operation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireWriteOperationResponse {
    #[serde(rename = "0")]
    pub operation_id: Vec<u8>,
}

/// Request to read op heads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadOpHeadsRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub workspace: String,
}

/// Response from reading op heads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReadOpHeadsResponse {
    #[serde(rename = "0")]
    pub state: WireJjOpHeadState,
}

/// Request to compare-and-swap op heads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCompareAndSwapOpHeadsRequest {
    #[serde(rename = "0")]
    pub repo: String,
    #[serde(rename = "1")]
    pub workspace: String,
    #[serde(default, skip_serializing_if = "is_default", rename = "2")]
    pub expected_operation_ids: Vec<Vec<u8>>,
    #[serde(default, skip_serializing_if = "is_default", rename = "3")]
    pub new_operation_ids: Vec<Vec<u8>>,
    #[serde(default, skip_serializing_if = "is_default", rename = "4")]
    pub transaction_id: Option<String>,
}

/// Response from compare-and-swap op heads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCompareAndSwapOpHeadsResponse {
    #[serde(rename = "0")]
    pub state: WireJjOpHeadState,
}

// Transparent ToWire/ToApi implementations.

impl crate::wire::ToWire for WireJjOpStorePayloadEncoding {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjOpStorePayloadEncoding {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjViewObject {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjViewObject {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjOperationObject {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjOperationObject {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireJjOpHeadState {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireJjOpHeadState {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadViewRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadViewRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadViewResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadViewResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteViewRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteViewRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteViewResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteViewResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadOperationRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadOperationRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadOperationResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadOperationResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteOperationRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteOperationRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireWriteOperationResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireWriteOperationResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadOpHeadsRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadOpHeadsRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireReadOpHeadsResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireReadOpHeadsResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireCompareAndSwapOpHeadsRequest {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireCompareAndSwapOpHeadsRequest {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}

impl crate::wire::ToWire for WireCompareAndSwapOpHeadsResponse {
    type Wire = Self;
    fn to_wire(self) -> Self::Wire {
        self
    }
}

impl crate::wire::ToApi for WireCompareAndSwapOpHeadsResponse {
    type Api = Self;
    type Error = std::convert::Infallible;
    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok(self)
    }
}
