use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct AssetFaceWithPersonRow {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub person_id: Option<Uuid>,
    pub image_width: i32,
    pub image_height: i32,
    pub bounding_box_x1: i32,
    pub bounding_box_y1: i32,
    pub bounding_box_x2: i32,
    pub bounding_box_y2: i32,
    pub source_type: String,
    pub person_owner_id: Option<Uuid>,
    pub person_name: Option<String>,
    pub person_birth_date: Option<NaiveDate>,
    pub person_thumbnail_path: Option<String>,
    pub person_is_hidden: Option<bool>,
    pub person_is_favorite: Option<bool>,
    pub person_color: Option<String>,
    pub person_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct CreateAssetFaceData {
    pub person_id: Uuid,
    pub asset_id: Uuid,
    pub image_width: i32,
    pub image_height: i32,
    pub bounding_box_x1: i32,
    pub bounding_box_y1: i32,
    pub bounding_box_x2: i32,
    pub bounding_box_y2: i32,
}

const FACE_SELECT: &str = r#"
    af.id,
    af."assetId" AS asset_id,
    af."personId" AS person_id,
    af."imageWidth" AS image_width,
    af."imageHeight" AS image_height,
    af."boundingBoxX1" AS bounding_box_x1,
    af."boundingBoxY1" AS bounding_box_y1,
    af."boundingBoxX2" AS bounding_box_x2,
    af."boundingBoxY2" AS bounding_box_y2,
    af."sourceType"::text AS source_type,
    p."ownerId" AS person_owner_id,
    p.name AS person_name,
    p."birthDate" AS person_birth_date,
    p."thumbnailPath" AS person_thumbnail_path,
    p."isHidden" AS person_is_hidden,
    p."isFavorite" AS person_is_favorite,
    p.color AS person_color,
    p."updatedAt" AS person_updated_at
"#;

pub async fn get_faces_by_asset(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<AssetFaceWithPersonRow>, sqlx::Error> {
    let sql = format!(
        r#"
        SELECT {FACE_SELECT}
        FROM asset_face af
        LEFT JOIN person p ON p.id = af."personId"
        WHERE af."assetId" = $1
          AND af."deletedAt" IS NULL
          AND af."isVisible" = TRUE
        ORDER BY af."boundingBoxX1" ASC
        "#
    );
    sqlx::query_as::<_, AssetFaceWithPersonRow>(&sql)
        .bind(asset_id)
        .fetch_all(pool)
        .await
}

pub async fn get_face_by_id(
    pool: &Pool<Postgres>,
    face_id: &Uuid,
) -> Result<Option<AssetFaceWithPersonRow>, sqlx::Error> {
    let sql = format!(
        r#"
        SELECT {FACE_SELECT}
        FROM asset_face af
        LEFT JOIN person p ON p.id = af."personId"
        WHERE af.id = $1
          AND af."deletedAt" IS NULL
        "#
    );
    sqlx::query_as::<_, AssetFaceWithPersonRow>(&sql)
        .bind(face_id)
        .fetch_optional(pool)
        .await
}

pub async fn owner_owns_face(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    face_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM asset_face af
            INNER JOIN asset a ON a.id = af."assetId"
            WHERE af.id = $1
              AND a."ownerId" = $2
              AND a."deletedAt" IS NULL
        )
        "#,
    )
    .bind(face_id)
    .bind(owner_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn create_asset_face(
    pool: &Pool<Postgres>,
    data: &CreateAssetFaceData,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        INSERT INTO asset_face (
            "assetId", "personId", "imageWidth", "imageHeight",
            "boundingBoxX1", "boundingBoxY1", "boundingBoxX2", "boundingBoxY2",
            "sourceType"
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'manual')
        RETURNING id
        "#,
    )
    .bind(data.asset_id)
    .bind(data.person_id)
    .bind(data.image_width)
    .bind(data.image_height)
    .bind(data.bounding_box_x1)
    .bind(data.bounding_box_y1)
    .bind(data.bounding_box_x2)
    .bind(data.bounding_box_y2)
    .fetch_one(pool)
    .await
}

pub async fn delete_asset_face(pool: &Pool<Postgres>, face_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM asset_face WHERE id = $1"#)
        .bind(face_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn soft_delete_asset_face(pool: &Pool<Postgres>, face_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE asset_face SET "deletedAt" = now() WHERE id = $1 AND "deletedAt" IS NULL"#,
    )
    .bind(face_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_random_face_id(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT id
        FROM asset_face
        WHERE "personId" = $1
          AND "deletedAt" IS NULL
          AND "isVisible" = TRUE
        LIMIT 1
        "#,
    )
    .bind(person_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_person_face_asset_id(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT "faceAssetId" FROM person WHERE id = $1"#)
        .bind(person_id)
        .fetch_optional(pool)
        .await
}

pub async fn set_person_face_asset_id(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
    face_asset_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE person SET "faceAssetId" = $2 WHERE id = $1"#)
        .bind(person_id)
        .bind(face_asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn asset_has_edits(pool: &Pool<Postgres>, asset_id: &Uuid) -> Result<bool, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM asset_edit WHERE "assetId" = $1)"#,
    )
    .bind(asset_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn get_asset_scale_for_face(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<(i32, i32, i32, i32)>, sqlx::Error> {
    #[derive(FromRow)]
    struct Row {
        width: Option<i32>,
        height: Option<i32>,
        exif_image_width: Option<i32>,
        exif_image_height: Option<i32>,
    }

    let row = sqlx::query_as::<_, Row>(
        r#"
        SELECT
            a.width,
            a.height,
            e."exifImageWidth" AS exif_image_width,
            e."exifImageHeight" AS exif_image_height
        FROM asset a
        INNER JOIN asset_exif e ON e."assetId" = a.id
        WHERE a.id = $1
          AND a."deletedAt" IS NULL
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|value| {
        Some((
            value.width?,
            value.height?,
            value.exif_image_width?,
            value.exif_image_height?,
        ))
    }))
}

#[derive(Debug, sqlx::FromRow)]
pub struct FaceVisibilityRow {
    pub id: Uuid,
    pub bounding_box_x1: f32,
    pub bounding_box_y1: f32,
    pub bounding_box_x2: f32,
    pub bounding_box_y2: f32,
    pub image_width: i32,
    pub image_height: i32,
    pub is_visible: bool,
}

pub async fn list_for_visibility_by_asset(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<FaceVisibilityRow>, sqlx::Error> {
    sqlx::query_as::<_, FaceVisibilityRow>(
        r#"
        SELECT
            id,
            "boundingBoxX1" AS bounding_box_x1,
            "boundingBoxY1" AS bounding_box_y1,
            "boundingBoxX2" AS bounding_box_x2,
            "boundingBoxY2" AS bounding_box_y2,
            "imageWidth" AS image_width,
            "imageHeight" AS image_height,
            "isVisible" AS is_visible
        FROM asset_face
        WHERE "assetId" = $1
          AND "deletedAt" IS NULL
        ORDER BY "boundingBoxX1" ASC
        "#,
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
}

pub async fn update_visibilities(
    pool: &Pool<Postgres>,
    visible_ids: &[Uuid],
    hidden_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if visible_ids.is_empty() && hidden_ids.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;

    if !visible_ids.is_empty() {
        sqlx::query(
            r#"
            UPDATE asset_face
            SET "isVisible" = true
            WHERE id = ANY($1)
            "#,
        )
        .bind(visible_ids)
        .execute(&mut *tx)
        .await?;
    }

    if !hidden_ids.is_empty() {
        sqlx::query(
            r#"
            UPDATE asset_face
            SET "isVisible" = false
            WHERE id = ANY($1)
            "#,
        )
        .bind(hidden_ids)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}
