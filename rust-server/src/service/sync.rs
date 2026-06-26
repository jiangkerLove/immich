use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::sessions::{is_pending_sync_reset, reset_sync_progress};
use crate::models::db::sync_checkpoint::{self, SyncCheckpointRow};
use crate::models::db::sync_repository::{
    album_asset_exif_get_backfill, album_asset_exif_get_creates, album_asset_exif_get_updates,
    album_asset_get_backfill, album_asset_get_creates, album_asset_get_updates, album_get_album_users,
    album_get_created_after, album_get_deletes, album_get_upserts, album_to_asset_get_backfill,
    album_to_asset_get_deletes, album_to_asset_get_upserts, album_user_get_backfill,
    album_user_get_deletes, album_user_get_upserts, asset_edit_get_deletes, asset_edit_get_upserts,
    asset_exif_get_upserts, asset_face_get_deletes, asset_face_get_upserts, asset_get_deletes,
    asset_get_upserts, asset_metadata_get_deletes, asset_metadata_get_upserts, asset_ocr_get_deletes,
    asset_ocr_get_upserts, auth_user_get_upserts, memory_get_deletes, memory_get_upserts,
    memory_to_asset_get_deletes, memory_to_asset_get_upserts, partner_asset_exif_get_backfill,
    partner_asset_exif_get_upserts, partner_asset_get_backfill, partner_asset_get_deletes,
    partner_asset_get_upserts, partner_get_created_after, partner_get_deletes, partner_get_upserts,
    partner_stack_get_backfill, partner_stack_get_deletes, partner_stack_get_upserts,
    person_get_deletes, person_get_upserts, stack_get_deletes, stack_get_upserts, user_get_deletes,
    user_get_upserts, user_metadata_get_deletes, user_metadata_get_upserts, AlbumUserRow,
    SyncBackfillOptions, SyncCreatedAfterOptions, SyncQueryOptions,
};
use crate::models::dto::auth::AuthDto;
use crate::models::request::sync::{SyncAckDeleteReq, SyncAckSetReq, SyncStreamReq};
use crate::models::response::response::ErrorResp;
use crate::utils::permission::require_permission;
use crate::utils::sync::{from_ack, serialize, to_ack, SyncAck};

pub const COMPLETE_ID: &str = "complete";
pub const MAX_DAYS: i64 = 30;

pub const SYNC_REQUEST_AUTH_USERS_V1: &str = "AuthUsersV1";
pub const SYNC_REQUEST_USERS_V1: &str = "UsersV1";
pub const SYNC_REQUEST_PARTNERS_V1: &str = "PartnersV1";
pub const SYNC_REQUEST_ASSETS_V1: &str = "AssetsV1";
pub const SYNC_REQUEST_ASSETS_V2: &str = "AssetsV2";
pub const SYNC_REQUEST_STACKS_V1: &str = "StacksV1";
pub const SYNC_REQUEST_PARTNER_ASSETS_V1: &str = "PartnerAssetsV1";
pub const SYNC_REQUEST_PARTNER_ASSETS_V2: &str = "PartnerAssetsV2";
pub const SYNC_REQUEST_PARTNER_STACKS_V1: &str = "PartnerStacksV1";
pub const SYNC_REQUEST_ALBUM_ASSETS_V1: &str = "AlbumAssetsV1";
pub const SYNC_REQUEST_ALBUM_ASSETS_V2: &str = "AlbumAssetsV2";
pub const SYNC_REQUEST_ALBUMS_V1: &str = "AlbumsV1";
pub const SYNC_REQUEST_ALBUMS_V2: &str = "AlbumsV2";
pub const SYNC_REQUEST_ALBUM_USERS_V1: &str = "AlbumUsersV1";
pub const SYNC_REQUEST_ALBUM_TO_ASSETS_V1: &str = "AlbumToAssetsV1";
pub const SYNC_REQUEST_ASSET_EXIFS_V1: &str = "AssetExifsV1";
pub const SYNC_REQUEST_ALBUM_ASSET_EXIFS_V1: &str = "AlbumAssetExifsV1";
pub const SYNC_REQUEST_ASSET_OCR_V1: &str = "AssetOcrV1";
pub const SYNC_REQUEST_PARTNER_ASSET_EXIFS_V1: &str = "PartnerAssetExifsV1";
pub const SYNC_REQUEST_MEMORIES_V1: &str = "MemoriesV1";
pub const SYNC_REQUEST_MEMORY_TO_ASSETS_V1: &str = "MemoryToAssetsV1";
pub const SYNC_REQUEST_PEOPLE_V1: &str = "PeopleV1";
pub const SYNC_REQUEST_ASSET_FACES_V1: &str = "AssetFacesV1";
pub const SYNC_REQUEST_ASSET_FACES_V2: &str = "AssetFacesV2";
pub const SYNC_REQUEST_USER_METADATA_V1: &str = "UserMetadataV1";
pub const SYNC_REQUEST_ASSET_METADATA_V1: &str = "AssetMetadataV1";
pub const SYNC_REQUEST_ASSET_EDITS_V1: &str = "AssetEditsV1";

