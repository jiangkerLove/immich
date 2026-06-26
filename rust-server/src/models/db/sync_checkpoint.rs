use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SyncCheckpointRow {
    pub r#type: String,
    pub ack: String,
}

pub async fn get_all(
    pool: &PgPool,
    session_id: &Uuid,
) -> Result<Vec<SyncCheckpointRow>, sqlx::Error> {
    sqlx::query_as::<_, SyncCheckpointRow>(
        r#"
        SELECT type, ack
        FROM session_sync_checkpoint
        WHERE "sessionId" = $1
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
}

pub async fn upsert_all(
    pool: &PgPool,
    session_id: &Uuid,
    items: &[(String, String)],
) -> Result<(), sqlx::Error> {
    if items.is_empty() {
        return Ok(());
    }

    let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
        r#"INSERT INTO session_sync_checkpoint ("sessionId", type, ack) "#,
    );
    builder.push_values(items, |mut b, (sync_type, ack)| {
        b.push_bind(session_id)
            .push_bind(sync_type)
            .push_bind(ack);
    });
    builder.push(
        r#" ON CONFLICT ("sessionId", type) DO UPDATE SET ack = EXCLUDED.ack"#,
    );
    builder.build().execute(pool).await?;
    Ok(())
}

pub async fn delete_all(
    pool: &PgPool,
    session_id: &Uuid,
    types: Option<&[String]>,
) -> Result<(), sqlx::Error> {
    match types {
        Some(types) if !types.is_empty() => {
            sqlx::query(
                r#"
                DELETE FROM session_sync_checkpoint
                WHERE "sessionId" = $1 AND type = ANY($2)
                "#,
            )
            .bind(session_id)
            .bind(types)
            .execute(pool)
            .await?;
        }
        _ => {
            sqlx::query(
                r#"DELETE FROM session_sync_checkpoint WHERE "sessionId" = $1"#,
            )
            .bind(session_id)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

pub async fn get_now(pool: &PgPool) -> Result<String, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT immich_uuid_v7(now() - interval '1 millisecond')::text"#,
    )
    .fetch_one(pool)
    .await
}
