use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::notification::{search_notifications, NotificationSearchFilter};
use crate::models::dto::auth::AuthDto;
use crate::models::response::notification::{
    format_datetime, format_optional_datetime, NotificationResponse,
};
use crate::models::response::response::ErrorResp;
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct NotificationService {
    pool: PgPool,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSearchQuery {
    pub id: Option<Uuid>,
    pub level: Option<String>,
    pub r#type: Option<String>,
    pub unread: Option<String>,
}

impl NotificationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &NotificationSearchQuery,
    ) -> Result<Vec<NotificationResponse>, ErrorResp> {
        require_permission(auth, Permission::NotificationRead)?;

        let filter = NotificationSearchFilter {
            id: query.id,
            level: query.level.clone(),
            notification_type: query.r#type.clone(),
            unread: parse_bool(&query.unread),
        };

        let rows = search_notifications(&self.pool, &auth.user.id, &filter).await?;
        Ok(rows
            .into_iter()
            .map(|row| NotificationResponse {
                id: row.id,
                created_at: format_datetime(&row.created_at),
                level: row.level,
                notification_type: row.notification_type,
                title: row.title,
                description: row.description,
                data: row.data,
                read_at: format_optional_datetime(&row.read_at),
            })
            .collect())
    }
}

fn parse_bool(value: &Option<String>) -> Option<bool> {
    value
        .as_deref()
        .and_then(crate::utils::query::parse_query_bool)
}
