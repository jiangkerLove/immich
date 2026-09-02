use serde::{Serialize, Deserialize};
use serde::de::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    All, // 'all'

    ActivityCreate, // 'activity.create'
    ActivityRead,
    ActivityUpdate,
    ActivityDelete,
    ActivityStatistics,

    ApiKeyCreate,
    ApiKeyRead,
    ApiKeyUpdate,
    ApiKeyDelete,

    DuplicateRead,
    DuplicateDelete,

    AssetEditGet,
    AssetEditCreate,
    AssetEditDelete,

    AssetRead,
    AssetUpdate,
    AssetDelete,
    AssetShare,
    AssetFileRead,
    AssetFileDownload,
    AssetFileDelete,
    AssetView,
    AssetDownload,
    AssetUpload,
    AssetCopy,
    AssetStatistics,

    AlbumCreate,
    AlbumRead,
    AlbumUpdate,
    AlbumDelete,
    AlbumStatistics,
    AlbumAddAsset,
    AlbumRemoveAsset,
    AlbumShare,
    AlbumDownload,

    AuthChangePassword,

    AuthDeviceDelete,

    PinCodeCreate,
    PinCodeUpdate,
    PinCodeDelete,

    ArchiveRead,
    FolderRead,

    FaceCreate,
    FaceRead,
    FaceUpdate,
    FaceDelete,

    LibraryCreate,
    LibraryRead,
    LibraryUpdate,
    LibraryDelete,
    LibraryStatistics,

    TimelineRead,
    TimelineDownload,

    MemoryCreate,
    MemoryRead,
    MemoryUpdate,
    MemoryDelete,
    MemoryStatistics,
    MemoryAssetCreate,
    MemoryAssetDelete,

    MapRead,
    MapSearch,

    NotificationCreate,
    NotificationRead,
    NotificationUpdate,
    NotificationDelete,

    PartnerCreate,
    PartnerRead,
    PartnerUpdate,
    PartnerDelete,

    PersonCreate,
    PersonRead,
    PersonUpdate,
    PersonDelete,
    PersonStatistics,
    PersonMerge,
    PersonReassign,

    SessionCreate,
    SessionRead,
    SessionUpdate,
    SessionDelete,
    SessionLock,

    SharedLinkCreate,
    SharedLinkRead,
    SharedLinkUpdate,
    SharedLinkDelete,

    StackCreate,
    StackRead,
    StackUpdate,
    StackDelete,

    SyncStream,
    SyncCheckpointRead,
    SyncCheckpointUpdate,
    SyncCheckpointDelete,

    SystemConfigRead,
    SystemConfigUpdate,

    SystemMetadataRead,
    SystemMetadataUpdate,

    PluginRead,

    WorkflowCreate,
    WorkflowRead,
    WorkflowUpdate,
    WorkflowDelete,

    ServerLicenseRead,
    ServerLicenseUpdate,
    ServerLicenseDelete,
    ServerVersionCheck,

    AdminAuthUnlinkAll,

    TagCreate,
    TagRead,
    TagUpdate,
    TagDelete,
    TagAsset,

    AdminUserCreate,
    AdminUserRead,
    AdminUserUpdate,
    AdminUserDelete,
    AdminSessionRead,

    UserRead,
    UserUpdate,
    UserPreferenceRead,
    UserPreferenceUpdate,
    UserLicenseRead,
    UserLicenseUpdate,
    UserLicenseDelete,
    UserOnboardingRead,
    UserOnboardingUpdate,
    UserOnboardingDelete,
    UserProfileImageRead,
    UserProfileImageUpdate,
    UserProfileImageDelete,
}

impl Permission {
    /// 将枚举转换为字符串（如 `ActivityCreate` -> "activity.create"）
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::All => "all",
            Permission::ActivityCreate => "activity.create",
            Permission::ActivityRead => "activity.read",
            Permission::ActivityUpdate => "activity.update",
            Permission::ActivityDelete => "activity.delete",
            Permission::ActivityStatistics => "activity.statistics",

