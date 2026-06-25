/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

pub mod bookmark;
pub mod commit;
pub mod file;
pub mod opstore;
pub mod tree;
pub mod workspace;

#[derive(Copy, Clone, Debug, Error)]
#[error("invalid byte slice length, expected {expected_len} found {found_len}")]
pub struct TryFromBytesError {
    pub expected_len: usize,
    pub found_len: usize,
}

#[derive(Debug, Error)]
#[error("Failed to convert from wire to API representation")]
pub enum WireToApiConversionError {
    #[error("Unrecognized enum variant: {0}")]
    UnrecognizedEnumVariant(&'static str),
    #[error("Cannot populate required field: {0}")]
    CannotPopulateRequiredField(&'static str),
    #[error("Missing field: {0}")]
    MissingField(&'static str),
}

impl From<Infallible> for WireToApiConversionError {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

/// Convert from an API type to Wire type.
pub trait ToWire: Sized {
    type Wire: ToApi<Api = Self> + serde::Serialize + serde::de::DeserializeOwned;

    fn to_wire(self) -> Self::Wire;
}

/// Convert from a Wire type to API type.
pub trait ToApi: Send + Sized {
    type Api: ToWire<Wire = Self>;
    type Error: Into<WireToApiConversionError> + Send + Sync + std::error::Error;

    fn to_api(self) -> Result<Self::Api, Self::Error>;
}

impl<A: ToWire> ToWire for Vec<A> {
    type Wire = Vec<<A as ToWire>::Wire>;

    fn to_wire(self) -> Self::Wire {
        self.into_iter().map(|v| v.to_wire()).collect()
    }
}

impl<W: ToApi> ToApi for Vec<W> {
    type Api = Vec<<W as ToApi>::Api>;
    type Error = <W as ToApi>::Error;

    fn to_api(self) -> Result<Self::Api, Self::Error> {
        self.into_iter().map(|v| v.to_api()).collect()
    }
}

impl<A: ToWire, B: ToWire> ToWire for (A, B) {
    type Wire = (<A as ToWire>::Wire, <B as ToWire>::Wire);

    fn to_wire(self) -> Self::Wire {
        (self.0.to_wire(), self.1.to_wire())
    }
}

impl<A: ToApi, B: ToApi> ToApi for (A, B) {
    type Api = (<A as ToApi>::Api, <B as ToApi>::Api);
    type Error = WireToApiConversionError;

    fn to_api(self) -> Result<Self::Api, Self::Error> {
        Ok((
            self.0.to_api().map_err(|e| e.into())?,
            self.1.to_api().map_err(|e| e.into())?,
        ))
    }
}

impl<A: ToWire> ToWire for Option<A> {
    type Wire = Option<<A as ToWire>::Wire>;

    fn to_wire(self) -> Self::Wire {
        self.map(|a| a.to_wire())
    }
}

impl<W: ToApi> ToApi for Option<W> {
    type Api = Option<<W as ToApi>::Api>;
    type Error = <W as ToApi>::Error;

    fn to_api(self) -> Result<Self::Api, Self::Error> {
        self.map(|w| w.to_api()).transpose()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WireMap<K, V>(Vec<(K, V)>);

impl<K: ToWire + Eq + std::hash::Hash + Ord, V: ToWire> ToWire for HashMap<K, V> {
    type Wire = WireMap<<K as ToWire>::Wire, <V as ToWire>::Wire>;

    fn to_wire(self) -> Self::Wire {
        let iter = self.into_iter();
        #[cfg(test)]
        let iter = std::collections::BTreeMap::from_iter(iter).into_iter();
        WireMap(iter.map(|(k, v)| (k.to_wire(), v.to_wire())).collect())
    }
}

impl<K: ToApi, V: ToApi> ToApi for WireMap<K, V>
where
    <K as ToApi>::Api: Eq + std::hash::Hash + Ord,
{
    type Api = HashMap<<K as ToApi>::Api, <V as ToApi>::Api>;
    type Error = WireToApiConversionError;

    fn to_api(self) -> Result<Self::Api, Self::Error> {
        self.0
            .into_iter()
            .map(|(k, v)| {
                Ok((
                    k.to_api().map_err(|e| e.into())?,
                    v.to_api().map_err(|e| e.into())?,
                ))
            })
            .collect()
    }
}

macro_rules! transparent_wire {
    ( $($name: ty),* $(,)? ) => {
        $(
        impl ToWire for $name {
            type Wire = $name;

            fn to_wire(self) -> Self::Wire {
                self
            }
        }

        impl ToApi for $name {
            type Api = $name;
            type Error = std::convert::Infallible;

            fn to_api(self) -> Result<Self::Api, Self::Error> {
                Ok(self)
            }
        }
        )*
    }
}

transparent_wire!(
    bool,
    u8,
    i8,
    u16,
    i16,
    u32,
    i32,
    u64,
    i64,
    usize,
    isize,
    bytes::Bytes,
    String,
    (),
);

/// Check whether a value is equal to its default.
pub(crate) fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    v == &T::default()
}

/// Fixed-size byte array wrapper for wire serialization.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WireId<const N: usize>([u8; N]);

impl<const N: usize> Default for WireId<N> {
    fn default() -> Self {
        Self([0u8; N])
    }
}

impl<const N: usize> Serialize for WireId<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde_bytes::Bytes::new(&self.0).serialize(serializer)
    }
}

impl<'de, const N: usize> Deserialize<'de> for WireId<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
        if bytes.len() != N {
            return Err(serde::de::Error::custom(format!(
                "expected {N} bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; N];
        arr.copy_from_slice(&bytes);
        Ok(WireId(arr))
    }
}

impl<const N: usize> From<[u8; N]> for WireId<N> {
    fn from(v: [u8; N]) -> Self {
        Self(v)
    }
}

impl<const N: usize> From<WireId<N>> for [u8; N] {
    fn from(v: WireId<N>) -> Self {
        v.0
    }
}

impl<const N: usize> fmt::Display for WireId<N> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(fmt, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl<const N: usize> WireId<N> {
    /// Return a hex-encoded string of this id.
    pub fn hex(&self) -> String {
        self.to_string()
    }
}

#[cfg(any(test, feature = "for-tests"))]
impl<const N: usize> quickcheck::Arbitrary for WireId<N> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut arr = [0u8; N];
        for i in 0..N {
            arr[i] = u8::arbitrary(g);
        }
        WireId(arr)
    }
}
