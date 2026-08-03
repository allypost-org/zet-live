use serde::{Deserialize, Serialize};

use super::{DropOffType, FileData, PickupType};
use crate::proto::gtfs_schedule::data::BulkInsert;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopTime {
    #[serde(alias = "trip_id")]
    pub trip_id: String,
    #[serde(alias = "arrival_time")]
    pub arrival_time: Option<String>,
    #[serde(alias = "departure_time")]
    pub departure_time: Option<String>,
    #[serde(alias = "stop_id")]
    pub stop_id: String,
    #[serde(alias = "stop_sequence")]
    pub stop_sequence: i32,
    #[serde(alias = "stop_headsign")]
    pub stop_headsign: Option<String>,
    #[serde(alias = "pickup_type")]
    pub pickup_type: Option<PickupType>,
    #[serde(alias = "drop_off_type")]
    pub drop_off_type: Option<DropOffType>,
    #[serde(alias = "shape_dist_traveled")]
    pub shape_dist_traveled: Option<f32>,
}

impl FileData for StopTime {
    fn file_name() -> &'static str {
        "stop_times.txt"
    }

    fn table_name() -> &'static str {
        "gtfs_stop_times"
    }

    fn into_bulk_insert(self) -> BulkInsert {
        BulkInsert::StopTime(self)
    }
}
