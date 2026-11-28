use crate::{
    entity::util::mixed_value::MixedValue,
    proto::{
        gtfs_realtime::data::transit_realtime::VehiclePosition, gtfs_schedule::data::geo::LatLng,
    },
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vehicle {
    #[serde(alias = "vehicle_id")]
    pub id: String,
    #[serde(alias = "route_id")]
    pub route_id: String,
    #[serde(alias = "trip_id")]
    pub trip_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_long_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trip_headsign: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearing: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stop_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stop_arrival_delay: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stop_arrival_time: Option<i64>,
    /// Shape points between the previous and the current position, in travel order.
    #[serde(skip)]
    pub path: Vec<LatLng>,
}

impl Vehicle {
    pub fn to_simple(&self) -> Vec<MixedValue> {
        vec![
            self.id.clone().into(),
            self.route_id.clone().into(),
            self.trip_id.clone().into(),
            self.latitude.into(),
            self.longitude.into(),
            self.prev_latitude
                .map_or(MixedValue::null(), MixedValue::F64),
            self.prev_longitude
                .map_or(MixedValue::null(), MixedValue::F64),
            self.next_stop_id
                .as_deref()
                .map_or(MixedValue::null(), MixedValue::from),
            self.next_stop_sequence
                .map_or(MixedValue::null(), MixedValue::U64),
            self.next_stop_arrival_delay
                .map_or(MixedValue::null(), MixedValue::I64),
            self.next_stop_arrival_time
                .map_or(MixedValue::null(), |v| MixedValue::U64(v.cast_unsigned())),
            self.bearing.map_or(MixedValue::null(), MixedValue::F64),
            self.route_long_name
                .as_deref()
                .map_or(MixedValue::null(), MixedValue::from),
            self.trip_headsign
                .as_deref()
                .map_or(MixedValue::null(), MixedValue::from),
            if self.path.is_empty() {
                MixedValue::null()
            } else {
                MixedValue::F64List(
                    self.path
                        .iter()
                        .flat_map(|point| [point.lng, point.lat])
                        .collect(),
                )
            },
        ]
    }
}

impl TryFrom<VehiclePosition> for Vehicle {
    type Error = VehicleError;

    fn try_from(value: VehiclePosition) -> Result<Self, Self::Error> {
        value.try_into()
    }
}

impl TryFrom<&VehiclePosition> for Vehicle {
    type Error = VehicleError;

    fn try_from(value: &VehiclePosition) -> Result<Self, Self::Error> {
        let Some(vehicle_info) = value.vehicle.as_ref() else {
            return Err(VehicleError::MissingVehicleInfo);
        };

        let Some(trip_info) = value.trip.as_ref() else {
            return Err(VehicleError::MissingTripInfo);
        };

        let Some(position_info) = value.position else {
            return Err(VehicleError::MissingPositionInfo);
        };

        Ok(Self {
            id: vehicle_info.id().to_string(),
            route_id: trip_info.route_id().to_string(),
            trip_id: trip_info.trip_id().to_string(),
            route_long_name: None,
            trip_headsign: None,
            latitude: f64::from(position_info.latitude),
            longitude: f64::from(position_info.longitude),
            bearing: position_info.bearing.map(f64::from),
            prev_latitude: None,
            prev_longitude: None,
            next_stop_id: None,
            next_stop_sequence: None,
            next_stop_arrival_delay: None,
            next_stop_arrival_time: None,
            path: Vec::new(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum VehicleError {
    #[error("Missing vehicle info")]
    MissingVehicleInfo,

    #[error("Missing trip info")]
    MissingTripInfo,

    #[error("Missing position info")]
    MissingPositionInfo,
}