pub const SYNC_TYPES_ORDER: &[&str] = &[
    SYNC_REQUEST_AUTH_USERS_V1,
    SYNC_REQUEST_USERS_V1,
    SYNC_REQUEST_PARTNERS_V1,
    SYNC_REQUEST_ASSETS_V1,
    SYNC_REQUEST_ASSETS_V2,
    SYNC_REQUEST_STACKS_V1,
    SYNC_REQUEST_PARTNER_ASSETS_V1,
    SYNC_REQUEST_PARTNER_ASSETS_V2,
    SYNC_REQUEST_PARTNER_STACKS_V1,
    SYNC_REQUEST_ALBUM_ASSETS_V1,
    SYNC_REQUEST_ALBUM_ASSETS_V2,
    SYNC_REQUEST_ALBUMS_V1,
    SYNC_REQUEST_ALBUMS_V2,
    SYNC_REQUEST_ALBUM_USERS_V1,
    SYNC_REQUEST_ALBUM_TO_ASSETS_V1,
    SYNC_REQUEST_ASSET_EXIFS_V1,
    SYNC_REQUEST_ALBUM_ASSET_EXIFS_V1,
    SYNC_REQUEST_ASSET_OCR_V1,
    SYNC_REQUEST_PARTNER_ASSET_EXIFS_V1,
    SYNC_REQUEST_MEMORIES_V1,
    SYNC_REQUEST_MEMORY_TO_ASSETS_V1,
    SYNC_REQUEST_PEOPLE_V1,
    SYNC_REQUEST_ASSET_FACES_V1,
    SYNC_REQUEST_ASSET_FACES_V2,
    SYNC_REQUEST_USER_METADATA_V1,
    SYNC_REQUEST_ASSET_METADATA_V1,
    SYNC_REQUEST_ASSET_EDITS_V1,
];

pub const SYNC_ENTITY_AUTH_USER_V1: &str = "AuthUserV1";
pub const SYNC_ENTITY_USER_V1: &str = "UserV1";
pub const SYNC_ENTITY_USER_DELETE_V1: &str = "UserDeleteV1";
pub const SYNC_ENTITY_ASSET_V2: &str = "AssetV2";
pub const SYNC_ENTITY_ASSET_DELETE_V1: &str = "AssetDeleteV1";
pub const SYNC_ENTITY_ASSET_EXIF_V1: &str = "AssetExifV1";
pub const SYNC_ENTITY_ASSET_EDIT_V1: &str = "AssetEditV1";
pub const SYNC_ENTITY_ASSET_EDIT_DELETE_V1: &str = "AssetEditDeleteV1";
pub const SYNC_ENTITY_ASSET_METADATA_V1: &str = "AssetMetadataV1";
pub const SYNC_ENTITY_ASSET_METADATA_DELETE_V1: &str = "AssetMetadataDeleteV1";
pub const SYNC_ENTITY_ASSET_OCR_V1: &str = "AssetOcrV1";
pub const SYNC_ENTITY_ASSET_OCR_DELETE_V1: &str = "AssetOcrDeleteV1";
pub const SYNC_ENTITY_PARTNER_V1: &str = "PartnerV1";
pub const SYNC_ENTITY_PARTNER_DELETE_V1: &str = "PartnerDeleteV1";
pub const SYNC_ENTITY_PARTNER_ASSET_V2: &str = "PartnerAssetV2";
pub const SYNC_ENTITY_PARTNER_ASSET_BACKFILL_V2: &str = "PartnerAssetBackfillV2";
pub const SYNC_ENTITY_PARTNER_ASSET_DELETE_V1: &str = "PartnerAssetDeleteV1";
pub const SYNC_ENTITY_PARTNER_ASSET_EXIF_V1: &str = "PartnerAssetExifV1";
pub const SYNC_ENTITY_PARTNER_ASSET_EXIF_BACKFILL_V1: &str = "PartnerAssetExifBackfillV1";
pub const SYNC_ENTITY_PARTNER_STACK_BACKFILL_V1: &str = "PartnerStackBackfillV1";
pub const SYNC_ENTITY_PARTNER_STACK_DELETE_V1: &str = "PartnerStackDeleteV1";
pub const SYNC_ENTITY_PARTNER_STACK_V1: &str = "PartnerStackV1";
pub const SYNC_ENTITY_ALBUM_V1: &str = "AlbumV1";
pub const SYNC_ENTITY_ALBUM_V2: &str = "AlbumV2";
pub const SYNC_ENTITY_ALBUM_DELETE_V1: &str = "AlbumDeleteV1";
pub const SYNC_ENTITY_ALBUM_USER_V1: &str = "AlbumUserV1";
pub const SYNC_ENTITY_ALBUM_USER_BACKFILL_V1: &str = "AlbumUserBackfillV1";
pub const SYNC_ENTITY_ALBUM_USER_DELETE_V1: &str = "AlbumUserDeleteV1";
pub const SYNC_ENTITY_ALBUM_ASSET_CREATE_V2: &str = "AlbumAssetCreateV2";
pub const SYNC_ENTITY_ALBUM_ASSET_UPDATE_V2: &str = "AlbumAssetUpdateV2";
pub const SYNC_ENTITY_ALBUM_ASSET_BACKFILL_V2: &str = "AlbumAssetBackfillV2";
pub const SYNC_ENTITY_ALBUM_ASSET_EXIF_CREATE_V1: &str = "AlbumAssetExifCreateV1";
pub const SYNC_ENTITY_ALBUM_ASSET_EXIF_UPDATE_V1: &str = "AlbumAssetExifUpdateV1";
pub const SYNC_ENTITY_ALBUM_ASSET_EXIF_BACKFILL_V1: &str = "AlbumAssetExifBackfillV1";
pub const SYNC_ENTITY_ALBUM_TO_ASSET_V1: &str = "AlbumToAssetV1";
pub const SYNC_ENTITY_ALBUM_TO_ASSET_DELETE_V1: &str = "AlbumToAssetDeleteV1";
pub const SYNC_ENTITY_ALBUM_TO_ASSET_BACKFILL_V1: &str = "AlbumToAssetBackfillV1";
pub const SYNC_ENTITY_MEMORY_V1: &str = "MemoryV1";
pub const SYNC_ENTITY_MEMORY_DELETE_V1: &str = "MemoryDeleteV1";
pub const SYNC_ENTITY_MEMORY_TO_ASSET_V1: &str = "MemoryToAssetV1";
pub const SYNC_ENTITY_MEMORY_TO_ASSET_DELETE_V1: &str = "MemoryToAssetDeleteV1";
pub const SYNC_ENTITY_STACK_V1: &str = "StackV1";
pub const SYNC_ENTITY_STACK_DELETE_V1: &str = "StackDeleteV1";
pub const SYNC_ENTITY_PERSON_V1: &str = "PersonV1";
pub const SYNC_ENTITY_PERSON_DELETE_V1: &str = "PersonDeleteV1";
pub const SYNC_ENTITY_ASSET_FACE_V2: &str = "AssetFaceV2";
pub const SYNC_ENTITY_ASSET_FACE_DELETE_V1: &str = "AssetFaceDeleteV1";
pub const SYNC_ENTITY_USER_METADATA_V1: &str = "UserMetadataV1";
pub const SYNC_ENTITY_USER_METADATA_DELETE_V1: &str = "UserMetadataDeleteV1";
pub const SYNC_ENTITY_SYNC_ACK_V1: &str = "SyncAckV1";
pub const SYNC_ENTITY_SYNC_RESET_V1: &str = "SyncResetV1";
pub const SYNC_ENTITY_SYNC_COMPLETE_V1: &str = "SyncCompleteV1";

