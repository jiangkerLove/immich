use sqlx::{Postgres, QueryBuilder};

pub fn parse_query_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// Bind a text value compared against `asset_visibility_enum` columns.
pub fn push_visibility_enum_eq(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    visibility: impl Into<String>,
) {
    query.push(format!(" {column} = "));
    query.push_bind(visibility.into());
    query.push("::asset_visibility_enum");
}

