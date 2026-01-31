//! Time utilities for handling and converting timestamps.
use chrono::{DateTime, Local};
use serde::{self, Deserialize, Deserializer, Serializer};
use std::time::SystemTime;

/// Serializes a `SystemTime` to an RFC 3339 string.
pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let datetime: DateTime<Local> = (*time).into();
    serializer.serialize_str(&datetime.to_rfc3339())
}

/// Deserializes an RFC 3339 string to a `SystemTime`.
pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    DateTime::parse_from_rfc3339(&s)
        .map(SystemTime::from)
        .map_err(serde::de::Error::custom)
}

/// A module for serializing and deserializing `Option<SystemTime>`.
pub mod option {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializes an `Option<SystemTime>` to an RFC 3339 string.
    pub fn serialize<S>(time: &Option<SystemTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match time {
            Some(t) => {
                let datetime: DateTime<Local> = (*t).into();
                serializer.serialize_some(&datetime.to_rfc3339())
            }
            None => serializer.serialize_none(),
        }
    }

    /// Deserializes an `Option` from an RFC 3339 string to `Option<SystemTime>`.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SystemTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Temp(#[serde(with = "super")] SystemTime);

        let opt: Option<Temp> = Option::deserialize(deserializer)?;
        Ok(opt.map(|Temp(st)| st))
    }
}