            Permission::ApiKeyCreate => "apiKey.create",
            Permission::ApiKeyRead => "apiKey.read",
            Permission::ApiKeyUpdate => "apiKey.update",
            Permission::ApiKeyDelete => "apiKey.delete",

            Permission::DuplicateRead => "duplicate.read",
            Permission::DuplicateDelete => "duplicate.delete",

            Permission::AssetEditGet => "asset.edit.get",
            Permission::AssetEditCreate => "asset.edit.create",
            Permission::AssetEditDelete => "asset.edit.delete",

            Permission::AssetRead => "asset.read",
            Permission::AssetUpdate => "asset.update",
            Permission::AssetDelete => "asset.delete",
            Permission::AssetShare => "asset.share",
            Permission::AssetFileRead => "assetFile.read",
            Permission::AssetFileDownload => "assetFile.download",
            Permission::AssetFileDelete => "assetFile.delete",
            Permission::AssetView => "asset.view",
            Permission::AssetDownload => "asset.download",
            Permission::AssetUpload => "asset.upload",
            Permission::AssetCopy => "asset.copy",
            Permission::AssetStatistics => "asset.statistics",

            Permission::AlbumCreate => "album.create",
            Permission::AlbumRead => "album.read",
            Permission::AlbumUpdate => "album.update",
            Permission::AlbumDelete => "album.delete",
            Permission::AlbumStatistics => "album.statistics",
            Permission::AlbumAddAsset => "album.addAsset",
            Permission::AlbumRemoveAsset => "album.removeAsset",
            Permission::AlbumShare => "album.share",
            Permission::AlbumDownload => "album.download",

            Permission::AuthChangePassword => "auth.changePassword",

            Permission::AuthDeviceDelete => "authDevice.delete",

            Permission::PinCodeCreate => "pinCode.create",
            Permission::PinCodeUpdate => "pinCode.update",
            Permission::PinCodeDelete => "pinCode.delete",

            Permission::ArchiveRead => "archive.read",
            Permission::FolderRead => "folder.read",

            Permission::FaceCreate => "face.create",
            Permission::FaceRead => "face.read",
            Permission::FaceUpdate => "face.update",
            Permission::FaceDelete => "face.delete",

            Permission::LibraryCreate => "library.create",
            Permission::LibraryRead => "library.read",
            Permission::LibraryUpdate => "library.update",
            Permission::LibraryDelete => "library.delete",
            Permission::LibraryStatistics => "library.statistics",

            Permission::TimelineRead => "timeline.read",
            Permission::TimelineDownload => "timeline.download",

            Permission::MemoryCreate => "memory.create",
            Permission::MemoryRead => "memory.read",
            Permission::MemoryUpdate => "memory.update",
            Permission::MemoryDelete => "memory.delete",
            Permission::MemoryStatistics => "memory.statistics",
            Permission::MemoryAssetCreate => "memoryAsset.create",
            Permission::MemoryAssetDelete => "memoryAsset.delete",

            Permission::MapRead => "map.read",
            Permission::MapSearch => "map.search",

            Permission::NotificationCreate => "notification.create",
            Permission::NotificationRead => "notification.read",
            Permission::NotificationUpdate => "notification.update",
            Permission::NotificationDelete => "notification.delete",

            Permission::PartnerCreate => "partner.create",
            Permission::PartnerRead => "partner.read",
            Permission::PartnerUpdate => "partner.update",
            Permission::PartnerDelete => "partner.delete",

            Permission::PersonCreate => "person.create",
            Permission::PersonRead => "person.read",
            Permission::PersonUpdate => "person.update",
            Permission::PersonDelete => "person.delete",
            Permission::PersonStatistics => "person.statistics",
            Permission::PersonMerge => "person.merge",
            Permission::PersonReassign => "person.reassign",

            Permission::SessionCreate => "session.create",
            Permission::SessionRead => "session.read",
            Permission::SessionUpdate => "session.update",
            Permission::SessionDelete => "session.delete",
            Permission::SessionLock => "session.lock",

