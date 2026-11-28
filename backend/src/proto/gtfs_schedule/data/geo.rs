//! Planar geometry for short distances around a reference point.

pub const METRES_PER_DEGREE: f64 = 111_320.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

/// Local planar coordinates in metres around a reference point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metres {
    pub east: f64,
    pub north: f64,
}

impl Metres {
    pub const ORIGIN: Self = Self {
        east: 0.0,
        north: 0.0,
    };

    pub fn from_lat_lng(point: LatLng, origin: LatLng) -> Self {
        Self {
            east: (point.lng - origin.lng) * origin.lat.to_radians().cos() * METRES_PER_DEGREE,
            north: (point.lat - origin.lat) * METRES_PER_DEGREE,
        }
    }

    pub fn to_lat_lng(self, origin: LatLng) -> LatLng {
        LatLng {
            lat: origin.lat + self.north / METRES_PER_DEGREE,
            lng: origin.lng + self.east / (origin.lat.to_radians().cos() * METRES_PER_DEGREE),
        }
    }

    pub fn lerp(self, to: Self, t: f64) -> Self {
        Self {
            east: (to.east - self.east).mul_add(t, self.east),
            north: (to.north - self.north).mul_add(t, self.north),
        }
    }

    pub fn distance(self, to: Self) -> f64 {
        (to.east - self.east).hypot(to.north - self.north)
    }

    /// Unit vector from `self` to `to`, or `None` if the points are equal.
    pub fn direction_to(self, to: Self) -> Option<(f64, f64)> {
        let length = self.distance(to);
        (length > 0.0).then(|| {
            (
                (to.east - self.east) / length,
                (to.north - self.north) / length,
            )
        })
    }
}

/// Compass bearing in whole degrees: 0 is north, 90 is east. Range is `[0, 360)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bearing(pub(super) f64);

impl Bearing {
    pub fn from_vector(east: f64, north: f64) -> Self {
        Self(east.atan2(north).to_degrees().rem_euclid(360.0).round() % 360.0)
    }

    pub const fn degrees(self) -> f64 {
        self.0
    }
}
