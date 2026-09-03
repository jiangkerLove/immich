use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

use super::person_schema::PersonSchema;

const DUPLICATE_SEARCH_LIMIT: i64 = 64;

#[derive(Debug, Clone, FromRow)]
pub struct OcrAssetRow {
    pub visibility: String,
    pub preview_path: Option<String>,
}

pub async fn get_for_ocr(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<OcrAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, OcrAssetRow>(
        r#"
            SELECT
                asset.visibility,
                (
                    SELECT asset_file.path
                    FROM asset_file
                    WHERE asset_file."assetId" = asset.id
                      AND asset_file.type = 'preview'
                      AND asset_file."isEdited" = false
                    LIMIT 1
                ) AS preview_path
            FROM asset
            WHERE asset.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn stream_for_ocr(pool: &Pool<Postgres>, force: bool) -> Result<Vec<Uuid>, sqlx::Error> {
    if force {
        return sqlx::query_scalar(
            r#"
                SELECT asset.id
                FROM asset
                WHERE asset."deletedAt" IS NULL
                  AND asset.visibility != 'hidden'
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(
        r#"
            SELECT asset.id
            FROM asset
            INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
            WHERE asset_job_status."ocrAt" IS NULL
              AND asset."deletedAt" IS NULL
              AND asset.visibility != 'hidden'
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn set_ocr_at(pool: &Pool<Postgres>, asset_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO asset_job_status ("assetId", "ocrAt")
            VALUES ($1, NOW())
            ON CONFLICT ("assetId") DO UPDATE SET "ocrAt" = EXCLUDED."ocrAt"
        "#,
    )
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
pub struct DuplicateSearchAssetRow {
    pub id: Uuid,
    pub asset_type: String,
    pub owner_id: Uuid,
    pub duplicate_id: Option<Uuid>,
    pub stack_id: Option<Uuid>,
    pub visibility: String,
    pub embedding: Option<String>,
}

pub async fn get_for_duplicate_search(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<DuplicateSearchAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, DuplicateSearchAssetRow>(
        r#"
            SELECT
                asset.id,
                asset.type AS asset_type,
                asset."ownerId" AS owner_id,
                asset."duplicateId" AS duplicate_id,
                asset."stackId" AS stack_id,
                asset.visibility,
                smart_search.embedding::text AS embedding
            FROM asset
            LEFT JOIN smart_search ON asset.id = smart_search."assetId"
            WHERE asset.id = $1
            LIMIT 1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn stream_for_duplicate_search(
    pool: &Pool<Postgres>,
    force: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if force {
        return sqlx::query_scalar(
            r#"
                SELECT asset.id
                FROM asset
                INNER JOIN smart_search ON asset.id = smart_search."assetId"
                INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
                WHERE asset."deletedAt" IS NULL
                  AND asset.visibility IN ('archive', 'timeline')
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(
        r#"
            SELECT asset.id
            FROM asset
            INNER JOIN smart_search ON asset.id = smart_search."assetId"
            INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
            WHERE asset."deletedAt" IS NULL
              AND asset.visibility IN ('archive', 'timeline')
              AND asset_job_status."duplicatesDetectedAt" IS NULL
        "#,
    )
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, FromRow)]
pub struct DuplicateMatchRow {
    pub asset_id: Uuid,
    pub duplicate_id: Option<Uuid>,
    pub distance: f64,
}

pub async fn search_duplicate_assets(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    embedding: &str,
    max_distance: f64,
    asset_type: &str,
    owner_ids: &[Uuid],
) -> Result<Vec<DuplicateMatchRow>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL vchordrq.probes = 1")
        .execute(&mut *tx)
        .await
        .ok();

    let rows = sqlx::query_as::<_, DuplicateMatchRow>(
        r#"
            WITH cte AS (
                SELECT
                    asset.id AS asset_id,
                    asset."duplicateId" AS duplicate_id,
                    smart_search.embedding <=> $1::vector AS distance
                FROM asset
                INNER JOIN smart_search ON asset.id = smart_search."assetId"
                WHERE asset."ownerId" = ANY($2::uuid[])
                  AND asset."deletedAt" IS NULL
                  AND asset.type = $3
                  AND asset.id != $4
                  AND asset."stackId" IS NULL
                  AND asset.visibility IN ('archive', 'timeline', 'locked')
                ORDER BY distance
                LIMIT $5
            )
            SELECT asset_id, duplicate_id, distance
            FROM cte
            WHERE distance <= $6
        "#,
    )
    .bind(embedding)
    .bind(owner_ids)
    .bind(asset_type)
    .bind(asset_id)
    .bind(DUPLICATE_SEARCH_LIMIT)
    .bind(max_distance)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(rows)
}

pub async fn merge_duplicate_group(
    pool: &Pool<Postgres>,
    target_id: &Uuid,
    asset_ids: &[Uuid],
    source_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() && source_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
            UPDATE asset
            SET "duplicateId" = $1
            WHERE id = ANY($2::uuid[])
               OR "duplicateId" = ANY($3::uuid[])
        "#,
    )
    .bind(target_id)
    .bind(asset_ids)
    .bind(source_ids)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_duplicate_id(pool: &Pool<Postgres>, asset_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE asset SET "duplicateId" = NULL WHERE id = $1"#)
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_duplicates_detected_at(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
            INSERT INTO asset_job_status ("assetId", "duplicatesDetectedAt")
            SELECT id, NOW()
            FROM asset
            WHERE id = ANY($1)
            ON CONFLICT ("assetId") DO UPDATE
            SET "duplicatesDetectedAt" = EXCLUDED."duplicatesDetectedAt"
        "#,
    )
    .bind(asset_ids)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetectFaceAssetFace {
    pub id: Uuid,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "imageWidth")]
    pub image_width: i32,
    #[serde(rename = "imageHeight")]
    pub image_height: i32,
    #[serde(rename = "boundingBoxX1")]
    pub bounding_box_x1: i32,
    #[serde(rename = "boundingBoxY1")]
    pub bounding_box_y1: i32,
    #[serde(rename = "boundingBoxX2")]
    pub bounding_box_x2: i32,
    #[serde(rename = "boundingBoxY2")]
    pub bounding_box_y2: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct DetectFacesAssetRow {
    pub id: Uuid,
    pub visibility: String,
    pub preview_path: Option<String>,
    pub preview_file_count: i64,
    pub faces: Option<serde_json::Value>,
}

pub async fn get_for_detect_faces(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<DetectFacesAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, DetectFacesAssetRow>(
        r#"
            SELECT
                asset.id,
                asset.visibility,
                (
                    SELECT asset_file.path
                    FROM asset_file
                    WHERE asset_file."assetId" = asset.id
                      AND asset_file.type = 'preview'
                      AND asset_file."isEdited" = false
                    LIMIT 1
                ) AS preview_path,
                (
                    SELECT COUNT(*)::bigint
                    FROM asset_file
                    WHERE asset_file."assetId" = asset.id
                      AND asset_file.type = 'preview'
                      AND asset_file."isEdited" = false
                ) AS preview_file_count,
                (
                    SELECT COALESCE(json_agg(af), '[]'::json)
                    FROM (
                        SELECT
                            asset_face.id,
                            asset_face."sourceType",
                            asset_face."imageWidth",
                            asset_face."imageHeight",
                            asset_face."boundingBoxX1",
                            asset_face."boundingBoxY1",
                            asset_face."boundingBoxX2",
                            asset_face."boundingBoxY2"
                        FROM asset_face
                        WHERE asset_face."assetId" = asset.id
                          AND asset_face."deletedAt" IS NULL
                    ) AS af
                ) AS faces
            FROM asset
            INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
            WHERE asset.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn stream_for_detect_faces(
    pool: &Pool<Postgres>,
    force: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if force {
        return sqlx::query_scalar(
            r#"
                SELECT asset.id
                FROM asset
                INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
                WHERE asset."deletedAt" IS NULL
                  AND asset.visibility != 'hidden'
                  AND EXISTS (
                    SELECT 1
                    FROM asset_file
                    WHERE "assetId" = asset.id AND type = 'preview'
                  )
                ORDER BY asset."fileCreatedAt" DESC
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(
        r#"
            SELECT asset.id
            FROM asset
            INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
            WHERE asset."deletedAt" IS NULL
              AND asset.visibility != 'hidden'
              AND asset_job_status."facesRecognizedAt" IS NULL
              AND EXISTS (
                SELECT 1
                FROM asset_file
                WHERE "assetId" = asset.id AND type = 'preview'
              )
            ORDER BY asset."fileCreatedAt" DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn set_faces_recognized_at(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO asset_job_status ("assetId", "facesRecognizedAt")
            VALUES ($1, NOW())
            ON CONFLICT ("assetId") DO UPDATE SET "facesRecognizedAt" = EXCLUDED."facesRecognizedAt"
        "#,
    )
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
pub struct FacialRecognitionFaceRow {
    pub id: Uuid,
    pub person_id: Option<Uuid>,
    pub source_type: String,
    pub owner_id: Uuid,
    pub visibility: String,
    pub file_created_at: DateTime<Utc>,
    pub embedding: Option<String>,
}

pub async fn get_for_facial_recognition(
    pool: &Pool<Postgres>,
    face_id: &Uuid,
) -> Result<Option<FacialRecognitionFaceRow>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let face_col = schema.face_person_col_quoted();
    sqlx::query_as::<_, FacialRecognitionFaceRow>(&format!(
        r#"
            SELECT
                asset_face.id,
                asset_face.{face_col} AS person_id,
                asset_face."sourceType"::text AS source_type,
                asset."ownerId" AS owner_id,
                asset.visibility,
                asset."fileCreatedAt" AS file_created_at,
                face_search.embedding::text AS embedding
            FROM asset_face
            INNER JOIN asset ON asset.id = asset_face."assetId"
            LEFT JOIN face_search ON face_search."faceId" = asset_face.id
            WHERE asset_face.id = $1
              AND asset_face."deletedAt" IS NULL
        "#
    ))
    .bind(face_id)
    .fetch_optional(pool)
    .await
}

pub async fn stream_unassigned_ml_faces(
    pool: &Pool<Postgres>,
    force: bool,
    cluster_group_id: Option<&Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let face_col = schema.face_person_col_quoted();

    if force {
        if let Some(cluster_group_id) = cluster_group_id {
            return sqlx::query_scalar(
                r#"
                SELECT asset_face.id
                FROM asset_face
                INNER JOIN asset ON asset.id = asset_face."assetId"
                INNER JOIN "user" ON "user".id = asset."ownerId"
                WHERE asset_face."sourceType" = 'machine-learning'
                  AND asset_face."deletedAt" IS NULL
                  AND "user"."clusterGroupId" = $1
                "#,
            )
            .bind(cluster_group_id)
            .fetch_all(pool)
            .await;
        }

        return sqlx::query_scalar(
            r#"
                SELECT asset_face.id
                FROM asset_face
                WHERE asset_face."sourceType" = 'machine-learning'
                  AND asset_face."deletedAt" IS NULL
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    if let Some(cluster_group_id) = cluster_group_id {
        return sqlx::query_scalar(&format!(
            r#"
            SELECT asset_face.id
            FROM asset_face
            INNER JOIN asset ON asset.id = asset_face."assetId"
            INNER JOIN "user" ON "user".id = asset."ownerId"
            WHERE asset_face."sourceType" = 'machine-learning'
              AND asset_face.{face_col} IS NULL
              AND asset_face."deletedAt" IS NULL
              AND "user"."clusterGroupId" = $1
            "#
        ))
        .bind(cluster_group_id)
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(&format!(
        r#"
            SELECT asset_face.id
            FROM asset_face
            WHERE asset_face."sourceType" = 'machine-learning'
              AND asset_face.{face_col} IS NULL
              AND asset_face."deletedAt" IS NULL
        "#
    ))
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, FromRow)]
pub struct FaceSearchMatchRow {
    pub id: Uuid,
    pub person_id: Option<Uuid>,
    pub distance: f64,
}

pub async fn search_faces(
    pool: &Pool<Postgres>,
    embedding: &str,
    owner_ids: &[Uuid],
    max_distance: f64,
    num_results: i64,
    has_person: bool,
    min_birth_date: Option<DateTime<Utc>>,
) -> Result<Vec<FaceSearchMatchRow>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let face_col = schema.face_person_col_quoted();
    let person_join = schema.join_person_to_face_with_owner("person", "asset_face", "asset");

    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL vchordrq.probes = 1")
        .execute(&mut *tx)
        .await
        .ok();

    let rows = if has_person {
        sqlx::query_as::<_, FaceSearchMatchRow>(&format!(
            r#"
                WITH cte AS (
                    SELECT
                        asset_face.id,
                        asset_face.{face_col} AS person_id,
                        face_search.embedding <=> $1::vector AS distance
                    FROM asset_face
                    INNER JOIN asset ON asset.id = asset_face."assetId"
                    INNER JOIN face_search ON face_search."faceId" = asset_face.id
                    LEFT JOIN person ON {person_join}
                    WHERE asset."ownerId" = ANY($2::uuid[])
                      AND asset."deletedAt" IS NULL
                      AND asset_face.{face_col} IS NOT NULL
                      AND ($5::timestamptz IS NULL OR person."birthDate" IS NULL OR person."birthDate" <= $5::date)
                    ORDER BY distance
                    LIMIT $3
                )
                SELECT id, person_id, distance
                FROM cte
                WHERE distance <= $4
            "#
        ))
        .bind(embedding)
        .bind(owner_ids)
        .bind(num_results)
        .bind(max_distance)
        .bind(min_birth_date)
        .fetch_all(&mut *tx)
        .await?
    } else {
        sqlx::query_as::<_, FaceSearchMatchRow>(&format!(
            r#"
                WITH cte AS (
                    SELECT
                        asset_face.id,
                        asset_face.{face_col} AS person_id,
                        face_search.embedding <=> $1::vector AS distance
                    FROM asset_face
                    INNER JOIN asset ON asset.id = asset_face."assetId"
                    INNER JOIN face_search ON face_search."faceId" = asset_face.id
                    LEFT JOIN person ON {person_join}
                    WHERE asset."ownerId" = ANY($2::uuid[])
                      AND asset."deletedAt" IS NULL
                      AND ($5::timestamptz IS NULL OR person."birthDate" IS NULL OR person."birthDate" <= $5::date)
                    ORDER BY distance
                    LIMIT $3
                )
                SELECT id, person_id, distance
                FROM cte
                WHERE distance <= $4
            "#
        ))
        .bind(embedding)
        .bind(owner_ids)
        .bind(num_results)
        .bind(max_distance)
        .bind(min_birth_date)
        .fetch_all(&mut *tx)
        .await?
    };

    tx.commit().await?;
    Ok(rows)
}

pub async fn get_latest_face_date(pool: &Pool<Postgres>) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT max("facesRecognizedAt")::text
        FROM asset_job_status
        "#,
    )
    .fetch_one(pool)
    .await
}

pub async fn prewarm_face_vectors(pool: &Pool<Postgres>) {
    if !crate::utils::vector::face_search_available(pool).await {
        return;
    }
    let _ = sqlx::query("SELECT vchordrq_prewarm($1)")
        .bind("face_search")
        .execute(pool)
        .await;
}
