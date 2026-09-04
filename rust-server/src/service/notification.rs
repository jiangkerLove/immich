use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::album;
use crate::models::db::assets;
use crate::models::db::auth_permission::Permission;
use crate::models::db::notification::{
    NotificationSearchFilter, create_notification, delete_notification, delete_notifications,
    filter_owned_ids, get_notification, search_notifications, update_notification,
    update_notifications,
};
use crate::models::db::system_metadata::get_json;
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::db::users::UserDb;
use crate::models::dto::auth::AuthDto;
use crate::models::response::notification::{
    NotificationResponse, format_datetime, format_optional_datetime,
};
use crate::models::response::response::ErrorResp;
use crate::service::email::{
    AlbumInviteEmailData, AlbumUpdateEmailData, EmailService, EmailTemplate, SmtpConfig,
    TestEmailData, WelcomeEmailData,
};
use crate::service::job::{JobService, SendMailAttachmentJob, SendMailJob};
use crate::service::websocket::WebSocketHub;
use crate::utils::file_response::file_extension;
use crate::utils::permission::{require_admin, require_permission};
use crate::utils::preferences::resolve_preferences;

#[derive(Clone)]
pub struct NotificationService {
    pool: PgPool,
    websocket: WebSocketHub,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSearchQuery {
    pub id: Option<Uuid>,
    pub level: Option<String>,
    pub r#type: Option<String>,
    pub unread: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationUpdateReq {
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationUpdateAllReq {
    pub ids: Vec<Uuid>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDeleteAllReq {
    pub ids: Vec<Uuid>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationCreateReq {
    pub user_id: Uuid,
    pub level: Option<String>,
    pub r#type: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub data: Option<serde_json::Value>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestEmailResponse {
    pub message_id: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreviewReq {
    pub template: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreviewResponse {
    pub name: String,
    pub html: String,
}

impl NotificationService {
    pub fn new(pool: PgPool, websocket: WebSocketHub) -> Self {
        Self { pool, websocket }
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &NotificationSearchQuery,
    ) -> Result<Vec<NotificationResponse>, ErrorResp> {
        require_permission(auth, Permission::NotificationRead)?;
        self.search_rows(auth, query).await
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<NotificationResponse, ErrorResp> {
        require_permission(auth, Permission::NotificationRead)?;
        let row = get_notification(&self.pool, &auth.user.id, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Notification not found".to_string()))?;
        Ok(map_row(row))
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &NotificationUpdateReq,
    ) -> Result<NotificationResponse, ErrorResp> {
        require_permission(auth, Permission::NotificationUpdate)?;
        self.ensure_owned(auth, &[*id], Permission::NotificationUpdate)
            .await?;
        let row = update_notification(&self.pool, &auth.user.id, id, dto.read_at)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Notification not found".to_string()))?;
        Ok(map_row(row))
    }

    pub async fn update_all(
        &self,
        auth: &AuthDto,
        dto: &NotificationUpdateAllReq,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::NotificationUpdate)?;
        self.ensure_owned(auth, &dto.ids, Permission::NotificationUpdate)
            .await?;
        update_notifications(&self.pool, &auth.user.id, &dto.ids, dto.read_at).await?;
        Ok(())
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::NotificationDelete)?;
        self.ensure_owned(auth, &[*id], Permission::NotificationDelete)
            .await?;
        if !delete_notification(&self.pool, &auth.user.id, id).await? {
            return Err(ErrorResp::BadRequest("Notification not found".to_string()));
        }
        Ok(())
    }

    pub async fn delete_all(
        &self,
        auth: &AuthDto,
        dto: &NotificationDeleteAllReq,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::NotificationDelete)?;
        self.ensure_owned(auth, &dto.ids, Permission::NotificationDelete)
            .await?;
        delete_notifications(&self.pool, &auth.user.id, &dto.ids).await?;
        Ok(())
    }

    pub async fn admin_create(
        &self,
        auth: &AuthDto,
        dto: &NotificationCreateReq,
    ) -> Result<NotificationResponse, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::NotificationCreate)?;

        let row = create_notification(
            &self.pool,
            &dto.user_id,
            dto.level.as_deref().unwrap_or("info"),
            dto.r#type.as_deref().unwrap_or("Custom"),
            &dto.title,
            dto.description.as_deref(),
            dto.data.clone(),
            dto.read_at,
        )
        .await
        .map_err(|_| ErrorResp::BadRequest("Failed to create notification".to_string()))?;

        let response = map_row(row);
        self.websocket
            .emit_notification(dto.user_id, response.clone());
        Ok(response)
    }

    pub async fn admin_send_test_email(
        &self,
        auth: &AuthDto,
        dto: &SmtpConfig,
    ) -> Result<TestEmailResponse, ErrorResp> {
        require_admin(auth)?;

        let user = UserDb::select_full_by_id(&self.pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()))?;

        EmailService::verify_smtp(&dto.transport).await?;

        let base_url = get_external_domain(&self.pool).await?;
        let rendered = EmailService::render_test(
            &TestEmailData {
                base_url,
                display_name: user.name,
            },
            "",
        );

        let reply_to = if dto.reply_to.is_empty() {
            dto.from.as_str()
        } else {
            dto.reply_to.as_str()
        };

        let message_id = EmailService::send(
            &user.email,
            &dto.from,
            reply_to,
            "Test email from Immich",
            &rendered.html,
            &rendered.text,
            &dto.transport,
            &[],
        )
        .await?;

        Ok(TestEmailResponse { message_id })
    }

    pub async fn admin_render_template(
        &self,
        auth: &AuthDto,
        name: &str,
        dto: &TemplatePreviewReq,
    ) -> Result<TemplatePreviewResponse, ErrorResp> {
        require_admin(auth)?;

        let template = EmailTemplate::parse(name)
            .ok_or_else(|| ErrorResp::BadRequest(format!("Unknown template: {name}")))?;

        let base_url = get_external_domain(&self.pool).await?;
        let defaults = get_email_templates(&self.pool).await?;

        let html = match template {
            EmailTemplate::Test => {
                EmailService::render_test(
                    &TestEmailData {
                        base_url: base_url.clone(),
                        display_name: "John Doe".to_string(),
                    },
                    "",
                )
                .html
            }
            EmailTemplate::Welcome => {
                let custom = if dto.template.is_empty() {
                    defaults.welcome_template
                } else {
                    dto.template.clone()
                };
                EmailService::render_welcome(
                    &WelcomeEmailData {
                        base_url: base_url.clone(),
                        display_name: "John Doe".to_string(),
                        username: "john@doe.com".to_string(),
                        password: Some("thisIsAPassword123".to_string()),
                    },
                    &custom,
                )
                .html
            }
            EmailTemplate::AlbumInvite => {
                let custom = if dto.template.is_empty() {
                    defaults.album_invite_template
                } else {
                    dto.template.clone()
                };
                EmailService::render_album_invite(
                    &AlbumInviteEmailData {
                        base_url: base_url.clone(),
                        album_id: "1".to_string(),
                        album_name: "John Doe's Favorites".to_string(),
                        sender_name: "John Doe".to_string(),
                        recipient_name: "Jane Doe".to_string(),
                        cid: None,
                    },
                    &custom,
                )
                .html
            }
            EmailTemplate::AlbumUpdate => {
                let custom = if dto.template.is_empty() {
                    defaults.album_update_template
                } else {
                    dto.template.clone()
                };
                EmailService::render_album_update(
                    &AlbumUpdateEmailData {
                        base_url,
                        album_id: "1".to_string(),
                        album_name: "Favorite Photos".to_string(),
                        recipient_name: "Jane Doe".to_string(),
                        cid: None,
                    },
                    &custom,
                )
                .html
            }
        };

        Ok(TemplatePreviewResponse {
            name: name.to_string(),
            html,
        })
    }

    async fn search_rows(
        &self,
        auth: &AuthDto,
        query: &NotificationSearchQuery,
    ) -> Result<Vec<NotificationResponse>, ErrorResp> {
        let filter = NotificationSearchFilter {
            id: query.id,
            level: query.level.clone(),
            notification_type: query.r#type.clone(),
            unread: parse_bool(&query.unread),
        };

        let rows = search_notifications(&self.pool, &auth.user.id, &filter).await?;
        Ok(rows.into_iter().map(map_row).collect())
    }

    async fn ensure_owned(
        &self,
        auth: &AuthDto,
        ids: &[Uuid],
        permission: Permission,
    ) -> Result<(), ErrorResp> {
        if ids.is_empty() {
            return Ok(());
        }
        let owned = filter_owned_ids(&self.pool, &auth.user.id, ids).await?;
        if owned.len() != ids.len() {
            return Err(ErrorResp::BadRequest(format!(
                "Not found or no {} access",
                permission.as_str()
            )));
        }
        Ok(())
    }
}

fn map_row(row: crate::models::db::notification::NotificationRow) -> NotificationResponse {
    NotificationResponse {
        id: row.id,
        created_at: format_datetime(&row.created_at),
        level: row.level,
        notification_type: row.notification_type,
        title: row.title,
        description: row.description,
        data: row.data,
        read_at: format_optional_datetime(&row.read_at),
    }
}

fn parse_bool(value: &Option<String>) -> Option<bool> {
    value
        .as_deref()
        .and_then(crate::utils::query::parse_query_bool)
}

struct EmailTemplateDefaults {
    welcome_template: String,
    album_invite_template: String,
    album_update_template: String,
}

async fn get_external_domain(pool: &PgPool) -> Result<String, ErrorResp> {
    let config = get_json(pool, "system-config").await?;
    Ok(config
        .and_then(|value| {
            value
                .get("server")
                .and_then(|server| server.get("externalDomain"))
                .and_then(|domain| domain.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default())
}

async fn get_email_templates(pool: &PgPool) -> Result<EmailTemplateDefaults, ErrorResp> {
    let defaults_json = include_str!("../../config/system_config_defaults.json");
    let defaults: serde_json::Value = serde_json::from_str(defaults_json).unwrap_or_default();
    let stored = get_json(pool, "system-config").await?;

    let templates = stored
        .as_ref()
        .and_then(|value| value.get("templates"))
        .or_else(|| defaults.get("templates"))
        .cloned()
        .unwrap_or_default();

    let email = templates.get("email").cloned().unwrap_or_default();

    Ok(EmailTemplateDefaults {
        welcome_template: email
            .get("welcomeTemplate")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        album_invite_template: email
            .get("albumInviteTemplate")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        album_update_template: email
            .get("albumUpdateTemplate")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationJobResult {
    Success,
    Skipped,
}

#[derive(Clone)]
pub struct NotificationJobProcessor {
    pool: PgPool,
    websocket: WebSocketHub,
    jobs: JobService,
}

impl NotificationJobProcessor {
    pub fn with_jobs(pool: PgPool, websocket: WebSocketHub, jobs: JobService) -> Self {
        Self {
            pool,
            websocket,
            jobs,
        }
    }

    pub async fn process(
        &self,
        job_name: &str,
        data: &serde_json::Value,
    ) -> Result<NotificationJobResult, ErrorResp> {
        match job_name {
            "NotifyUserSignup" => {
                let job: crate::service::job::NotifyUserSignupJob =
                    serde_json::from_value(data.clone())
                        .map_err(|e| ErrorResp::ServerError(e.to_string()))?;
                self.handle_user_signup(&job).await
            }
            "NotifyAlbumInvite" => {
                let job: crate::service::job::NotifyAlbumInviteJob =
                    serde_json::from_value(data.clone())
                        .map_err(|e| ErrorResp::ServerError(e.to_string()))?;
                self.handle_album_invite(&job).await
            }
            "NotifyAlbumUpdate" => {
                let job: crate::service::job::NotifyAlbumUpdateJob =
                    serde_json::from_value(data.clone())
                        .map_err(|e| ErrorResp::ServerError(e.to_string()))?;
                self.handle_album_update(&job).await
            }
            "SendMail" => {
                let job: SendMailJob = serde_json::from_value(data.clone())
                    .map_err(|e| ErrorResp::ServerError(e.to_string()))?;
                self.handle_send_mail(&job).await
            }
            _ => Ok(NotificationJobResult::Skipped),
        }
    }

    async fn handle_user_signup(
        &self,
        job: &crate::service::job::NotifyUserSignupJob,
    ) -> Result<NotificationJobResult, ErrorResp> {
        let user = match UserDb::select_full_by_id(&self.pool, &job.id).await? {
            Some(user) => user,
            None => return Ok(NotificationJobResult::Skipped),
        };

        let base_url = get_external_domain(&self.pool).await?;
        let templates = get_email_templates(&self.pool).await?;
        let rendered = EmailService::render_welcome(
            &WelcomeEmailData {
                base_url,
                display_name: user.name,
                username: user.email.clone(),
                password: job.password.clone(),
            },
            &templates.welcome_template,
        );

        self.jobs
            .queue_send_mail(SendMailJob {
                to: user.email,
                subject: "Welcome to Immich".to_string(),
                html: rendered.html,
                text: rendered.text,
                image_attachments: None,
            })
            .await?;

        Ok(NotificationJobResult::Success)
    }

    async fn handle_album_invite(
        &self,
        job: &crate::service::job::NotifyAlbumInviteJob,
    ) -> Result<NotificationJobResult, ErrorResp> {
        let album = match album::get_album_row(&self.pool, &job.id).await? {
            Some(album) => album,
            None => return Ok(NotificationJobResult::Skipped),
        };

        let recipient = match UserDb::select_full_by_id(&self.pool, &job.recipient_id).await? {
            Some(user) => user,
            None => return Ok(NotificationJobResult::Skipped),
        };

        self.send_album_local_notification(
            &album,
            &job.recipient_id,
            "AlbumInvite",
            Some(&job.sender_name),
        )
        .await?;

        if !email_notifications_enabled(&self.pool, &job.recipient_id, true).await? {
            return Ok(NotificationJobResult::Skipped);
        }

        let attachment = self.get_album_thumbnail_attachment(&album).await?;
        let base_url = get_external_domain(&self.pool).await?;
        let templates = get_email_templates(&self.pool).await?;
        let rendered = EmailService::render_album_invite(
            &AlbumInviteEmailData {
                base_url,
                album_id: album.id.to_string(),
                album_name: album.album_name.clone(),
                sender_name: job.sender_name.clone(),
                recipient_name: recipient.name.clone(),
                cid: attachment.as_ref().map(|item| item.cid.clone()),
            },
            &templates.album_invite_template,
        );

        self.jobs
            .queue_send_mail(SendMailJob {
                to: recipient.email,
                subject: format!(
                    "You have been added to a shared album - {}",
                    album.album_name
                ),
                html: rendered.html,
                text: rendered.text,
                image_attachments: attachment.map(|item| vec![item]),
            })
            .await?;

        Ok(NotificationJobResult::Success)
    }

    async fn handle_album_update(
        &self,
        job: &crate::service::job::NotifyAlbumUpdateJob,
    ) -> Result<NotificationJobResult, ErrorResp> {
        let album = match album::get_album_row(&self.pool, &job.id).await? {
            Some(album) => album,
            None => return Ok(NotificationJobResult::Skipped),
        };

        let recipient = match UserDb::select_full_by_id(&self.pool, &job.recipient_id).await? {
            Some(user) => user,
            None => return Ok(NotificationJobResult::Skipped),
        };

        self.send_album_local_notification(&album, &job.recipient_id, "AlbumUpdate", None)
            .await?;

        if !email_notifications_enabled(&self.pool, &job.recipient_id, false).await? {
            return Ok(NotificationJobResult::Skipped);
        }

        let attachment = self.get_album_thumbnail_attachment(&album).await?;
        let base_url = get_external_domain(&self.pool).await?;
        let templates = get_email_templates(&self.pool).await?;
        let rendered = EmailService::render_album_update(
            &AlbumUpdateEmailData {
                base_url,
                album_id: album.id.to_string(),
                album_name: album.album_name.clone(),
                recipient_name: recipient.name.clone(),
                cid: attachment.as_ref().map(|item| item.cid.clone()),
            },
            &templates.album_update_template,
        );

        self.jobs
            .queue_send_mail(SendMailJob {
                to: recipient.email,
                subject: format!(
                    "New media has been added to an album - {}",
                    album.album_name
                ),
                html: rendered.html,
                text: rendered.text,
                image_attachments: attachment.map(|item| vec![item]),
            })
            .await?;

        Ok(NotificationJobResult::Success)
    }

    async fn handle_send_mail(
        &self,
        job: &SendMailJob,
    ) -> Result<NotificationJobResult, ErrorResp> {
        let smtp = get_smtp_config(&self.pool).await?;
        if !smtp.enabled {
            return Ok(NotificationJobResult::Skipped);
        }

        let reply_to = if smtp.reply_to.is_empty() {
            smtp.from.as_str()
        } else {
            smtp.reply_to.as_str()
        };

        let attachments: Vec<crate::service::email::EmailImageAttachment> = job
            .image_attachments
            .as_ref()
            .map(|items| {
                items
                    .iter()
                    .map(|item| crate::service::email::EmailImageAttachment {
                        filename: item.filename.clone(),
                        path: item.path.clone(),
                        cid: item.cid.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        EmailService::send(
            &job.to,
            &smtp.from,
            reply_to,
            &job.subject,
            &job.html,
            &job.text,
            &smtp.transport,
            &attachments,
        )
        .await?;

        Ok(NotificationJobResult::Success)
    }

    async fn send_album_local_notification(
        &self,
        album: &album::AlbumRow,
        user_id: &Uuid,
        notification_type: &str,
        sender_name: Option<&str>,
    ) -> Result<(), ErrorResp> {
        let (level, title, description) = if notification_type == "AlbumInvite" {
            (
                "success",
                "Shared Album Invitation",
                format!(
                    "{} shared an album ({}) with you",
                    sender_name.unwrap_or("Someone"),
                    album.album_name
                ),
            )
        } else {
            (
                "info",
                "Shared Album Update",
                format!(
                    "New media has been added to the album ({})",
                    album.album_name
                ),
            )
        };

        let data = serde_json::json!({ "albumId": album.id });
        let row = create_notification(
            &self.pool,
            user_id,
            level,
            notification_type,
            title,
            Some(&description),
            Some(data),
            None,
        )
        .await
        .map_err(|_| ErrorResp::ServerError("Failed to create notification".to_string()))?;

        let response = map_row(row);
        self.websocket.emit_notification(*user_id, response);
        Ok(())
    }

    async fn get_album_thumbnail_attachment(
        &self,
        album: &album::AlbumRow,
    ) -> Result<Option<SendMailAttachmentJob>, ErrorResp> {
        let Some(thumbnail_id) = album.album_thumbnail_asset_id else {
            return Ok(None);
        };

        let thumb = assets::get_for_thumbnail(&self.pool, &thumbnail_id, "thumbnail")
            .await?
            .and_then(|row| row.path);

        let Some(path) = thumb else {
            return Ok(None);
        };

        Ok(Some(SendMailAttachmentJob {
            filename: format!("album-thumbnail{}", file_extension(&path)),
            path,
            cid: "album-thumbnail".to_string(),
        }))
    }
}

async fn get_smtp_config(pool: &PgPool) -> Result<SmtpConfig, ErrorResp> {
    let defaults_json = include_str!("../../config/system_config_defaults.json");
    let defaults: serde_json::Value = serde_json::from_str(defaults_json).unwrap_or_default();
    let stored = get_json(pool, "system-config").await?;

    let notifications = stored
        .as_ref()
        .and_then(|value| value.get("notifications"))
        .or_else(|| defaults.get("notifications"))
        .cloned()
        .unwrap_or_default();

    serde_json::from_value(notifications.get("smtp").cloned().unwrap_or_default())
        .map_err(|err| ErrorResp::ServerError(err.to_string()))
}

async fn email_notifications_enabled(
    pool: &PgPool,
    user_id: &Uuid,
    invite: bool,
) -> Result<bool, ErrorResp> {
    let stored = UserMetadataPO::get_preferences_json(pool, user_id).await?;
    let preferences = resolve_preferences(stored);
    let email = preferences
        .get("emailNotifications")
        .cloned()
        .unwrap_or_default();

    let enabled = email
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if !enabled {
        return Ok(false);
    }

    if invite {
        Ok(email
            .get("albumInvite")
            .and_then(|value| value.as_bool())
            .unwrap_or(true))
    } else {
        Ok(email
            .get("albumUpdate")
            .and_then(|value| value.as_bool())
            .unwrap_or(true))
    }
}