            Permission::SharedLinkCreate => "sharedLink.create",
            Permission::SharedLinkRead => "sharedLink.read",
            Permission::SharedLinkUpdate => "sharedLink.update",
            Permission::SharedLinkDelete => "sharedLink.delete",

            Permission::StackCreate => "stack.create",
            Permission::StackRead => "stack.read",
            Permission::StackUpdate => "stack.update",
            Permission::StackDelete => "stack.delete",

            Permission::SyncStream => "sync.stream",
            Permission::SyncCheckpointRead => "syncCheckpoint.read",
            Permission::SyncCheckpointUpdate => "syncCheckpoint.update",
            Permission::SyncCheckpointDelete => "syncCheckpoint.delete",

            Permission::SystemConfigRead => "systemConfig.read",
            Permission::SystemConfigUpdate => "systemConfig.update",

            Permission::SystemMetadataRead => "systemMetadata.read",
            Permission::SystemMetadataUpdate => "systemMetadata.update",

            Permission::PluginRead => "plugin.read",

            Permission::WorkflowCreate => "workflow.create",
            Permission::WorkflowRead => "workflow.read",
            Permission::WorkflowUpdate => "workflow.update",
            Permission::WorkflowDelete => "workflow.delete",

            Permission::ServerLicenseRead => "serverLicense.read",
            Permission::ServerLicenseUpdate => "serverLicense.update",
            Permission::ServerLicenseDelete => "serverLicense.delete",
            Permission::ServerVersionCheck => "server.versionCheck",

            Permission::AdminAuthUnlinkAll => "adminAuth.unlinkAll",

            Permission::TagCreate => "tag.create",
            Permission::TagRead => "tag.read",
            Permission::TagUpdate => "tag.update",
            Permission::TagDelete => "tag.delete",
            Permission::TagAsset => "tag.asset",

            Permission::AdminUserCreate => "admin.user.create",
            Permission::AdminUserRead => "admin.user.read",
            Permission::AdminUserUpdate => "admin.user.update",
            Permission::AdminUserDelete => "admin.user.delete",
            Permission::AdminSessionRead => "adminSession.read",

