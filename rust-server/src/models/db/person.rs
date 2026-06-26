use std::collections::HashMap;

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

#[derive(Debug, FromRow)]
struct AssetPersonRow {
    asset_id: Uuid,
    id: Uuid,
    name: String,
    birth_date: Option<NaiveDate>,
    thumbnail_path: String,
    is_hidden: bool,
    is_favorite: bool,
    color: Option<String>,
    updated_at: DateTime<Utc>,
}

pub async fn get_people_by_asset_ids(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<PersonRow>>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query_as::<_, AssetPersonRow>(
        r#"
            SELECT DISTINCT ON (af."assetId", p.id)
                af."assetId" as asset_id,
                p.id,
                p.name,
                p."birthDate" as birth_date,
                p."thumbnailPath" as thumbnail_path,
                p."isHidden" as is_hidden,
                p."isFavorite" as is_favorite,
                p.color,
                p."updatedAt" as updated_at
            FROM asset_face af
            INNER JOIN person p ON p.id = af."personId"
            WHERE af."assetId" = ANY($1)
              AND af."deletedAt" IS NULL
              AND af."isVisible" = TRUE
            ORDER BY af."assetId", p.id
        "#,
    )
    .bind(asset_ids)
    .fetch_all(pool)
    .await?;

    let mut map: HashMap<Uuid, Vec<PersonRow>> = HashMap::new();
    for row in rows {
        map.entry(row.asset_id).or_default().push(PersonRow {
            id: row.id,
            name: row.name,
            birth_date: row.birth_date,
            thumbnail_path: row.thumbnail_path,
            is_hidden: row.is_hidden,
            is_favorite: row.is_favorite,
            color: row.color,
            updated_at: row.updated_at,
        });
    }
    Ok(map)
}

const PERSON_SELECT: &str = r#"
    id,
    name,
    "birthDate" as birth_date,
    "thumbnailPath" as thumbnail_path,
    "isHidden" as is_hidden,
    "isFavorite" as is_favorite,
    color,
    "updatedAt" as updated_at
"#;