type CheckpointMap = HashMap<String, SyncAck>;

#[derive(Clone)]
pub struct SyncService {
    pool: PgPool,
}

impl SyncService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_acks(&self, auth: &AuthDto) -> Result<Vec<SyncCheckpointRow>, ErrorResp> {
        let session_id = self.require_session(auth)?;
        require_permission(auth, Permission::SyncCheckpointRead)?;

        sync_checkpoint::get_all(&self.pool, &session_id)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn set_acks(&self, auth: &AuthDto, dto: &SyncAckSetReq) -> Result<(), ErrorResp> {
        let session_id = self.require_session(auth)?;
        require_permission(auth, Permission::SyncCheckpointUpdate)?;

        let mut checkpoints: HashMap<String, String> = HashMap::new();
        for ack in &dto.acks {
            let parsed = from_ack(ack);
            if parsed.ack_type == SYNC_ENTITY_SYNC_RESET_V1 {
                reset_sync_progress(&self.pool, &session_id)
                    .await
                    .map_err(ErrorResp::from)?;
                return Ok(());
            }
            if !is_valid_sync_entity_type(&parsed.ack_type) {
                return Err(ErrorResp::BadRequest(format!(
                    "Invalid ack type: {}",
                    parsed.ack_type
                )));
            }
            checkpoints.insert(parsed.ack_type.clone(), ack.clone());
        }

        let items: Vec<(String, String)> = checkpoints.into_iter().collect();
        sync_checkpoint::upsert_all(&self.pool, &session_id, &items)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn delete_acks(
        &self,
        auth: &AuthDto,
        dto: &SyncAckDeleteReq,
    ) -> Result<(), ErrorResp> {
        let session_id = self.require_session(auth)?;
        require_permission(auth, Permission::SyncCheckpointDelete)?;

        sync_checkpoint::delete_all(&self.pool, &session_id, dto.types.as_deref())
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn stream(
        &self,
        auth: &AuthDto,
        dto: &SyncStreamReq,
    ) -> Result<Vec<String>, ErrorResp> {
        let session = auth
            .session
            .as_ref()
            .ok_or_else(|| ErrorResp::Forbidden("Sync endpoints cannot be used with API keys".to_string()))?;
        require_permission(auth, Permission::SyncStream)?;

        let session_id = Uuid::parse_str(&session.id)
            .map_err(|_| ErrorResp::ServerError("Invalid session id".to_string()))?;

        if dto.reset.unwrap_or(false) {
            reset_sync_progress(&self.pool, &session_id)
                .await
                .map_err(ErrorResp::from)?;
        }

        let mut lines = Vec::new();

        if is_pending_sync_reset(&self.pool, &session_id)
            .await
            .map_err(ErrorResp::from)?
        {
            push_line(
                &mut lines,
                SYNC_ENTITY_SYNC_RESET_V1,
                &json!({}),
                &["reset"],
                None,
            );
            return Ok(lines);
        }

        let checkpoints = sync_checkpoint::get_all(&self.pool, &session_id)
            .await
            .map_err(ErrorResp::from)?;
        let checkpoint_map = build_checkpoint_map(&checkpoints);

        if needs_full_sync(&checkpoint_map) {
            push_line(
                &mut lines,
                SYNC_ENTITY_SYNC_RESET_V1,
                &json!({}),
                &["reset"],
                None,
            );
            return Ok(lines);
        }

        let now_id = sync_checkpoint::get_now(&self.pool)
            .await
            .map_err(ErrorResp::from)?;
        let options = SyncQueryOptions {
            now_id: now_id.clone(),
            user_id: auth.user.id,
            ack: None,
        };

        for sync_type in SYNC_TYPES_ORDER
            .iter()
            .copied()
            .filter(|t| dto.types.iter().any(|r| r == t))
        {
            match sync_type {
                SYNC_REQUEST_ASSETS_V1 => self.sync_assets_v1()?,
                SYNC_REQUEST_ASSET_FACES_V1 => self.sync_asset_faces_v1()?,
                SYNC_REQUEST_PARTNER_ASSETS_V1 => self.sync_partner_assets_v1()?,
                SYNC_REQUEST_ALBUM_ASSETS_V1 => self.sync_album_assets_v1()?,
                SYNC_REQUEST_AUTH_USERS_V1 => {
                    self.sync_auth_users_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_USERS_V1 => {
                    self.sync_users_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_PARTNERS_V1 => {
                    self.sync_partners_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_ASSETS_V2 => {
                    self.sync_assets_v2(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_ASSET_EXIFS_V1 => {
                    self.sync_asset_exifs_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_ASSET_EDITS_V1 => {
                    self.sync_asset_edits_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_PARTNER_ASSETS_V2 => {
                    self.sync_partner_assets_v2(
                        &options,
                        &mut lines,
                        &checkpoint_map,
                        &session_id,
                    )
                    .await?
                }
                SYNC_REQUEST_ASSET_METADATA_V1 => {
                    self.sync_asset_metadata_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_PARTNER_ASSET_EXIFS_V1 => {
                    self.sync_partner_asset_exifs_v1(
                        &options,
                        &mut lines,
                        &checkpoint_map,
                        &session_id,
                    )
                    .await?
                }
                SYNC_REQUEST_ALBUMS_V1 => {
                    self.sync_albums_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_ALBUMS_V2 => {
                    self.sync_albums_v2(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_ALBUM_USERS_V1 => {
                    self.sync_album_users_v1(
                        &options,
                        &mut lines,
                        &checkpoint_map,
                        &session_id,
                    )
                    .await?
                }
                SYNC_REQUEST_ALBUM_ASSETS_V2 => {
                    self.sync_album_assets_v2(
                        &options,
                        &mut lines,
                        &checkpoint_map,
                        &session_id,
                    )
                    .await?
                }
                SYNC_REQUEST_ALBUM_TO_ASSETS_V1 => {
                    self.sync_album_to_assets_v1(
                        &options,
                        &mut lines,
                        &checkpoint_map,
                        &session_id,
                    )
                    .await?
                }
                SYNC_REQUEST_ALBUM_ASSET_EXIFS_V1 => {
                    self.sync_album_asset_exifs_v1(
                        &options,
                        &mut lines,
                        &checkpoint_map,
                        &session_id,
                    )
                    .await?
                }
                SYNC_REQUEST_MEMORIES_V1 => {
                    self.sync_memories_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_MEMORY_TO_ASSETS_V1 => {
                    self.sync_memory_assets_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_STACKS_V1 => {
                    self.sync_stack_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_PARTNER_STACKS_V1 => {
                    self.sync_partner_stack_v1(
                        &options,
                        &mut lines,
                        &checkpoint_map,
                        &session_id,
                    )
                    .await?
                }
                SYNC_REQUEST_PEOPLE_V1 => {
                    self.sync_people_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_ASSET_FACES_V2 => {
                    self.sync_asset_faces_v2(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_USER_METADATA_V1 => {
                    self.sync_user_metadata_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                SYNC_REQUEST_ASSET_OCR_V1 => {
                    self.sync_asset_ocr_v1(&options, &mut lines, &checkpoint_map)
                        .await?
                }
                _ => {}
            }
        }

        push_line(
            &mut lines,
            SYNC_ENTITY_SYNC_COMPLETE_V1,
            &json!({}),
            &[&now_id],
            None,
        );

        Ok(lines)
    }

    fn require_session(&self, auth: &AuthDto) -> Result<Uuid, ErrorResp> {
        let session = auth.session.as_ref().ok_or_else(|| {
            ErrorResp::Forbidden("Sync endpoints cannot be used with API keys".to_string())
        })?;
        Uuid::parse_str(&session.id)
            .map_err(|_| ErrorResp::ServerError("Invalid session id".to_string()))
    }

    async fn upsert_backfill_checkpoint(
        &self,
        session_id: &Uuid,
        sync_type: &str,
        create_id: &str,
    ) -> Result<(), ErrorResp> {
        let ack = to_ack(&SyncAck {
            ack_type: sync_type.to_string(),
            update_id: create_id.to_string(),
            extra_id: Some(COMPLETE_ID.to_string()),
        });
        sync_checkpoint::upsert_all(&self.pool, session_id, &[(sync_type.to_string(), ack)])
            .await
            .map_err(ErrorResp::from)
    }

    fn sync_assets_v1(&self) -> Result<(), ErrorResp> {
        Err(ErrorResp::BadRequest(
            "SyncRequestType.AssetsV1 is deprecated, use SyncRequestType.AssetsV2 instead"
                .to_string(),
        ))
    }

    fn sync_partner_assets_v1(&self) -> Result<(), ErrorResp> {
        Err(ErrorResp::BadRequest(
            "SyncRequestType.PartnerAssetsV1 is deprecated, use SyncRequestType.PartnerAssetsV2 instead"
                .to_string(),
        ))
    }

    fn sync_album_assets_v1(&self) -> Result<(), ErrorResp> {
        Err(ErrorResp::BadRequest(
            "SyncRequestType.AlbumAssetsV1 is deprecated, use SyncRequestType.AlbumAssetsV2 instead"
                .to_string(),
        ))
    }

    fn sync_asset_faces_v1(&self) -> Result<(), ErrorResp> {
        Err(ErrorResp::BadRequest(
            "SyncRequestType.AssetFacesV1 is deprecated, use SyncRequestType.AssetFacesV2 instead"
                .to_string(),
        ))
    }

    async fn sync_auth_users_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let upsert_type = SYNC_ENTITY_AUTH_USER_V1;
        let upserts = auth_user_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &user_data_with_profile_flag(upsert.data),
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_users_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_USER_DELETE_V1;
        let deletes = user_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_USER_V1;
        let upserts = user_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &user_data_with_profile_flag(upsert.data),
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_partners_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_PARTNER_DELETE_V1;
        let deletes = partner_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_PARTNER_V1;
        let upserts = partner_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_assets_v2(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ASSET_DELETE_V1;
        let deletes = asset_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_ASSET_V2;
        let upserts = asset_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_partner_assets_v2(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
        session_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_PARTNER_ASSET_DELETE_V1;
        let deletes = partner_asset_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let backfill_type = SYNC_ENTITY_PARTNER_ASSET_BACKFILL_V2;
        let backfill_checkpoint = checkpoint_map.get(backfill_type);
        let created_after_options = SyncCreatedAfterOptions {
            now_id: options.now_id.clone(),
            user_id: options.user_id,
            after_create_id: backfill_checkpoint.map(|ack| ack.update_id.clone()),
        };
        let partners = partner_get_created_after(&self.pool, &created_after_options).await?;

        let upsert_type = SYNC_ENTITY_PARTNER_ASSET_V2;
        if let Some(upsert_checkpoint) = checkpoint_map.get(upsert_type) {
            let end_id = upsert_checkpoint.update_id.clone();
            for partner in &partners {
                let create_id = partner.create_id.clone();
                if is_entity_backfill_complete(&create_id, backfill_checkpoint) {
                    continue;
                }
                let start_id = get_start_id(&create_id, backfill_checkpoint);
                let backfill_options = SyncBackfillOptions {
                    now_id: options.now_id.clone(),
                    after_update_id: start_id,
                    before_update_id: end_id.clone(),
                };
                let backfill = partner_asset_get_backfill(
                    &self.pool,
                    &backfill_options,
                    &partner.shared_by_id,
                )
                .await?;
                for upsert in backfill {
                    push_line(
                        lines,
                        backfill_type,
                        &upsert.data,
                        &[&create_id, &upsert.update_id],
                        None,
                    );
                }
                push_entity_backfill_complete_ack(lines, backfill_type, &create_id);
            }
        } else if let Some(last) = partners.last() {
            self.upsert_backfill_checkpoint(session_id, backfill_type, &last.create_id)
                .await?;
        }

        let upserts = partner_asset_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_asset_exifs_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let upsert_type = SYNC_ENTITY_ASSET_EXIF_V1;
        let upserts = asset_exif_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_asset_edits_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ASSET_EDIT_DELETE_V1;
        let deletes = asset_edit_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_ASSET_EDIT_V1;
        let upserts = asset_edit_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_partner_asset_exifs_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
        session_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let backfill_type = SYNC_ENTITY_PARTNER_ASSET_EXIF_BACKFILL_V1;
        let backfill_checkpoint = checkpoint_map.get(backfill_type);
        let created_after_options = SyncCreatedAfterOptions {
            now_id: options.now_id.clone(),
            user_id: options.user_id,
            after_create_id: backfill_checkpoint.map(|ack| ack.update_id.clone()),
        };
        let partners = partner_get_created_after(&self.pool, &created_after_options).await?;

        let upsert_type = SYNC_ENTITY_PARTNER_ASSET_EXIF_V1;
        if let Some(upsert_checkpoint) = checkpoint_map.get(upsert_type) {
            let end_id = upsert_checkpoint.update_id.clone();
            for partner in &partners {
                let create_id = partner.create_id.clone();
                if is_entity_backfill_complete(&create_id, backfill_checkpoint) {
                    continue;
                }
                let start_id = get_start_id(&create_id, backfill_checkpoint);
                let backfill_options = SyncBackfillOptions {
                    now_id: options.now_id.clone(),
                    after_update_id: start_id,
                    before_update_id: end_id.clone(),
                };
                let backfill = partner_asset_exif_get_backfill(
                    &self.pool,
                    &backfill_options,
                    &partner.shared_by_id,
                )
                .await?;
                for upsert in backfill {
                    push_line(
                        lines,
                        backfill_type,
                        &upsert.data,
                        &[&create_id, &upsert.update_id],
                        None,
                    );
                }
                push_entity_backfill_complete_ack(lines, backfill_type, &create_id);
            }
        } else if let Some(last) = partners.last() {
            self.upsert_backfill_checkpoint(session_id, backfill_type, &last.create_id)
                .await?;
        }

        let upserts = partner_asset_exif_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_albums_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ALBUM_DELETE_V1;
        let deletes = album_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_ALBUM_V1;
        let upserts = album_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            let album_id = upsert
                .data
                .get("id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| ErrorResp::ServerError("Invalid album id".to_string()))?;
            let album_users = album_get_album_users(&self.pool, &album_id).await?;
            push_line(
                lines,
                upsert_type,
                &sync_album_v2_to_v1(&upsert.data, &album_users),
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_albums_v2(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ALBUM_DELETE_V1;
        let deletes = album_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_ALBUM_V2;
        let upserts = album_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_album_users_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
        session_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ALBUM_USER_DELETE_V1;
        let deletes = album_user_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let backfill_type = SYNC_ENTITY_ALBUM_USER_BACKFILL_V1;
        let backfill_checkpoint = checkpoint_map.get(backfill_type);
        let created_after_options = SyncCreatedAfterOptions {
            now_id: options.now_id.clone(),
            user_id: options.user_id,
            after_create_id: backfill_checkpoint.map(|ack| ack.update_id.clone()),
        };
        let albums = album_get_created_after(&self.pool, &created_after_options).await?;

        let upsert_type = SYNC_ENTITY_ALBUM_USER_V1;
        if let Some(upsert_checkpoint) = checkpoint_map.get(upsert_type) {
            let end_id = upsert_checkpoint.update_id.clone();
            for album in &albums {
                let create_id = album.create_id.clone();
                if is_entity_backfill_complete(&create_id, backfill_checkpoint) {
                    continue;
                }
                let start_id = get_start_id(&create_id, backfill_checkpoint);
                let backfill_options = SyncBackfillOptions {
                    now_id: options.now_id.clone(),
                    after_update_id: start_id,
                    before_update_id: end_id.clone(),
                };
                let backfill =
                    album_user_get_backfill(&self.pool, &backfill_options, &album.id).await?;
                for upsert in backfill {
                    push_line(
                        lines,
                        backfill_type,
                        &upsert.data,
                        &[&create_id, &upsert.update_id],
                        None,
                    );
                }
                push_entity_backfill_complete_ack(lines, backfill_type, &create_id);
            }
        } else if let Some(last) = albums.last() {
            self.upsert_backfill_checkpoint(session_id, backfill_type, &last.create_id)
                .await?;
        }

        let upserts = album_user_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_album_assets_v2(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
        session_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let backfill_type = SYNC_ENTITY_ALBUM_ASSET_BACKFILL_V2;
        let backfill_checkpoint = checkpoint_map.get(backfill_type);
        let created_after_options = SyncCreatedAfterOptions {
            now_id: options.now_id.clone(),
            user_id: options.user_id,
            after_create_id: backfill_checkpoint.map(|ack| ack.update_id.clone()),
        };
        let albums = album_get_created_after(&self.pool, &created_after_options).await?;

        let update_type = SYNC_ENTITY_ALBUM_ASSET_UPDATE_V2;
        let create_type = SYNC_ENTITY_ALBUM_ASSET_CREATE_V2;
        let update_checkpoint = checkpoint_map.get(update_type);
        let create_checkpoint = checkpoint_map.get(create_type);

        if create_checkpoint.is_some() {
            let end_id = create_checkpoint
                .as_ref()
                .map(|ack| ack.update_id.clone())
                .unwrap_or_default();
            for album in &albums {
                let create_id = album.create_id.clone();
                if is_entity_backfill_complete(&create_id, backfill_checkpoint) {
                    continue;
                }
                let start_id = get_start_id(&create_id, backfill_checkpoint);
                let backfill_options = SyncBackfillOptions {
                    now_id: options.now_id.clone(),
                    after_update_id: start_id,
                    before_update_id: end_id.clone(),
                };
                let backfill = album_asset_get_backfill(
                    &self.pool,
                    &backfill_options,
                    &album.id,
                    &options.user_id,
                )
                .await?;
                for upsert in backfill {
                    push_line(
                        lines,
                        backfill_type,
                        &upsert.data,
                        &[&create_id, &upsert.update_id],
                        None,
                    );
                }
                push_entity_backfill_complete_ack(lines, backfill_type, &create_id);
            }
        } else if let Some(last) = albums.last() {
            self.upsert_backfill_checkpoint(session_id, backfill_type, &last.create_id)
                .await?;
        }

        if let Some(create_checkpoint) = create_checkpoint {
            let updates = album_asset_get_updates(
                &self.pool,
                &with_ack(options, update_checkpoint),
                create_checkpoint,
            )
            .await?;
            for upsert in updates {
                push_line(
                    lines,
                    update_type,
                    &upsert.data,
                    &[&upsert.update_id],
                    None,
                );
            }
        }

        let creates = album_asset_get_creates(
            &self.pool,
            &with_ack(options, create_checkpoint),
        )
        .await?;
        let mut first = true;
        for upsert in creates {
            if first {
                push_line(
                    lines,
                    SYNC_ENTITY_SYNC_ACK_V1,
                    &json!({}),
                    &[&options.now_id],
                    Some(update_type),
                );
                first = false;
            }
            push_line(
                lines,
                create_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_album_asset_exifs_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
        session_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let backfill_type = SYNC_ENTITY_ALBUM_ASSET_EXIF_BACKFILL_V1;
        let backfill_checkpoint = checkpoint_map.get(backfill_type);
        let created_after_options = SyncCreatedAfterOptions {
            now_id: options.now_id.clone(),
            user_id: options.user_id,
            after_create_id: backfill_checkpoint.map(|ack| ack.update_id.clone()),
        };
        let albums = album_get_created_after(&self.pool, &created_after_options).await?;

        let update_type = SYNC_ENTITY_ALBUM_ASSET_EXIF_UPDATE_V1;
        let create_type = SYNC_ENTITY_ALBUM_ASSET_EXIF_CREATE_V1;
        let upsert_checkpoint = checkpoint_map.get(update_type);
        let create_checkpoint = checkpoint_map.get(create_type);

        if create_checkpoint.is_some() {
            let end_id = create_checkpoint
                .as_ref()
                .map(|ack| ack.update_id.clone())
                .unwrap_or_default();
            for album in &albums {
                let create_id = album.create_id.clone();
                if is_entity_backfill_complete(&create_id, backfill_checkpoint) {
                    continue;
                }
                let start_id = get_start_id(&create_id, backfill_checkpoint);
                let backfill_options = SyncBackfillOptions {
                    now_id: options.now_id.clone(),
                    after_update_id: start_id,
                    before_update_id: end_id.clone(),
                };
                let backfill =
                    album_asset_exif_get_backfill(&self.pool, &backfill_options, &album.id).await?;
                for upsert in backfill {
                    push_line(
                        lines,
                        backfill_type,
                        &upsert.data,
                        &[&create_id, &upsert.update_id],
                        None,
                    );
                }
                push_entity_backfill_complete_ack(lines, backfill_type, &create_id);
            }
        } else if let Some(last) = albums.last() {
            self.upsert_backfill_checkpoint(session_id, backfill_type, &last.create_id)
                .await?;
        }

        if create_checkpoint.is_some() {
            let updates = album_asset_exif_get_updates(
                &self.pool,
                &with_ack(options, upsert_checkpoint),
                create_checkpoint.as_ref().unwrap(),
            )
            .await?;
            for upsert in updates {
                push_line(
                    lines,
                    update_type,
                    &upsert.data,
                    &[&upsert.update_id],
                    None,
                );
            }
        }

        let creates = album_asset_exif_get_creates(
            &self.pool,
            &with_ack(options, create_checkpoint),
        )
        .await?;
        let mut first = true;
        for upsert in creates {
            if first {
                push_line(
                    lines,
                    SYNC_ENTITY_SYNC_ACK_V1,
                    &json!({}),
                    &[&options.now_id],
                    Some(update_type),
                );
                first = false;
            }
            push_line(
                lines,
                create_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_album_to_assets_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
        session_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ALBUM_TO_ASSET_DELETE_V1;
        let deletes = album_to_asset_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let backfill_type = SYNC_ENTITY_ALBUM_TO_ASSET_BACKFILL_V1;
        let backfill_checkpoint = checkpoint_map.get(backfill_type);
        let created_after_options = SyncCreatedAfterOptions {
            now_id: options.now_id.clone(),
            user_id: options.user_id,
            after_create_id: backfill_checkpoint.map(|ack| ack.update_id.clone()),
        };
        let albums = album_get_created_after(&self.pool, &created_after_options).await?;

        let upsert_type = SYNC_ENTITY_ALBUM_TO_ASSET_V1;
        if let Some(upsert_checkpoint) = checkpoint_map.get(upsert_type) {
            let end_id = upsert_checkpoint.update_id.clone();
            for album in &albums {
                let create_id = album.create_id.clone();
                if is_entity_backfill_complete(&create_id, backfill_checkpoint) {
                    continue;
                }
                let start_id = get_start_id(&create_id, backfill_checkpoint);
                let backfill_options = SyncBackfillOptions {
                    now_id: options.now_id.clone(),
                    after_update_id: start_id,
                    before_update_id: end_id.clone(),
                };
                let backfill =
                    album_to_asset_get_backfill(&self.pool, &backfill_options, &album.id).await?;
                for upsert in backfill {
                    push_line(
                        lines,
                        backfill_type,
                        &upsert.data,
                        &[&create_id, &upsert.update_id],
                        None,
                    );
                }
                push_entity_backfill_complete_ack(lines, backfill_type, &create_id);
            }
        } else if let Some(last) = albums.last() {
            self.upsert_backfill_checkpoint(session_id, backfill_type, &last.create_id)
                .await?;
        }

        let upserts = album_to_asset_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_memories_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_MEMORY_DELETE_V1;
        let deletes = memory_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_MEMORY_V1;
        let upserts = memory_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_memory_assets_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_MEMORY_TO_ASSET_DELETE_V1;
        let deletes = memory_to_asset_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_MEMORY_TO_ASSET_V1;
        let upserts = memory_to_asset_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_stack_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_STACK_DELETE_V1;
        let deletes = stack_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_STACK_V1;
        let upserts = stack_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_partner_stack_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
        session_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_PARTNER_STACK_DELETE_V1;
        let deletes = partner_stack_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let backfill_type = SYNC_ENTITY_PARTNER_STACK_BACKFILL_V1;
        let backfill_checkpoint = checkpoint_map.get(backfill_type);
        let created_after_options = SyncCreatedAfterOptions {
            now_id: options.now_id.clone(),
            user_id: options.user_id,
            after_create_id: backfill_checkpoint.map(|ack| ack.update_id.clone()),
        };
        let partners = partner_get_created_after(&self.pool, &created_after_options).await?;

        let upsert_type = SYNC_ENTITY_PARTNER_STACK_V1;
        if let Some(upsert_checkpoint) = checkpoint_map.get(upsert_type) {
            let end_id = upsert_checkpoint.update_id.clone();
            for partner in &partners {
                let create_id = partner.create_id.clone();
                if is_entity_backfill_complete(&create_id, backfill_checkpoint) {
                    continue;
                }
                let start_id = get_start_id(&create_id, backfill_checkpoint);
                let backfill_options = SyncBackfillOptions {
                    now_id: options.now_id.clone(),
                    after_update_id: start_id,
                    before_update_id: end_id.clone(),
                };
                let backfill = partner_stack_get_backfill(
                    &self.pool,
                    &backfill_options,
                    &partner.shared_by_id,
                )
                .await?;
                for upsert in backfill {
                    push_line(
                        lines,
                        backfill_type,
                        &upsert.data,
                        &[&create_id, &upsert.update_id],
                        None,
                    );
                }
                push_entity_backfill_complete_ack(lines, backfill_type, &create_id);
            }
        } else if let Some(last) = partners.last() {
            self.upsert_backfill_checkpoint(session_id, backfill_type, &last.create_id)
                .await?;
        }

        let upserts = partner_stack_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_people_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_PERSON_DELETE_V1;
        let deletes = person_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_PERSON_V1;
        let upserts = person_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_asset_faces_v2(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ASSET_FACE_DELETE_V1;
        let deletes = asset_face_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_ASSET_FACE_V2;
        let upserts = asset_face_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_user_metadata_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_USER_METADATA_DELETE_V1;
        let deletes = user_metadata_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_USER_METADATA_V1;
        let upserts = user_metadata_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_asset_metadata_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ASSET_METADATA_DELETE_V1;
        let deletes = asset_metadata_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            push_line(
                lines,
                delete_type,
                &delete.data,
                &[&delete.audit_id],
                None,
            );
        }

        let upsert_type = SYNC_ENTITY_ASSET_METADATA_V1;
        let upserts = asset_metadata_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }

    async fn sync_asset_ocr_v1(
        &self,
        options: &SyncQueryOptions,
        lines: &mut Vec<String>,
        checkpoint_map: &CheckpointMap,
    ) -> Result<(), ErrorResp> {
        let delete_type = SYNC_ENTITY_ASSET_OCR_DELETE_V1;
        let deletes = asset_ocr_get_deletes(
            &self.pool,
            &with_ack(options, checkpoint_map.get(delete_type)),
        )
        .await?;
        for delete in deletes {
            let mut data = delete.data.clone();
            if let Some(obj) = data.as_object_mut() {
                obj.insert("id".to_string(), json!(delete.audit_id));
            }
            push_line(lines, delete_type, &data, &[&delete.audit_id], None);
        }

        let upsert_type = SYNC_ENTITY_ASSET_OCR_V1;
        let upserts = asset_ocr_get_upserts(
            &self.pool,
            &with_ack(options, checkpoint_map.get(upsert_type)),
        )
        .await?;
        for upsert in upserts {
            push_line(
                lines,
                upsert_type,
                &upsert.data,
                &[&upsert.update_id],
                None,
            );
        }
        Ok(())
    }
}

fn build_checkpoint_map(checkpoints: &[SyncCheckpointRow]) -> CheckpointMap {
    checkpoints
        .iter()
        .map(|row| (row.r#type.clone(), from_ack(&row.ack)))
        .collect()
}

fn needs_full_sync(checkpoint_map: &CheckpointMap) -> bool {
    let Some(complete_ack) = checkpoint_map.get(SYNC_ENTITY_SYNC_COMPLETE_V1) else {
        return false;
    };
    let hex_str: String = complete_ack.update_id.replace('-', "");
    let hex_prefix = &hex_str[..hex_str.len().min(12)];
    let Ok(milliseconds) = u64::from_str_radix(hex_prefix, 16) else {
        return false;
    };
    let Some(timestamp) = chrono::DateTime::from_timestamp_millis(milliseconds as i64) else {
        return false;
    };
    timestamp < Utc::now() - Duration::days(MAX_DAYS)
}

fn is_entity_backfill_complete(create_id: &str, checkpoint: Option<&SyncAck>) -> bool {
    checkpoint.is_some_and(|ack| {
        ack.update_id == create_id && ack.extra_id.as_deref() == Some(COMPLETE_ID)
    })
}

fn get_start_id(create_id: &str, checkpoint: Option<&SyncAck>) -> Option<String> {
    checkpoint.and_then(|ack| {
        if ack.update_id == create_id {
            ack.extra_id.clone()
        } else {
            None
        }
    })
}

fn with_ack(options: &SyncQueryOptions, ack: Option<&SyncAck>) -> SyncQueryOptions {
    SyncQueryOptions {
        ack: ack.cloned(),
        ..options.clone()
    }
}

fn push_line(
    lines: &mut Vec<String>,
    sync_type: &str,
    data: &Value,
    ids: &[&str],
    ack_type: Option<&str>,
) {
    lines.push(serialize(sync_type, data, ids, ack_type));
}

fn push_entity_backfill_complete_ack(lines: &mut Vec<String>, ack_type: &str, id: &str) {
    push_line(
        lines,
        SYNC_ENTITY_SYNC_ACK_V1,
        &json!({}),
        &[id, COMPLETE_ID],
        Some(ack_type),
    );
}

fn user_data_with_profile_flag(mut data: Value) -> Value {
    if let Some(obj) = data.as_object_mut() {
        let has_profile = obj
            .get("profileImagePath")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        obj.remove("profileImagePath");
        obj.insert("hasProfileImage".to_string(), json!(has_profile));
    }
    data
}

pub fn sync_album_v2_to_v1(album: &Value, album_users: &[AlbumUserRow]) -> Value {
    let owner = album_users
        .iter()
        .find(|user| user.role == "owner")
        .expect("album owner");
    let mut result = album.clone();
    if let Some(obj) = result.as_object_mut() {
        obj.insert("ownerId".to_string(), json!(owner.user_id));
    }
    result
}

fn is_valid_sync_entity_type(sync_type: &str) -> bool {
    matches!(
        sync_type,
        SYNC_ENTITY_AUTH_USER_V1
            | SYNC_ENTITY_USER_V1
            | SYNC_ENTITY_USER_DELETE_V1
            | "AssetV1"
            | SYNC_ENTITY_ASSET_V2
            | SYNC_ENTITY_ASSET_DELETE_V1
            | SYNC_ENTITY_ASSET_EXIF_V1
            | SYNC_ENTITY_ASSET_EDIT_V1
            | SYNC_ENTITY_ASSET_EDIT_DELETE_V1
            | SYNC_ENTITY_ASSET_METADATA_V1
            | SYNC_ENTITY_ASSET_METADATA_DELETE_V1
            | SYNC_ENTITY_ASSET_OCR_V1
            | SYNC_ENTITY_ASSET_OCR_DELETE_V1
            | SYNC_ENTITY_PARTNER_V1
            | SYNC_ENTITY_PARTNER_DELETE_V1
            | "PartnerAssetV1"
            | SYNC_ENTITY_PARTNER_ASSET_V2
            | "PartnerAssetBackfillV1"
            | SYNC_ENTITY_PARTNER_ASSET_BACKFILL_V2
            | SYNC_ENTITY_PARTNER_ASSET_DELETE_V1
            | SYNC_ENTITY_PARTNER_ASSET_EXIF_V1
            | SYNC_ENTITY_PARTNER_ASSET_EXIF_BACKFILL_V1
            | SYNC_ENTITY_PARTNER_STACK_BACKFILL_V1
            | SYNC_ENTITY_PARTNER_STACK_DELETE_V1
            | SYNC_ENTITY_PARTNER_STACK_V1
            | SYNC_ENTITY_ALBUM_V1
            | SYNC_ENTITY_ALBUM_V2
            | SYNC_ENTITY_ALBUM_DELETE_V1
            | SYNC_ENTITY_ALBUM_USER_V1
            | SYNC_ENTITY_ALBUM_USER_BACKFILL_V1
            | SYNC_ENTITY_ALBUM_USER_DELETE_V1
            | "AlbumAssetCreateV1"
            | SYNC_ENTITY_ALBUM_ASSET_CREATE_V2
            | "AlbumAssetUpdateV1"
            | SYNC_ENTITY_ALBUM_ASSET_UPDATE_V2
            | "AlbumAssetBackfillV1"
            | SYNC_ENTITY_ALBUM_ASSET_BACKFILL_V2
            | SYNC_ENTITY_ALBUM_ASSET_EXIF_CREATE_V1
            | SYNC_ENTITY_ALBUM_ASSET_EXIF_UPDATE_V1
            | SYNC_ENTITY_ALBUM_ASSET_EXIF_BACKFILL_V1
            | SYNC_ENTITY_ALBUM_TO_ASSET_V1
            | SYNC_ENTITY_ALBUM_TO_ASSET_DELETE_V1
            | SYNC_ENTITY_ALBUM_TO_ASSET_BACKFILL_V1
            | SYNC_ENTITY_MEMORY_V1
            | SYNC_ENTITY_MEMORY_DELETE_V1
            | SYNC_ENTITY_MEMORY_TO_ASSET_V1
            | SYNC_ENTITY_MEMORY_TO_ASSET_DELETE_V1
            | SYNC_ENTITY_STACK_V1
            | SYNC_ENTITY_STACK_DELETE_V1
            | SYNC_ENTITY_PERSON_V1
            | SYNC_ENTITY_PERSON_DELETE_V1
            | "AssetFaceV1"
            | SYNC_ENTITY_ASSET_FACE_V2
            | SYNC_ENTITY_ASSET_FACE_DELETE_V1
            | SYNC_ENTITY_USER_METADATA_V1
            | SYNC_ENTITY_USER_METADATA_DELETE_V1
            | SYNC_ENTITY_SYNC_ACK_V1
            | SYNC_ENTITY_SYNC_RESET_V1
            | SYNC_ENTITY_SYNC_COMPLETE_V1
    )
}
