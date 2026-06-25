use serde::de::{Deserialize, Deserializer};

/// Deserialize a PATCH field that may be absent, null, or a value.
/// - absent → `None` (do not update)
/// - null → `Some(None)` (clear)
/// - value → `Some(Some(value))`
pub fn deserialize_patch_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
