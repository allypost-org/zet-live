#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum MixedValue {
    Null(Null),
    String(String),
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    F64List(Vec<f64>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Null;

impl serde::Serialize for Null {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_none()
    }
}

impl MixedValue {
    pub const fn null() -> Self {
        Self::Null(Null)
    }
}

impl From<String> for MixedValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<&str> for MixedValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<u32> for MixedValue {
    fn from(value: u32) -> Self {
        Self::U32(value)
    }
}
impl From<u64> for MixedValue {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}
impl From<i32> for MixedValue {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}
impl From<f32> for MixedValue {
    fn from(value: f32) -> Self {
        Self::F32(value)
    }
}
impl From<f64> for MixedValue {
    fn from(value: f64) -> Self {
        Self::F64(value)
    }
}
