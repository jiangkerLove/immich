use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::db::DbService;
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct TagService {
    db: DbService,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TagResponse {
    pub id: Uuid,
    pub value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub color: Option<String>,
    pub parent_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCreateReq {
    pub name: String,
    pub color: Option<String>,
    pub parent_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagUpdateReq {
    pub color: Option<String>,
}

impl TagService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            db: DbService::new(pool),
        }
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<TagResponse>, ErrorResp> {
        require_permission(auth, Permission::TagRead)?;
        sqlx::query_as::<_, TagResponse>(
            r#"
                SELECT id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                       color, "parentId" as parent_id
                FROM tag
                WHERE "userId" = $1
                ORDER BY value
            "#,
        )
        .bind(auth.user.id)
        .fetch_all(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<TagResponse, ErrorResp> {
        require_permission(auth, Permission::TagRead)?;
        self.get_owned(auth, id).await
    }

    pub async fn create(&self, auth: &AuthDto, dto: &TagCreateReq) -> Result<TagResponse, ErrorResp> {
        require_permission(auth, Permission::TagCreate)?;

        let value = if let Some(parent_id) = dto.parent_id {
            let parent = self.get_owned(auth, &parent_id).await?;
            format!("{}/{}", parent.value, dto.name)
        } else {
            dto.name.clone()
        };

        let duplicate: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT id FROM tag WHERE "userId" = $1 AND value = $2"#,
        )
        .bind(auth.user.id)
        .bind(&value)
        .fetch_optional(&self.db.pool)
        .await?;

        if duplicate.is_some() {
            return Err(ErrorResp::BadRequest(
                "A tag with that name already exists".to_string(),
            ));
        }

        sqlx::query_as::<_, TagResponse>(
            r#"
                INSERT INTO tag ("userId", color, value, "parentId")
                VALUES ($1, $2, $3, $4)
                RETURNING id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                          color, "parentId" as parent_id
            "#,
        )
        .bind(auth.user.id)
        .bind(&dto.color)
        .bind(&value)
        .bind(dto.parent_id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &TagUpdateReq,
    ) -> Result<TagResponse, ErrorResp> {
        require_permission(auth, Permission::TagUpdate)?;
        self.get_owned(auth, id).await?;

        sqlx::query_as::<_, TagResponse>(
            r#"
                UPDATE tag SET color = $1 WHERE id = $2
                RETURNING id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                          color, "parentId" as parent_id
            "#,
        )
        .bind(&dto.color)
        .bind(id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::TagDelete)?;
        self.get_owned(auth, id).await?;
        sqlx::query(r#"DELETE FROM tag WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    async fn get_owned(&self, auth: &AuthDto, id: &Uuid) -> Result<TagResponse, ErrorResp> {
        sqlx::query_as::<_, TagResponse>(
            r#"
                SELECT id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                       color, "parentId" as parent_id
                FROM tag
                WHERE id = $1 AND "userId" = $2
            "#,
        )
        .bind(id)
        .bind(auth.user.id)
        .fetch_optional(&self.db.pool)
        .await?
        .ok_or_else(|| ErrorResp::BadRequest("Tag not found".to_string()))
    }
}
