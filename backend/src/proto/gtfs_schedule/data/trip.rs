#![allow(clippy::struct_field_names)]

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use super::WheelchairBoarding;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Trip {
    #[serde(alias = "trip_id")]
    #[sqlx(rename = "trip_id")]
    pub id: String,
    #[serde(alias = "route_id")]
    pub route_id: String,
    #[serde(alias = "service_id")]
    pub service_id: String,
    #[serde(alias = "trip_headsign")]
    #[sqlx(rename = "trip_headsign")]
    pub headsign: Option<String>,
    #[serde(alias = "trip_short_name")]
    #[sqlx(rename = "trip_short_name")]
    pub short_name: Option<String>,
    #[serde(alias = "direction_id")]
    pub direction_id: Option<Direction>,
    #[serde(alias = "block_id")]
    pub block_id: Option<String>,
    #[serde(alias = "shape_id")]
    pub shape_id: Option<String>,
    #[serde(default)]
    #[serde(alias = "wheelchair_accessible")]
    pub wheelchair_boarding: WheelchairBoarding,
    #[serde(default)]
    #[serde(alias = "bikes_allowed")]
    pub bikes_allowed: BikesAllowed,
    #[serde(default)]
    #[serde(alias = "stop_ids")]
    #[sqlx(skip)]
    pub stop_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr, sqlx::Type)]
#[repr(i16)]
pub enum Direction {
    Outbound = 0,
    Inbound = 1,
}

impl TryFrom<i16> for Direction {
    type Error = ();

    fn try_from(v: i16) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Outbound),
            1 => Ok(Self::Inbound),
            _ => Err(()),
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize_repr, Deserialize_repr, sqlx::Type,
)]
#[repr(i16)]
pub enum BikesAllowed {
    #[default]
    Unknown = 0,
    Allowed = 1,
    NotAllowed = 2,
}

impl TryFrom<i16> for BikesAllowed {
    type Error = ();

    fn try_from(v: i16) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Allowed),
            2 => Ok(Self::NotAllowed),
            _ => Err(()),
        }
    }
}