pub async fn get_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(&format!(
        r#"SELECT {PERSON_SELECT} FROM person WHERE id = $1"#
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_by_id_for_owner(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    id: &Uuid,
) -> Result<Option<PersonRow>, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(&format!(
        r#"SELECT {PERSON_SELECT} FROM person WHERE id = $1 AND "ownerId" = $2"#
    ))
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await
}

pub async fn list_for_user(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    with_hidden: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<PersonRow>, sqlx::Error> {
    let mut query = format!(
        r#"
            SELECT {PERSON_SELECT}
            FROM person
            WHERE "ownerId" = $1
        "#
    );
    if !with_hidden {
        query.push_str(r#" AND "isHidden" = FALSE"#);
    }
    query.push_str(
        r#"
            ORDER BY "isHidden" ASC, "isFavorite" DESC,
                     NULLIF(name, '') ASC NULLS LAST, "createdAt" ASC
            LIMIT $2 OFFSET $3
        "#,
    );

    sqlx::query_as::<_, PersonRow>(&query)
        .bind(owner_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

#[derive(Debug, FromRow)]
pub struct PersonCountRow {
    pub total: i64,
    pub hidden: i64,
}

pub async fn count_for_user(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
) -> Result<PersonCountRow, sqlx::Error> {
    sqlx::query_as::<_, PersonCountRow>(
        r#"
            SELECT
                COUNT(*)::bigint as total,
                COUNT(*) FILTER (WHERE "isHidden" = TRUE)::bigint as hidden
            FROM person
            WHERE "ownerId" = $1
        "#,
    )
    .bind(owner_id)
    .fetch_one(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct PersonStatisticsRow {
    pub assets: i64,
}

pub async fn get_statistics(pool: &Pool<Postgres>, person_id: &Uuid) -> Result<PersonStatisticsRow, sqlx::Error> {
    sqlx::query_as::<_, PersonStatisticsRow>(
        r#"
            SELECT COUNT(DISTINCT a.id)::bigint as assets
            FROM asset_face af
            LEFT JOIN asset a ON a.id = af."assetId"
                AND a.visibility = 'timeline'
                AND a."deletedAt" IS NULL
            WHERE af."personId" = $1
              AND af."deletedAt" IS NULL
              AND af."isVisible" = TRUE
        "#,
    )
    .bind(person_id)
    .fetch_one(pool)
    .await
}

pub async fn create(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    name: Option<&str>,
    birth_date: Option<NaiveDate>,
    is_hidden: Option<bool>,
    is_favorite: Option<bool>,
    color: Option<&str>,
) -> Result<PersonRow, sqlx::Error> {
    sqlx::query_as::<_, PersonRow>(&format!(
        r#"
            INSERT INTO person ("ownerId", name, "birthDate", "isHidden", "isFavorite", color)
            VALUES ($1, COALESCE($2, ''), $3, COALESCE($4, FALSE), COALESCE($5, FALSE), $6)
            RETURNING {PERSON_SELECT}
        "#
    ))
    .bind(owner_id)
    .bind(name)
    .bind(birth_date)
    .bind(is_hidden)
    .bind(is_favorite)
    .bind(color)
    .fetch_one(pool)
    .await
}

pub async fn update(
    pool: &Pool<Postgres>,
    id: &Uuid,
    owner_id: &Uuid,
    name: Option<&str>,
    birth_date: Option<Option<NaiveDate>>,
    is_hidden: Option<bool>,
    is_favorite: Option<bool>,
    color: Option<Option<&str>>,
    update_face_asset_id: Option<Option<Uuid>>,
) -> Result<PersonRow, sqlx::Error> {
    let current = get_by_id_for_owner(pool, owner_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;

    let name = name.unwrap_or(&current.name);
    let birth_date = match birth_date {
        Some(value) => value,
        None => current.birth_date,
    };
    let is_hidden = is_hidden.unwrap_or(current.is_hidden);
    let is_favorite = is_favorite.unwrap_or(current.is_favorite);
    let color = match color {
        Some(value) => value.map(|s| s.to_string()),
        None => current.color.clone(),
    };

    match update_face_asset_id {
        Some(Some(face_asset_id)) => {
            sqlx::query(
                r#"
                    UPDATE person
                    SET name = $1, "birthDate" = $2, "isHidden" = $3,
                        "isFavorite" = $4, color = $5, "faceAssetId" = $6
                    WHERE id = $7 AND "ownerId" = $8
                "#,
            )
            .bind(name)
            .bind(birth_date)
            .bind(is_hidden)
            .bind(is_favorite)
            .bind(color)
            .bind(face_asset_id)
            .bind(id)
            .bind(owner_id)
            .execute(pool)
            .await?;
        }
        Some(None) => {
            sqlx::query(
                r#"
                    UPDATE person
                    SET name = $1, "birthDate" = $2, "isHidden" = $3,
                        "isFavorite" = $4, color = $5, "faceAssetId" = NULL
                    WHERE id = $6 AND "ownerId" = $7
                "#,
            )
            .bind(name)
            .bind(birth_date)
            .bind(is_hidden)
            .bind(is_favorite)
            .bind(color)
            .bind(id)
            .bind(owner_id)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(
                r#"
                    UPDATE person
                    SET name = $1, "birthDate" = $2, "isHidden" = $3,
                        "isFavorite" = $4, color = $5
                    WHERE id = $6 AND "ownerId" = $7
                "#,
            )
            .bind(name)
            .bind(birth_date)
            .bind(is_hidden)
            .bind(is_favorite)
            .bind(color)
            .bind(id)
            .bind(owner_id)
            .execute(pool)
            .await?;
        }
    }

    get_by_id_for_owner(pool, owner_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn delete_for_owner(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"DELETE FROM person WHERE id = ANY($1) AND "ownerId" = $2"#,
    )
    .bind(ids)
    .bind(owner_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn owner_owns_people(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    ids: &[Uuid],
) -> Result<bool, sqlx::Error> {
    if ids.is_empty() {
        return Ok(true);
    }
    let count: i64 = sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM person
            WHERE id = ANY($1) AND "ownerId" = $2
        "#,
    )
    .bind(ids)
    .bind(owner_id)
    .fetch_one(pool)
    .await?;
    Ok(count as usize == ids.len())
}

pub async fn get_face_id_for_asset(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
    asset_id: &Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id
            FROM asset_face
            WHERE "personId" = $1
              AND "assetId" = $2
              AND "deletedAt" IS NULL
            LIMIT 1
        "#,
    )
    .bind(person_id)
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_face_id_for_feature_update(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
    asset_id: &Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT af.id
            FROM asset_face af
            INNER JOIN asset a ON a.id = af."assetId"
            WHERE af."personId" = $1
              AND af."assetId" = $2
              AND af."deletedAt" IS NULL
              AND a."isOffline" = FALSE
            LIMIT 1
        "#,
    )
    .bind(person_id)
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn reassign_face(
    pool: &Pool<Postgres>,
    face_id: &Uuid,
    new_person_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE asset_face SET "personId" = $2 WHERE id = $1"#,
    )
    .bind(face_id)
    .bind(new_person_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reassign_faces_by_person(
    pool: &Pool<Postgres>,
    old_person_id: &Uuid,
    new_person_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE asset_face SET "personId" = $2 WHERE "personId" = $1"#,
    )
    .bind(old_person_id)
    .bind(new_person_id)
    .execute(pool)
    .await?;
    Ok(())
}