            Permission::UserRead => "user.read",
            Permission::UserUpdate => "user.update",
            Permission::UserPreferenceRead => "userPreference.read",
            Permission::UserPreferenceUpdate => "userPreference.update",
            Permission::UserLicenseRead => "userLicense.read",
            Permission::UserLicenseUpdate => "userLicense.update",
            Permission::UserLicenseDelete => "userLicense.delete",
            Permission::UserOnboardingRead => "userOnboarding.read",
            Permission::UserOnboardingUpdate => "userOnboarding.update",
            Permission::UserOnboardingDelete => "userOnboarding.delete",
            Permission::UserProfileImageRead => "userProfileImage.read",
            Permission::UserProfileImageUpdate => "userProfileImage.update",
            Permission::UserProfileImageDelete => "userProfileImage.delete",
        }
    }

    /// 从字符串解析为枚举（如 "activity.create" -> `ActivityCreate`）
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "all" => Some(Permission::All),
            "activity.create" => Some(Permission::ActivityCreate),
            "activity.read" => Some(Permission::ActivityRead),
            "activity.update" => Some(Permission::ActivityUpdate),
            "activity.delete" => Some(Permission::ActivityDelete),
            "activity.statistics" => Some(Permission::ActivityStatistics),

            "apiKey.create" => Some(Permission::ApiKeyCreate),
            "apiKey.read" => Some(Permission::ApiKeyRead),
            "apiKey.update" => Some(Permission::ApiKeyUpdate),
            "apiKey.delete" => Some(Permission::ApiKeyDelete),

            "duplicate.read" => Some(Permission::DuplicateRead),
            "duplicate.delete" => Some(Permission::DuplicateDelete),

            "asset.edit.get" => Some(Permission::AssetEditGet),
            "asset.edit.create" => Some(Permission::AssetEditCreate),
            "asset.edit.delete" => Some(Permission::AssetEditDelete),

            "asset.read" => Some(Permission::AssetRead),
            "asset.update" => Some(Permission::AssetUpdate),
            "asset.delete" => Some(Permission::AssetDelete),
            "asset.share" => Some(Permission::AssetShare),
            "assetFile.read" => Some(Permission::AssetFileRead),
            "assetFile.download" => Some(Permission::AssetFileDownload),
            "assetFile.delete" => Some(Permission::AssetFileDelete),
            "asset.view" => Some(Permission::AssetView),
            "asset.download" => Some(Permission::AssetDownload),
            "asset.upload" => Some(Permission::AssetUpload),
            "asset.copy" => Some(Permission::AssetCopy),
            "asset.statistics" => Some(Permission::AssetStatistics),

            "album.create" => Some(Permission::AlbumCreate),
            "album.read" => Some(Permission::AlbumRead),
            "album.update" => Some(Permission::AlbumUpdate),
            "album.delete" => Some(Permission::AlbumDelete),
            "album.statistics" => Some(Permission::AlbumStatistics),
            "album.addAsset" => Some(Permission::AlbumAddAsset),
            "album.removeAsset" => Some(Permission::AlbumRemoveAsset),
            "album.share" => Some(Permission::AlbumShare),
            "album.download" => Some(Permission::AlbumDownload),

            "auth.changePassword" => Some(Permission::AuthChangePassword),

            "authDevice.delete" => Some(Permission::AuthDeviceDelete),

            "pinCode.create" => Some(Permission::PinCodeCreate),
            "pinCode.update" => Some(Permission::PinCodeUpdate),
            "pinCode.delete" => Some(Permission::PinCodeDelete),

            "archive.read" => Some(Permission::ArchiveRead),
            "folder.read" => Some(Permission::FolderRead),

            "face.create" => Some(Permission::FaceCreate),
            "face.read" => Some(Permission::FaceRead),
            "face.update" => Some(Permission::FaceUpdate),
            "face.delete" => Some(Permission::FaceDelete),

            "library.create" => Some(Permission::LibraryCreate),
            "library.read" => Some(Permission::LibraryRead),
            "library.update" => Some(Permission::LibraryUpdate),
            "library.delete" => Some(Permission::LibraryDelete),
            "library.statistics" => Some(Permission::LibraryStatistics),

            "timeline.read" => Some(Permission::TimelineRead),
            "timeline.download" => Some(Permission::TimelineDownload),

            "memory.create" => Some(Permission::MemoryCreate),
            "memory.read" => Some(Permission::MemoryRead),
            "memory.update" => Some(Permission::MemoryUpdate),
            "memory.delete" => Some(Permission::MemoryDelete),
            "memory.statistics" => Some(Permission::MemoryStatistics),
            "memoryAsset.create" => Some(Permission::MemoryAssetCreate),
            "memoryAsset.delete" => Some(Permission::MemoryAssetDelete),

            "map.read" => Some(Permission::MapRead),
            "map.search" => Some(Permission::MapSearch),

            "notification.create" => Some(Permission::NotificationCreate),
            "notification.read" => Some(Permission::NotificationRead),
            "notification.update" => Some(Permission::NotificationUpdate),
            "notification.delete" => Some(Permission::NotificationDelete),

            "partner.create" => Some(Permission::PartnerCreate),
            "partner.read" => Some(Permission::PartnerRead),
            "partner.update" => Some(Permission::PartnerUpdate),
            "partner.delete" => Some(Permission::PartnerDelete),

            "person.create" => Some(Permission::PersonCreate),
            "person.read" => Some(Permission::PersonRead),
            "person.update" => Some(Permission::PersonUpdate),
            "person.delete" => Some(Permission::PersonDelete),
            "person.statistics" => Some(Permission::PersonStatistics),
            "person.merge" => Some(Permission::PersonMerge),
            "person.reassign" => Some(Permission::PersonReassign),

            "session.create" => Some(Permission::SessionCreate),
            "session.read" => Some(Permission::SessionRead),
            "session.update" => Some(Permission::SessionUpdate),
            "session.delete" => Some(Permission::SessionDelete),
            "session.lock" => Some(Permission::SessionLock),

            "sharedLink.create" => Some(Permission::SharedLinkCreate),
            "sharedLink.read" => Some(Permission::SharedLinkRead),
            "sharedLink.update" => Some(Permission::SharedLinkUpdate),
            "sharedLink.delete" => Some(Permission::SharedLinkDelete),

            "stack.create" => Some(Permission::StackCreate),
            "stack.read" => Some(Permission::StackRead),
            "stack.update" => Some(Permission::StackUpdate),
            "stack.delete" => Some(Permission::StackDelete),

            "sync.stream" => Some(Permission::SyncStream),
            "syncCheckpoint.read" => Some(Permission::SyncCheckpointRead),
            "syncCheckpoint.update" => Some(Permission::SyncCheckpointUpdate),
            "syncCheckpoint.delete" => Some(Permission::SyncCheckpointDelete),

            "systemConfig.read" => Some(Permission::SystemConfigRead),
            "systemConfig.update" => Some(Permission::SystemConfigUpdate),

            "systemMetadata.read" => Some(Permission::SystemMetadataRead),
            "systemMetadata.update" => Some(Permission::SystemMetadataUpdate),

            "plugin.read" => Some(Permission::PluginRead),

            "workflow.create" => Some(Permission::WorkflowCreate),
            "workflow.read" => Some(Permission::WorkflowRead),
            "workflow.update" => Some(Permission::WorkflowUpdate),
            "workflow.delete" => Some(Permission::WorkflowDelete),

            "serverLicense.read" => Some(Permission::ServerLicenseRead),
            "serverLicense.update" => Some(Permission::ServerLicenseUpdate),
            "serverLicense.delete" => Some(Permission::ServerLicenseDelete),
            "server.versionCheck" => Some(Permission::ServerVersionCheck),

            "adminAuth.unlinkAll" => Some(Permission::AdminAuthUnlinkAll),

            "tag.create" => Some(Permission::TagCreate),
            "tag.read" => Some(Permission::TagRead),
            "tag.update" => Some(Permission::TagUpdate),
            "tag.delete" => Some(Permission::TagDelete),
            "tag.asset" => Some(Permission::TagAsset),

            "admin.user.create" => Some(Permission::AdminUserCreate),
            "admin.user.read" => Some(Permission::AdminUserRead),
            "admin.user.update" => Some(Permission::AdminUserUpdate),
            "admin.user.delete" => Some(Permission::AdminUserDelete),
            "adminSession.read" => Some(Permission::AdminSessionRead),

            "user.read" => Some(Permission::UserRead),
            "user.update" => Some(Permission::UserUpdate),
            "userPreference.read" => Some(Permission::UserPreferenceRead),
            "userPreference.update" => Some(Permission::UserPreferenceUpdate),
            "userLicense.read" => Some(Permission::UserLicenseRead),
            "userLicense.update" => Some(Permission::UserLicenseUpdate),
            "userLicense.delete" => Some(Permission::UserLicenseDelete),
            "userOnboarding.read" => Some(Permission::UserOnboardingRead),
            "userOnboarding.update" => Some(Permission::UserOnboardingUpdate),
            "userOnboarding.delete" => Some(Permission::UserOnboardingDelete),
            "userProfileImage.read" => Some(Permission::UserProfileImageRead),
            "userProfileImage.update" => Some(Permission::UserProfileImageUpdate),
            "userProfileImage.delete" => Some(Permission::UserProfileImageDelete),

            _ => None,
        }
    }
}

impl Serialize for Permission {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Permission {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Permission::from_str(&s)
            .ok_or_else(|| D::Error::custom(format!("Unknown permission: '{}'", s)))
    }
}