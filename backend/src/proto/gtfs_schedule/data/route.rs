#![allow(clippy::struct_field_names)]

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    #[serde(alias = "route_id")]
    pub id: String,
    #[serde(alias = "agency_id", default)]
    pub agency_id: Option<String>,
    #[serde(alias = "route_short_name", default)]
    pub short_name: Option<String>,
    #[serde(alias = "route_long_name", default)]
    pub long_name: Option<String>,
    #[serde(alias = "route_desc", default)]
    pub desc: Option<String>,
    #[serde(alias = "route_type", default)]
    pub route_type: Option<RouteType>,
    #[serde(alias = "route_url")]
    pub url: Option<url::Url>,
    #[serde(alias = "route_color", default = "Route::default_route_color")]
    pub color: String,
    #[serde(
        alias = "route_text_color",
        default = "Route::default_route_text_color"
    )]
    pub text_color: String,
    #[serde(alias = "route_sort_order", default)]
    #[sqlx(skip)]
    pub sort_order: Option<u32>,
    #[serde(alias = "continuous_pickup", default)]
    #[sqlx(skip)]
    pub continuous_pickup: PickupType,
    #[serde(alias = "continuous_drop_off", default)]
    #[sqlx(skip)]
    pub continuous_drop_off: DropOffType,
    #[serde(alias = "network_id", default)]
    #[sqlx(skip)]
    pub network_id: Option<String>,
}
impl Route {
    pub fn default_route_color() -> String {
        "FFFFFF".to_string()
    }

    pub fn default_route_text_color() -> String {
        "000000".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr, sqlx::Type)]
#[repr(i16)]
pub enum RouteType {
    /// Tram, Streetcar, Light rail. Any light rail or street level system within a metropolitan area.
    Tram = 0,
    /// Subway, Metro. Any underground rail system within a metropolitan area.
    Subway = 1,
    /// Rail. Used for intercity and long-distance travel.
    Rail = 2,
    /// Bus. Used for short- and long-distance bus routes.
    Bus = 3,
    /// Ferry. Used for short- and long-distance boat service.
    Ferry = 4,
    /// Cable tram. Used for street-level rail cars where the cable runs beneath the vehicle (e.g., cable car in San Francisco).
    CableTram = 5,
    /// Aerial lift, suspended cable car (e.g., gondola lift, aerial tramway). Cable transport where cabins, cars, gondolas or open chairs are suspended by means of one or more cables.
    Gondola = 6,
    /// Funicular. Any rail system designed for steep inclines.
    Funicular = 7,
    /// Trolleybus. Electric buses that draw power from overhead wires using poles.
    Trolley = 11,
    /// Monorail. Railway in which the track consists of a single rail or a beam.
    Monorail = 12,
}

impl TryFrom<i16> for RouteType {
    type Error = ();

    fn try_from(v: i16) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Tram),
            1 => Ok(Self::Subway),
            2 => Ok(Self::Rail),
            3 => Ok(Self::Bus),
            4 => Ok(Self::Ferry),
            5 => Ok(Self::CableTram),
            6 => Ok(Self::Gondola),
            7 => Ok(Self::Funicular),
            11 => Ok(Self::Trolley),
            12 => Ok(Self::Monorail),
            _ => Err(()),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize_repr, Deserialize_repr, sqlx::Type,
)]
#[repr(i16)]
pub enum PickupType {
    /// Continuous stopping pickup.
    Continuous = 0,
    /// No continuous stopping pickup.
    #[default]
    None = 1,
    /// Must phone agency to arrange continuous stopping pickup.
    CallAgency = 2,
    /// Must coordinate with driver to arrange continuous stopping pickup.
    CoordinateWithDriver = 3,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize_repr, Deserialize_repr, sqlx::Type,
)]
#[repr(i16)]
pub enum DropOffType {
    /// Continuous stopping drop off.
    Continuous = 0,
    /// No continuous stopping drop off.
    #[default]
    None = 1,
    /// Must coordinate with driver to arrange continuous stopping drop off.
    CoordinateWithDriver = 2,
}
