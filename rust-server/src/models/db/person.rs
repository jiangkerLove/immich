use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct PersonRow {
    pub id: Uuid,
    pub name: String,
    pub birth_date: Option<NaiveDate>,
    pub thumbnail_path: String,
    pub is_hidden: bool,
    pub is_favorite: bool,
    pub color: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub async fn search_by_name(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    name: &str,
    with_hidden: bool,
) -> Result<Vec<PersonRow>, sqlx::Error> {
    let mut query = String::from(
        r#"
            SELECT
                id,
                name,
                "birthDate" as birth_date,
                "thumbnailPath" as thumbnail_path,
                "isHidden" as is_hidden,
                "isFavorite" as is_favorite,
                color,
                "updatedAt" as updated_at
            FROM person
            WHERE "ownerId" = $1
              AND f_unaccent(name) %> f_unaccent($2)
        "#,
    );
    if !with_hidden {
        query.push_str(r#" AND "isHidden" = FALSE"#);
    }
    query.push_str(
        r#"
            ORDER BY f_unaccent(name) <->>> f_unaccent($2)
            LIMIT 100
        "#,
    );

    sqlx::query_as::<_, PersonRow>(&query)
        .bind(user_id)
        .bind(name)
        .fetch_all(pool)
        .await
}
