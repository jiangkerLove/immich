use serde::de::Error;
use serde::{Deserialize, Serialize};

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
    ApiKeyRotate,

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
    AssetDerive,
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
    AlbumUserCreate,
    AlbumUserUpdate,
    AlbumUserDelete,

    AuthChangePassword,

    ClusterGroupRead,
    ClusterGroupLeave,
    ClusterGroupRequestCreate,
    ClusterGroupRequestRead,
    ClusterGroupRequestDelete,

    AuthDeviceDelete,

    BackupList,
    BackupDownload,
    BackupUpload,
    BackupDelete,

    JobCreate,
    JobRead,

    Maintenance,

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

    AdminConfigRead,
    AdminConfigUpdate,

    UserConfigRead,

    SystemMetadataRead,
    SystemMetadataUpdate,

    PluginCreate,
    PluginRead,
    PluginUpdate,
    PluginDelete,

    WorkflowCreate,
    WorkflowRead,
    WorkflowUpdate,
    WorkflowDelete,
    WorkflowLogs,

    ServerLicenseRead,
    ServerLicenseUpdate,
    ServerLicenseDelete,
    ServerVersionCheck,
    ServerAbout,
    ServerApkLinks,
    ServerStorage,
    ServerStatistics,

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
    UserLicenseCreate,
    UserLicenseRead,
    UserLicenseUpdate,
    UserLicenseDelete,
    UserOnboardingRead,
    UserOnboardingUpdate,
    UserOnboardingDelete,
    UserProfileImageCreate,
    UserProfileImageRead,
    UserProfileImageUpdate,
    UserProfileImageDelete,

    QueueRead,
    QueueUpdate,
    QueueJobCreate,
    QueueJobRead,
    QueueJobUpdate,
    QueueJobDelete,
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
            Permission::ApiKeyRotate => "apiKey.rotate",

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
            Permission::AssetDerive => "asset.derive",
            Permission::AssetStatistics => "asset.statistics",

            Permission::AlbumCreate => "album.create",
            Permission::AlbumRead => "album.read",
            Permission::AlbumUpdate => "album.update",
            Permission::AlbumDelete => "album.delete",
            Permission::AlbumStatistics => "album.statistics",
            Permission::AlbumAddAsset => "albumAsset.create",
            Permission::AlbumRemoveAsset => "albumAsset.delete",
            Permission::AlbumShare => "album.share",
            Permission::AlbumDownload => "album.download",
            Permission::AlbumUserCreate => "albumUser.create",
            Permission::AlbumUserUpdate => "albumUser.update",
            Permission::AlbumUserDelete => "albumUser.delete",

            Permission::AuthChangePassword => "auth.changePassword",

            Permission::ClusterGroupRead => "clusterGroup.read",
            Permission::ClusterGroupLeave => "clusterGroup.leave",
            Permission::ClusterGroupRequestCreate => "clusterGroupRequest.create",
            Permission::ClusterGroupRequestRead => "clusterGroupRequest.read",
            Permission::ClusterGroupRequestDelete => "clusterGroupRequest.delete",

            Permission::AuthDeviceDelete => "authDevice.delete",
            Permission::BackupList => "backup.list",
            Permission::BackupDownload => "backup.download",
            Permission::BackupUpload => "backup.upload",
            Permission::BackupDelete => "backup.delete",
            Permission::JobCreate => "job.create",
            Permission::JobRead => "job.read",
            Permission::Maintenance => "maintenance",

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
            Permission::AdminConfigRead => "adminConfig.read",
            Permission::AdminConfigUpdate => "adminConfig.update",
            Permission::UserConfigRead => "userConfig.read",

            Permission::SystemMetadataRead => "systemMetadata.read",
            Permission::SystemMetadataUpdate => "systemMetadata.update",

            Permission::PluginCreate => "plugin.create",
            Permission::PluginRead => "plugin.read",
            Permission::PluginUpdate => "plugin.update",
            Permission::PluginDelete => "plugin.delete",

            Permission::WorkflowCreate => "workflow.create",
            Permission::WorkflowRead => "workflow.read",
            Permission::WorkflowUpdate => "workflow.update",
            Permission::WorkflowDelete => "workflow.delete",
            Permission::WorkflowLogs => "workflow.logs",

            Permission::ServerLicenseRead => "serverLicense.read",
            Permission::ServerLicenseUpdate => "serverLicense.update",
            Permission::ServerLicenseDelete => "serverLicense.delete",
            Permission::ServerVersionCheck => "server.versionCheck",
            Permission::ServerAbout => "server.about",
            Permission::ServerApkLinks => "server.apkLinks",
            Permission::ServerStorage => "server.storage",
            Permission::ServerStatistics => "server.statistics",

            Permission::AdminAuthUnlinkAll => "adminAuth.unlinkAll",

            Permission::TagCreate => "tag.create",
            Permission::TagRead => "tag.read",
            Permission::TagUpdate => "tag.update",
            Permission::TagDelete => "tag.delete",
            Permission::TagAsset => "tag.asset",

            Permission::AdminUserCreate => "adminUser.create",
            Permission::AdminUserRead => "adminUser.read",
            Permission::AdminUserUpdate => "adminUser.update",
            Permission::AdminUserDelete => "adminUser.delete",
            Permission::AdminSessionRead => "adminSession.read",

            Permission::UserRead => "user.read",
            Permission::UserUpdate => "user.update",
            Permission::UserPreferenceRead => "userPreference.read",
            Permission::UserPreferenceUpdate => "userPreference.update",
            Permission::UserLicenseCreate => "userLicense.create",
            Permission::UserLicenseRead => "userLicense.read",
            Permission::UserLicenseUpdate => "userLicense.update",
            Permission::UserLicenseDelete => "userLicense.delete",
            Permission::UserOnboardingRead => "userOnboarding.read",
            Permission::UserOnboardingUpdate => "userOnboarding.update",
            Permission::UserOnboardingDelete => "userOnboarding.delete",
            Permission::UserProfileImageCreate => "userProfileImage.create",
            Permission::UserProfileImageRead => "userProfileImage.read",
            Permission::UserProfileImageUpdate => "userProfileImage.update",
            Permission::UserProfileImageDelete => "userProfileImage.delete",
            Permission::QueueRead => "queue.read",
            Permission::QueueUpdate => "queue.update",
            Permission::QueueJobCreate => "queueJob.create",
            Permission::QueueJobRead => "queueJob.read",
            Permission::QueueJobUpdate => "queueJob.update",
            Permission::QueueJobDelete => "queueJob.delete",
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
            "apiKey.rotate" => Some(Permission::ApiKeyRotate),

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
            "asset.derive" => Some(Permission::AssetDerive),
            "asset.statistics" => Some(Permission::AssetStatistics),

            "album.create" => Some(Permission::AlbumCreate),
            "album.read" => Some(Permission::AlbumRead),
            "album.update" => Some(Permission::AlbumUpdate),
            "album.delete" => Some(Permission::AlbumDelete),
            "album.statistics" => Some(Permission::AlbumStatistics),
            "albumAsset.create" | "album.addAsset" => Some(Permission::AlbumAddAsset),
            "albumAsset.delete" | "album.removeAsset" => Some(Permission::AlbumRemoveAsset),
            "albumUser.create" => Some(Permission::AlbumUserCreate),
            "albumUser.update" => Some(Permission::AlbumUserUpdate),
            "albumUser.delete" => Some(Permission::AlbumUserDelete),
            "album.share" => Some(Permission::AlbumShare),
            "album.download" => Some(Permission::AlbumDownload),

            "auth.changePassword" => Some(Permission::AuthChangePassword),

            "clusterGroup.read" => Some(Permission::ClusterGroupRead),
            "clusterGroup.leave" => Some(Permission::ClusterGroupLeave),
            "clusterGroupRequest.create" => Some(Permission::ClusterGroupRequestCreate),
            "clusterGroupRequest.read" => Some(Permission::ClusterGroupRequestRead),
            "clusterGroupRequest.delete" => Some(Permission::ClusterGroupRequestDelete),

            "authDevice.delete" => Some(Permission::AuthDeviceDelete),
            "backup.list" => Some(Permission::BackupList),
            "backup.download" => Some(Permission::BackupDownload),
            "backup.upload" => Some(Permission::BackupUpload),
            "backup.delete" => Some(Permission::BackupDelete),
            "job.create" => Some(Permission::JobCreate),
            "job.read" => Some(Permission::JobRead),
            "maintenance" => Some(Permission::Maintenance),

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
            "adminConfig.read" => Some(Permission::AdminConfigRead),
            "adminConfig.update" => Some(Permission::AdminConfigUpdate),
            "userConfig.read" => Some(Permission::UserConfigRead),

            "systemMetadata.read" => Some(Permission::SystemMetadataRead),
            "systemMetadata.update" => Some(Permission::SystemMetadataUpdate),

            "plugin.create" => Some(Permission::PluginCreate),
            "plugin.read" => Some(Permission::PluginRead),
            "plugin.update" => Some(Permission::PluginUpdate),
            "plugin.delete" => Some(Permission::PluginDelete),

            "workflow.create" => Some(Permission::WorkflowCreate),
            "workflow.read" => Some(Permission::WorkflowRead),
            "workflow.update" => Some(Permission::WorkflowUpdate),
            "workflow.delete" => Some(Permission::WorkflowDelete),
            "workflow.logs" => Some(Permission::WorkflowLogs),

            "serverLicense.read" => Some(Permission::ServerLicenseRead),
            "serverLicense.update" => Some(Permission::ServerLicenseUpdate),
            "serverLicense.delete" => Some(Permission::ServerLicenseDelete),
            "server.versionCheck" => Some(Permission::ServerVersionCheck),
            "server.about" => Some(Permission::ServerAbout),
            "server.apkLinks" => Some(Permission::ServerApkLinks),
            "server.storage" => Some(Permission::ServerStorage),
            "server.statistics" => Some(Permission::ServerStatistics),

            "adminAuth.unlinkAll" => Some(Permission::AdminAuthUnlinkAll),

            "tag.create" => Some(Permission::TagCreate),
            "tag.read" => Some(Permission::TagRead),
            "tag.update" => Some(Permission::TagUpdate),
            "tag.delete" => Some(Permission::TagDelete),
            "tag.asset" => Some(Permission::TagAsset),

            "adminUser.create" | "admin.user.create" => Some(Permission::AdminUserCreate),
            "adminUser.read" | "admin.user.read" => Some(Permission::AdminUserRead),
            "adminUser.update" | "admin.user.update" => Some(Permission::AdminUserUpdate),
            "adminUser.delete" | "admin.user.delete" => Some(Permission::AdminUserDelete),
            "adminSession.read" => Some(Permission::AdminSessionRead),

            "user.read" => Some(Permission::UserRead),
            "user.update" => Some(Permission::UserUpdate),
            "userPreference.read" => Some(Permission::UserPreferenceRead),
            "userPreference.update" => Some(Permission::UserPreferenceUpdate),
            "userLicense.create" => Some(Permission::UserLicenseCreate),
            "userLicense.read" => Some(Permission::UserLicenseRead),
            "userLicense.update" => Some(Permission::UserLicenseUpdate),
            "userLicense.delete" => Some(Permission::UserLicenseDelete),
            "userOnboarding.read" => Some(Permission::UserOnboardingRead),
            "userOnboarding.update" => Some(Permission::UserOnboardingUpdate),
            "userOnboarding.delete" => Some(Permission::UserOnboardingDelete),
            "userProfileImage.create" => Some(Permission::UserProfileImageCreate),
            "userProfileImage.read" => Some(Permission::UserProfileImageRead),
            "userProfileImage.update" => Some(Permission::UserProfileImageUpdate),
            "userProfileImage.delete" => Some(Permission::UserProfileImageDelete),
            "queue.read" => Some(Permission::QueueRead),
            "queue.update" => Some(Permission::QueueUpdate),
            "queueJob.create" => Some(Permission::QueueJobCreate),
            "queueJob.read" => Some(Permission::QueueJobRead),
            "queueJob.update" => Some(Permission::QueueJobUpdate),
            "queueJob.delete" => Some(Permission::QueueJobDelete),

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
