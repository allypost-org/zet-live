#![allow(clippy::struct_field_names)]

use serde::{Deserialize, Serialize};

use super::FileData;
use crate::proto::gtfs_schedule::data::BulkInsert;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    #[serde(alias = "shape_id")]
    #[sqlx(rename = "shape_id")]
    pub id: String,
    #[serde(alias = "shape_pt_lat")]
    #[sqlx(rename = "shape_pt_lat")]
    pub latitude: f64,
    #[serde(alias = "shape_pt_lon")]
    #[sqlx(rename = "shape_pt_lon")]
    pub longitude: f64,
    #[serde(alias = "shape_pt_sequence")]
    #[sqlx(rename = "shape_pt_sequence")]
    pub sequence: i32,
    #[serde(alias = "shape_dist_traveled")]
    #[sqlx(rename = "shape_dist_traveled")]
    pub distance: Option<f64>,
}

impl FileData for Shape {
    fn file_name() -> &'static str {
        "shapes.txt"
    }

    fn table_name() -> &'static str {
        "gtfs_shapes"
    }

    fn into_bulk_insert(self) -> BulkInsert {
        BulkInsert::Shape(self)
    }
}
