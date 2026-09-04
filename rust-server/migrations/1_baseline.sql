-- sqlx baseline: fused end-state of Immich Kysely migration history
-- listed in migrations/baseline_lock.json (all fused names → this single file).
-- Fresh empty databases apply this migration.
-- Existing Immich schemas are bridged: version 1 is recorded without re-executing.
-- Only after this baseline is locked in use: add migrations/2_*.sql when syncing new upstream.

-- ---------------------------------------------------------------------------
-- Extensions & functions
-- ---------------------------------------------------------------------------

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "unaccent";
CREATE EXTENSION IF NOT EXISTS "cube";
CREATE EXTENSION IF NOT EXISTS "earthdistance";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

CREATE OR REPLACE FUNCTION immich_uuid_v7(p_timestamp timestamptz DEFAULT clock_timestamp())
RETURNS uuid
VOLATILE LANGUAGE SQL
AS $$
    SELECT encode(
        set_bit(
            set_bit(
                overlay(uuid_send(gen_random_uuid())
                    PLACING substring(int8send(floor(extract(epoch FROM p_timestamp) * 1000)::bigint) FROM 3)
                    FROM 1 FOR 6),
                52, 1),
            53, 1),
        'hex')::uuid;
$$;

CREATE OR REPLACE FUNCTION updated_at()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    ts timestamptz := clock_timestamp();
BEGIN
    NEW."updatedAt" := ts;
    NEW."updateId" := immich_uuid_v7(ts);
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION f_unaccent(text)
RETURNS text
PARALLEL SAFE STRICT IMMUTABLE LANGUAGE SQL
RETURN unaccent('unaccent', $1);

CREATE OR REPLACE FUNCTION ll_to_earth_public(latitude double precision, longitude double precision)
RETURNS public.earth
PARALLEL SAFE STRICT IMMUTABLE LANGUAGE SQL
AS $$
    SELECT public.cube(
        public.cube(
            public.cube(public.earth() * cos(radians(latitude)) * cos(radians(longitude))),
            public.earth() * cos(radians(latitude)) * sin(radians(longitude))),
        public.earth() * sin(radians(latitude)))::public.earth
$$;

CREATE OR REPLACE FUNCTION tag_closure_after_insert()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO tag_closure (id_ancestor, id_descendant)
    VALUES (NEW.id, NEW.id)
    ON CONFLICT DO NOTHING;

    IF NEW."parentId" IS NOT NULL THEN
        INSERT INTO tag_closure (id_ancestor, id_descendant)
        SELECT id_ancestor, NEW.id
        FROM tag_closure
        WHERE id_descendant = NEW."parentId"
        ON CONFLICT DO NOTHING;
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION f_concat_ws(separator text, parts text[])
RETURNS text
PARALLEL SAFE IMMUTABLE LANGUAGE SQL
AS $$ SELECT array_to_string(parts, separator) $$;

-- Sync audit trigger functions (from Immich schema/functions.ts)
CREATE OR REPLACE FUNCTION user_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO user_audit ("userId") SELECT id FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION partner_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO partner_audit ("sharedById", "sharedWithId")
    SELECT "sharedById", "sharedWithId" FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION asset_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO asset_audit ("assetId", "ownerId")
    SELECT id, "ownerId" FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION album_asset_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO album_asset_audit ("albumId", "assetId")
    SELECT "albumId", "assetId" FROM OLD
    WHERE "albumId" IN (SELECT id FROM album WHERE id IN (SELECT "albumId" FROM OLD));
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION album_user_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO album_audit ("albumId", "userId")
    SELECT "albumId", "userId" FROM OLD;

    IF pg_trigger_depth() = 1 THEN
        INSERT INTO album_user_audit ("albumId", "userId")
        SELECT "albumId", "userId" FROM OLD;
    END IF;

    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION album_user_after_insert()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE album SET "updatedAt" = clock_timestamp(), "updateId" = immich_uuid_v7(clock_timestamp())
    WHERE id IN (SELECT "albumId" FROM inserted_rows)
      AND NOT EXISTS (SELECT FROM inserted_rows WHERE role = 'owner');
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION memory_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO memory_audit ("memoryId", "userId")
    SELECT id, "ownerId" FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION memory_asset_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO memory_asset_audit ("memoryId", "assetId")
    SELECT "memoriesId", "assetId" FROM OLD
    WHERE "memoriesId" IN (SELECT id FROM memory WHERE id IN (SELECT "memoriesId" FROM OLD));
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION stack_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO stack_audit ("stackId", "userId")
    SELECT id, "ownerId" FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION person_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO person_audit ("personGroupId", "ownerId")
    SELECT "personGroupId", "ownerId" FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION person_group_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO person_group_audit ("personGroupId", "clusterGroupId")
    SELECT id, "clusterGroupId" FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION album_user_delete()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM album
    WHERE album.id = OLD."albumId"
      AND NOT EXISTS (
          SELECT "albumId"
          FROM album_user
          WHERE album_user."albumId" = album.id
            AND album_user.role = 'owner'
      );
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION user_metadata_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO user_metadata_audit ("userId", key)
    SELECT "userId", key FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION asset_metadata_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO asset_metadata_audit ("assetId", key)
    SELECT "assetId", key FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION asset_face_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO asset_face_audit ("assetFaceId", "assetId")
    SELECT id, "assetId" FROM old;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION asset_edit_insert()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE asset SET "isEdited" = true
    FROM inserted_edit
    WHERE asset.id = inserted_edit."assetId" AND NOT asset."isEdited";
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION asset_edit_delete()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE asset SET "isEdited" = false
    FROM deleted_edit
    WHERE asset.id = deleted_edit."assetId" AND asset."isEdited"
      AND NOT EXISTS (SELECT FROM asset_edit edit WHERE edit."assetId" = asset.id);
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION asset_edit_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO asset_edit_audit ("editId", "assetId")
    SELECT id, "assetId" FROM OLD;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION asset_ocr_delete_audit()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO asset_ocr_audit ("assetId")
    SELECT "assetId" FROM OLD;
    RETURN NULL;
END;
$$;

-- ---------------------------------------------------------------------------
-- Types & tables
-- ---------------------------------------------------------------------------

CREATE TYPE assets_status_enum AS ENUM ('active', 'trashed', 'deleted');
CREATE TYPE sourcetype AS ENUM ('machine-learning', 'exif', 'manual');
CREATE TYPE asset_visibility_enum AS ENUM ('archive', 'timeline', 'hidden', 'locked');
CREATE TYPE asset_checksum_algorithm_enum AS ENUM ('sha1', 'sha1-path');
CREATE TYPE album_user_role_enum AS ENUM ('owner', 'editor', 'viewer');
CREATE TYPE video_stream_variant_codec_enum AS ENUM ('h264', 'hevc', 'av1');

CREATE TABLE cluster_group (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name varchar,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7()
);
CREATE INDEX "cluster_group_updateId_idx" ON cluster_group ("updateId");
CREATE TRIGGER "cluster_group_updatedAt"
    BEFORE UPDATE ON cluster_group
    FOR EACH ROW
    EXECUTE FUNCTION updated_at();

CREATE TABLE "user" (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    email varchar NOT NULL UNIQUE,
    password varchar DEFAULT NULL,
    "pinCode" varchar,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "profileImagePath" varchar NOT NULL DEFAULT '',
    "isAdmin" boolean NOT NULL DEFAULT false,
    "shouldChangePassword" boolean NOT NULL DEFAULT true,
    "avatarColor" varchar,
    "deletedAt" timestamptz,
    "oauthId" varchar NOT NULL DEFAULT '',
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "storageLabel" varchar UNIQUE,
    name varchar NOT NULL DEFAULT '',
    "quotaSizeInBytes" bigint,
    "quotaUsageInBytes" bigint NOT NULL DEFAULT 0,
    status varchar NOT NULL DEFAULT 'active',
    "profileChangedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "clusterGroupId" uuid NOT NULL REFERENCES cluster_group(id) ON UPDATE CASCADE ON DELETE NO ACTION
);

CREATE INDEX user_updated_at_id_idx ON "user" ("updatedAt", id);
CREATE INDEX "user_clusterGroupId_idx" ON "user" ("clusterGroupId");

CREATE TABLE cluster_group_request (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "clusterGroupId" uuid NOT NULL REFERENCES cluster_group(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT "cluster_group_request_clusterGroupId_userId_uq" UNIQUE ("clusterGroupId", "userId")
);
CREATE INDEX "cluster_group_request_clusterGroupId_idx" ON cluster_group_request ("clusterGroupId");
CREATE INDEX "cluster_group_request_userId_idx" ON cluster_group_request ("userId");

CREATE TABLE session (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    token bytea NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "expiresAt" timestamptz,
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "parentId" uuid REFERENCES session(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "deviceType" varchar NOT NULL DEFAULT '',
    "deviceOS" varchar NOT NULL DEFAULT '',
    "appVersion" varchar,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "isPendingSyncReset" boolean NOT NULL DEFAULT false,
    "pinExpiresAt" timestamptz,
    "oauthSid" varchar,
    "oauthBearerToken" varchar
);

CREATE INDEX session_token_idx ON session (token);
CREATE INDEX session_oauth_sid_idx ON session ("oauthSid");

CREATE TABLE api_key (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name varchar NOT NULL,
    key bytea NOT NULL,
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    permissions varchar[] NOT NULL,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7()
);

CREATE INDEX api_key_key_idx ON api_key (key);

CREATE TABLE user_metadata (
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    key varchar NOT NULL,
    value jsonb NOT NULL,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY ("userId", key)
);

CREATE INDEX idx_user_metadata_update_id ON user_metadata ("updateId");
CREATE INDEX idx_user_metadata_updated_at ON user_metadata ("updatedAt");

CREATE TABLE library (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name varchar NOT NULL,
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "importPaths" text[] NOT NULL,
    "exclusionPatterns" text[] NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "deletedAt" timestamptz,
    "refreshedAt" timestamptz,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7()
);

CREATE TABLE stack (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "primaryAssetId" uuid NOT NULL UNIQUE,
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE
);

CREATE TABLE asset (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    type varchar NOT NULL,
    "originalPath" varchar NOT NULL,
    "fileCreatedAt" timestamptz NOT NULL,
    "fileModifiedAt" timestamptz NOT NULL,
    "isFavorite" boolean NOT NULL DEFAULT false,
    duration integer,
    checksum bytea NOT NULL,
    "checksumAlgorithm" asset_checksum_algorithm_enum NOT NULL DEFAULT 'sha1',
    "livePhotoVideoId" uuid REFERENCES asset(id) ON UPDATE CASCADE ON DELETE SET NULL,
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "originalFileName" varchar NOT NULL,
    thumbhash bytea,
    "isOffline" boolean NOT NULL DEFAULT false,
    "libraryId" uuid REFERENCES library(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "isExternal" boolean NOT NULL DEFAULT false,
    "deletedAt" timestamptz,
    "localDateTime" timestamptz NOT NULL,
    "stackId" uuid REFERENCES stack(id) ON UPDATE CASCADE ON DELETE SET NULL,
    "duplicateId" uuid,
    status assets_status_enum NOT NULL DEFAULT 'active',
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    visibility asset_visibility_enum NOT NULL DEFAULT 'timeline',
    width integer,
    height integer,
    "isEdited" boolean NOT NULL DEFAULT false
);

ALTER TABLE stack
    ADD CONSTRAINT stack_primary_asset_id_fkey
    FOREIGN KEY ("primaryAssetId") REFERENCES asset(id);

CREATE UNIQUE INDEX asset_owner_checksum_idx ON asset ("ownerId", checksum) WHERE "libraryId" IS NULL;
CREATE UNIQUE INDEX asset_owner_library_checksum_idx ON asset ("ownerId", "libraryId", checksum) WHERE "libraryId" IS NOT NULL;
CREATE INDEX asset_file_created_at_idx ON asset ("fileCreatedAt");
CREATE INDEX asset_created_at_idx ON asset ("createdAt");
CREATE INDEX asset_checksum_idx ON asset (checksum);
CREATE INDEX asset_original_file_name_idx ON asset ("originalFileName");
CREATE INDEX asset_original_path_library_id_idx ON asset ("originalPath", "libraryId");
CREATE INDEX asset_id_stack_id_idx ON asset (id, "stackId");
CREATE INDEX asset_id_timeline_not_deleted_idx ON asset (id) WHERE visibility = 'timeline' AND "deletedAt" IS NULL;
CREATE INDEX asset_original_filename_trigram_idx ON asset USING gin (f_unaccent("originalFileName") gin_trgm_ops);
CREATE INDEX asset_local_date_time_idx ON asset ((("localDateTime" AT TIME ZONE 'UTC')::date));
CREATE INDEX asset_local_date_time_month_idx ON asset ((
    date_trunc('MONTH'::text, ("localDateTime" AT TIME ZONE 'UTC'::text)) AT TIME ZONE 'UTC'::text
));

CREATE TABLE asset_exif (
    "assetId" uuid PRIMARY KEY REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    make varchar,
    model varchar,
    "exifImageWidth" integer,
    "exifImageHeight" integer,
    "fileSizeInByte" bigint,
    orientation varchar,
    "dateTimeOriginal" timestamptz,
    "modifyDate" timestamptz,
    "lensModel" varchar,
    "fNumber" double precision,
    "focalLength" double precision,
    iso integer,
    latitude double precision,
    longitude double precision,
    city varchar,
    state varchar,
    country varchar,
    description text NOT NULL DEFAULT '',
    fps double precision,
    "exposureTime" varchar,
    "livePhotoCID" varchar,
    "timeZone" varchar,
    "projectionType" varchar,
    "profileDescription" varchar,
    colorspace varchar,
    "bitsPerSample" integer,
    "autoStackId" varchar,
    rating integer,
    tags varchar[],
    "updatedAt" timestamptz NOT NULL DEFAULT clock_timestamp(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "lockedProperties" varchar[]
);

CREATE INDEX asset_exif_city_idx ON asset_exif (city);
CREATE INDEX asset_exif_live_photo_cid_idx ON asset_exif ("livePhotoCID");
CREATE INDEX asset_exif_auto_stack_id_idx ON asset_exif ("autoStackId");
CREATE INDEX idx_asset_exif_gist_earthcoord ON asset_exif USING gist (ll_to_earth_public(latitude, longitude));

CREATE TABLE asset_file (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    type varchar NOT NULL,
    path varchar NOT NULL,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "isEdited" boolean NOT NULL DEFAULT false,
    "isProgressive" boolean NOT NULL DEFAULT false,
    "isTransparent" boolean NOT NULL DEFAULT false,
    UNIQUE ("assetId", type, "isEdited")
);

CREATE TABLE asset_metadata (
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    key varchar NOT NULL,
    value jsonb NOT NULL,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY ("assetId", key)
);

CREATE INDEX asset_metadata_update_id_idx ON asset_metadata ("updateId");
CREATE INDEX asset_metadata_updated_at_idx ON asset_metadata ("updatedAt");

CREATE TABLE asset_edit (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    action varchar NOT NULL,
    parameters jsonb NOT NULL,
    sequence integer NOT NULL,
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    UNIQUE ("assetId", sequence)
);

CREATE INDEX asset_edit_asset_id_idx ON asset_edit ("assetId");
CREATE INDEX asset_edit_update_id_idx ON asset_edit ("updateId");

CREATE TABLE asset_ocr (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    x1 real NOT NULL,
    y1 real NOT NULL,
    x2 real NOT NULL,
    y2 real NOT NULL,
    x3 real NOT NULL,
    y3 real NOT NULL,
    x4 real NOT NULL,
    y4 real NOT NULL,
    "boxScore" real NOT NULL,
    "textScore" real NOT NULL,
    text text NOT NULL,
    "isVisible" boolean NOT NULL DEFAULT true,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "updatedAt" timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX asset_ocr_update_id_idx ON asset_ocr ("updateId");
CREATE INDEX asset_ocr_asset_id_idx ON asset_ocr ("assetId");
CREATE TRIGGER "asset_ocr_updatedAt"
    BEFORE UPDATE ON asset_ocr
    FOR EACH ROW
    EXECUTE FUNCTION updated_at();

CREATE TABLE asset_job_status (
    "assetId" uuid PRIMARY KEY REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "facesRecognizedAt" timestamptz,
    "metadataExtractedAt" timestamptz,
    "duplicatesDetectedAt" timestamptz,
    "ocrAt" timestamptz
);

CREATE TABLE asset_audio (
    "assetId" uuid PRIMARY KEY REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    bitrate integer NOT NULL,
    index smallint NOT NULL,
    profile smallint,
    "codecName" text NOT NULL
);

CREATE TABLE asset_video (
    "assetId" uuid PRIMARY KEY REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    bitrate integer NOT NULL,
    "frameCount" integer NOT NULL,
    "timeBase" integer NOT NULL,
    index smallint NOT NULL,
    profile smallint,
    level smallint,
    "colorPrimaries" smallint NOT NULL,
    "colorTransfer" smallint NOT NULL,
    "colorMatrix" smallint NOT NULL,
    "dvProfile" smallint,
    "dvLevel" smallint,
    "dvBlSignalCompatibilityId" smallint,
    "codecName" text NOT NULL,
    "formatName" text NOT NULL,
    "formatLongName" text NOT NULL,
    "pixelFormat" text NOT NULL
);

CREATE TABLE asset_keyframe (
    "assetId" uuid PRIMARY KEY REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    pts integer[] NOT NULL,
    "accDuration" integer[] NOT NULL,
    "ownDuration" integer[] NOT NULL,
    "totalDuration" integer NOT NULL,
    "packetCount" integer NOT NULL,
    "outputFrames" integer NOT NULL
);

CREATE TABLE album (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "albumName" varchar NOT NULL DEFAULT 'Untitled Album',
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "albumThumbnailAssetId" uuid REFERENCES asset(id) ON UPDATE CASCADE ON DELETE SET NULL,
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    description text DEFAULT NULL,
    "deletedAt" timestamptz,
    "isActivityEnabled" boolean NOT NULL DEFAULT true,
    "order" varchar NOT NULL DEFAULT 'desc',
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7()
);

COMMENT ON COLUMN album."albumThumbnailAssetId" IS 'Asset ID to be used as thumbnail';

CREATE TABLE album_user (
    "albumId" uuid NOT NULL REFERENCES album(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    role album_user_role_enum NOT NULL DEFAULT 'editor',
    "createId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY ("albumId", "userId")
);

CREATE UNIQUE INDEX album_user_unique_owner ON album_user ("albumId") WHERE role = 'owner';

CREATE TRIGGER album_user_delete
    AFTER DELETE ON album_user
    REFERENCING OLD TABLE AS old
    FOR EACH ROW
    EXECUTE FUNCTION album_user_delete();

CREATE TABLE album_asset (
    "albumId" uuid NOT NULL REFERENCES album(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    PRIMARY KEY ("albumId", "assetId")
);

CREATE TABLE activity (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "albumId" uuid NOT NULL REFERENCES album(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "assetId" uuid REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    comment text,
    "isLiked" boolean NOT NULL DEFAULT false,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    CONSTRAINT activity_like_check CHECK (
        (comment IS NULL AND "isLiked" = true) OR (comment IS NOT NULL AND "isLiked" = false)
    ),
    CONSTRAINT activity_album_asset_fkey
        FOREIGN KEY ("albumId", "assetId")
        REFERENCES album_asset ("albumId", "assetId")
        ON DELETE CASCADE
);

CREATE UNIQUE INDEX activity_like_idx ON activity ("assetId", "userId", "albumId") WHERE "isLiked" = true;
CREATE INDEX album_user_create_id_idx ON album_user ("createId");
CREATE INDEX album_user_update_id_idx ON album_user ("updateId");

CREATE TABLE tag (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    value varchar NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    color varchar,
    "parentId" uuid REFERENCES tag(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    UNIQUE ("userId", value)
);

CREATE TABLE tag_asset (
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "tagId" uuid NOT NULL REFERENCES tag(id) ON UPDATE CASCADE ON DELETE CASCADE,
    PRIMARY KEY ("assetId", "tagId")
);

CREATE INDEX tag_asset_asset_id_tag_id_idx ON tag_asset ("assetId", "tagId");
CREATE INDEX tag_asset_asset_id_idx ON tag_asset ("assetId");
CREATE INDEX tag_asset_tag_id_idx ON tag_asset ("tagId");

CREATE TABLE tag_closure (
    id_ancestor uuid NOT NULL REFERENCES tag(id) ON UPDATE NO ACTION ON DELETE CASCADE,
    id_descendant uuid NOT NULL REFERENCES tag(id) ON UPDATE NO ACTION ON DELETE CASCADE,
    PRIMARY KEY (id_ancestor, id_descendant)
);

CREATE INDEX tag_closure_id_ancestor_idx ON tag_closure (id_ancestor);
CREATE INDEX tag_closure_id_descendant_idx ON tag_closure (id_descendant);

CREATE TABLE shared_link (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    description varchar,
    "userId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    key bytea NOT NULL UNIQUE,
    type varchar NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "expiresAt" timestamptz,
    "allowUpload" boolean NOT NULL DEFAULT false,
    "albumId" uuid REFERENCES album(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "allowDownload" boolean NOT NULL DEFAULT true,
    "showExif" boolean NOT NULL DEFAULT true,
    password varchar,
    slug varchar UNIQUE
);

CREATE INDEX shared_link_key_idx ON shared_link (key);

CREATE TABLE shared_link_asset (
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "sharedLinkId" uuid NOT NULL REFERENCES shared_link(id) ON UPDATE CASCADE ON DELETE CASCADE,
    PRIMARY KEY ("assetId", "sharedLinkId")
);

CREATE TABLE partner (
    "sharedById" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "sharedWithId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "createId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "inTimeline" boolean NOT NULL DEFAULT false,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    PRIMARY KEY ("sharedById", "sharedWithId")
);

CREATE INDEX partner_create_id_idx ON partner ("createId");
CREATE INDEX partner_update_id_idx ON partner ("updateId");

CREATE TABLE asset_face (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "personGroupId" uuid,
    "imageWidth" integer NOT NULL DEFAULT 0,
    "imageHeight" integer NOT NULL DEFAULT 0,
    "boundingBoxX1" integer NOT NULL DEFAULT 0,
    "boundingBoxY1" integer NOT NULL DEFAULT 0,
    "boundingBoxX2" integer NOT NULL DEFAULT 0,
    "boundingBoxY2" integer NOT NULL DEFAULT 0,
    "sourceType" sourcetype NOT NULL DEFAULT 'machine-learning',
    "deletedAt" timestamptz,
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "isVisible" boolean NOT NULL DEFAULT true
);

CREATE TABLE person_group (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "clusterGroupId" uuid NOT NULL REFERENCES cluster_group(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "createId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7()
);
CREATE INDEX "person_group_clusterGroupId_idx" ON person_group ("clusterGroupId");
CREATE INDEX "person_group_createId_idx" ON person_group ("createId");
CREATE INDEX "person_group_updateId_idx" ON person_group ("updateId");
CREATE TRIGGER person_group_delete_audit
    AFTER DELETE ON person_group
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION person_group_delete_audit();
CREATE TRIGGER "person_group_updatedAt"
    BEFORE UPDATE ON person_group
    FOR EACH ROW
    EXECUTE FUNCTION updated_at();

CREATE TABLE person (
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "personGroupId" uuid NOT NULL REFERENCES person_group(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    name varchar NOT NULL DEFAULT '',
    "thumbnailPath" varchar NOT NULL DEFAULT '',
    "isHidden" boolean NOT NULL DEFAULT false,
    "birthDate" date,
    "faceAssetId" uuid REFERENCES asset_face(id) ON DELETE SET NULL,
    "isFavorite" boolean NOT NULL DEFAULT false,
    color varchar,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    PRIMARY KEY ("ownerId", "personGroupId"),
    CONSTRAINT person_birth_date_chk CHECK ("birthDate" <= CURRENT_DATE)
);

ALTER TABLE asset_face
    ADD CONSTRAINT "asset_face_personGroupId_fkey"
    FOREIGN KEY ("personGroupId") REFERENCES person_group(id) ON UPDATE CASCADE ON DELETE SET NULL;

CREATE INDEX "asset_face_personGroupId_assetId_idx" ON asset_face ("personGroupId", "assetId");
CREATE INDEX "asset_face_personGroupId_assetId_notDeleted_isVisible_idx"
    ON asset_face ("personGroupId", "assetId")
    WHERE "deletedAt" IS NULL AND "isVisible" IS TRUE;
CREATE INDEX "asset_face_assetId_personGroupId_idx" ON asset_face ("assetId", "personGroupId");
CREATE INDEX "person_personGroupId_idx" ON person ("personGroupId");
CREATE INDEX idx_person_name_trigram ON person USING gin (f_unaccent(name) gin_trgm_ops);

CREATE TABLE system_metadata (
    key varchar PRIMARY KEY,
    value jsonb NOT NULL
);

CREATE TABLE memory (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "deletedAt" timestamptz,
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    type varchar NOT NULL,
    data jsonb NOT NULL,
    "isSaved" boolean NOT NULL DEFAULT false,
    "memoryAt" timestamptz NOT NULL,
    "seenAt" timestamptz,
    "showAt" timestamptz,
    "hideAt" timestamptz,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7()
);

CREATE INDEX memory_owner_id_idx ON memory ("ownerId");
CREATE INDEX memory_update_id_idx ON memory ("updateId");

CREATE TABLE memory_asset (
    "memoriesId" uuid NOT NULL REFERENCES memory(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    PRIMARY KEY ("memoriesId", "assetId")
);

CREATE INDEX memory_asset_memories_id_idx ON memory_asset ("memoriesId");
CREATE INDEX memory_asset_asset_id_idx ON memory_asset ("assetId");
CREATE INDEX memory_asset_update_id_idx ON memory_asset ("updateId");

CREATE TABLE notification (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "deletedAt" timestamptz,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "userId" uuid REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    level varchar NOT NULL DEFAULT 'info',
    type varchar NOT NULL DEFAULT 'Custom',
    data jsonb,
    title varchar NOT NULL,
    description text,
    "readAt" timestamptz
);

CREATE INDEX notification_user_id_idx ON notification ("userId");
CREATE INDEX notification_update_id_idx ON notification ("updateId");

CREATE TABLE version_history (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    version varchar NOT NULL
);

CREATE TABLE move_history (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "entityId" uuid NOT NULL,
    "pathType" varchar NOT NULL,
    "oldPath" varchar NOT NULL,
    "newPath" varchar NOT NULL,
    CONSTRAINT "UQ_entityId_pathType" UNIQUE ("entityId", "pathType"),
    CONSTRAINT "UQ_newPath" UNIQUE ("newPath")
);

CREATE TABLE integrity_report (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    type varchar NOT NULL,
    path varchar NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "assetId" uuid REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "fileAssetId" uuid REFERENCES asset_file(id) ON UPDATE CASCADE ON DELETE CASCADE,
    UNIQUE (type, path)
);

CREATE TABLE naturalearth_countries (
    id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    admin varchar(50) NOT NULL,
    admin_a3 varchar(3) NOT NULL,
    type varchar(50) NOT NULL,
    coordinates polygon NOT NULL
);

CREATE TABLE video_stream_session (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "expiresAt" timestamptz NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX video_stream_session_expires_at_idx ON video_stream_session ("expiresAt");

CREATE TABLE video_stream_variant (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "sessionId" uuid NOT NULL REFERENCES video_stream_session(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    bitrate integer NOT NULL,
    codec video_stream_variant_codec_enum NOT NULL,
    resolution smallint NOT NULL,
    UNIQUE ("sessionId", bitrate, resolution, codec)
);

CREATE TABLE video_stream_segment (
    "variantId" uuid NOT NULL REFERENCES video_stream_variant(id) ON UPDATE CASCADE ON DELETE CASCADE,
    index integer NOT NULL,
    "durationUs" integer NOT NULL,
    PRIMARY KEY ("variantId", index)
);

CREATE TABLE plugin (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    enabled boolean NOT NULL DEFAULT true,
    name varchar NOT NULL UNIQUE,
    version varchar NOT NULL,
    title varchar NOT NULL,
    description varchar NOT NULL,
    author varchar NOT NULL,
    "wasmBytes" bytea NOT NULL,
    templates jsonb NOT NULL,
    "sha256hash" bytea NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    UNIQUE (name, version)
);

CREATE TABLE plugin_method (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "pluginId" uuid NOT NULL REFERENCES plugin(id) ON UPDATE CASCADE ON DELETE CASCADE,
    name varchar NOT NULL,
    title varchar NOT NULL,
    description varchar NOT NULL,
    types varchar[] NOT NULL,
    "hostFunctions" boolean NOT NULL DEFAULT false,
    schema jsonb,
    "uiHints" varchar[] NOT NULL DEFAULT '{}',
    "allowedHosts" varchar[] NOT NULL DEFAULT '{}',
    UNIQUE ("pluginId", name)
);

CREATE TABLE workflow (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    trigger varchar NOT NULL,
    name varchar,
    description varchar,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    enabled boolean NOT NULL DEFAULT true,
    logging boolean NOT NULL DEFAULT false
);

CREATE TABLE workflow_step (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    enabled boolean NOT NULL DEFAULT true,
    "workflowId" uuid NOT NULL REFERENCES workflow(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "pluginMethodId" uuid NOT NULL REFERENCES plugin_method(id) ON UPDATE CASCADE ON DELETE CASCADE,
    config jsonb,
    "order" integer NOT NULL
);

CREATE TABLE workflow_log (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "workflowId" uuid NOT NULL REFERENCES workflow(id) ON UPDATE CASCADE ON DELETE CASCADE,
    result varchar NOT NULL,
    "workflowStepId" uuid REFERENCES workflow_step(id) ON UPDATE CASCADE ON DELETE SET NULL,
    "triggerDataId" uuid,
    "runId" uuid NOT NULL
);
CREATE INDEX "workflow_log_workflowId_idx" ON workflow_log ("workflowId");
CREATE INDEX "workflow_log_workflowStepId_idx" ON workflow_log ("workflowStepId");

-- smart_search / face_search：无 pgvector 时自动跳过
DO $$
BEGIN
    BEGIN
        CREATE EXTENSION IF NOT EXISTS "vchord" CASCADE;
    EXCEPTION
        WHEN OTHERS THEN
            NULL;
    END;

    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'vector') THEN
        BEGIN
            CREATE EXTENSION IF NOT EXISTS "vector" CASCADE;
        EXCEPTION
            WHEN OTHERS THEN
                RAISE NOTICE 'smart_search skipped: vector extension not installed';
                RETURN;
        END;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'vector') THEN
        RAISE NOTICE 'smart_search skipped: vector type not available';
        RETURN;
    END IF;

    CREATE TABLE smart_search (
        "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
        embedding vector(512) NOT NULL,
        PRIMARY KEY ("assetId")
    );
    ALTER TABLE smart_search ALTER COLUMN embedding SET STORAGE EXTERNAL;

    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vchord') THEN
        EXECUTE $idx$
            CREATE INDEX clip_index ON smart_search
            USING vchordrq (embedding vector_cosine_ops) WITH (options = $opts$
            residual_quantization = false
            [build.internal]
            lists = [1]
            spherical_centroids = true
            build_threads = 4
            sampling_factor = 1024
            $opts$)
        $idx$;
        EXECUTE format('ALTER DATABASE %I SET vchordrq.probes = 1', current_database());
    ELSE
        EXECUTE $idx$
            CREATE INDEX clip_index ON smart_search
            USING hnsw (embedding vector_cosine_ops)
            WITH (ef_construction = 300, m = 16)
        $idx$;
    END IF;

    CREATE TABLE face_search (
        "faceId" uuid NOT NULL REFERENCES asset_face(id) ON UPDATE CASCADE ON DELETE CASCADE,
        embedding vector(512) NOT NULL,
        PRIMARY KEY ("faceId")
    );
    ALTER TABLE face_search ALTER COLUMN embedding SET STORAGE EXTERNAL;

    EXECUTE $idx$
        CREATE INDEX face_index ON face_search
        USING hnsw (embedding vector_cosine_ops)
        WITH (ef_construction = 300, m = 16)
    $idx$;
END $$;

CREATE TABLE ocr_search (
    "assetId" uuid PRIMARY KEY REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    text text NOT NULL
);

CREATE INDEX idx_ocr_search_text ON ocr_search USING gin (f_unaccent(text) gin_trgm_ops);

CREATE TABLE geodata_places (
    id integer PRIMARY KEY,
    name varchar(200) NOT NULL,
    longitude double precision NOT NULL,
    latitude double precision NOT NULL,
    "countryCode" char(2) NOT NULL,
    "admin1Code" varchar(20),
    "admin2Code" varchar(80),
    "modificationDate" date NOT NULL,
    "admin1Name" varchar,
    "admin2Name" varchar,
    "alternateNames" varchar
);

CREATE INDEX idx_geodata_places_name ON geodata_places USING gin (f_unaccent(name) gin_trgm_ops);
CREATE INDEX idx_geodata_places_admin1_name ON geodata_places USING gin (f_unaccent("admin1Name") gin_trgm_ops);
CREATE INDEX idx_geodata_places_admin2_name ON geodata_places USING gin (f_unaccent("admin2Name") gin_trgm_ops);
CREATE INDEX idx_geodata_places_alternate_names ON geodata_places USING gin (f_unaccent("alternateNames") gin_trgm_ops);
CREATE INDEX idx_geodata_gist_earthcoord ON geodata_places (ll_to_earth_public(latitude, longitude));

-- ---------------------------------------------------------------------------
-- Sync audit tables
-- ---------------------------------------------------------------------------

CREATE TABLE user_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "userId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX user_audit_deleted_at_idx ON user_audit ("deletedAt");

CREATE TABLE user_metadata_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "userId" uuid NOT NULL,
    key varchar NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX user_metadata_audit_user_id_idx ON user_metadata_audit ("userId");
CREATE INDEX user_metadata_audit_key_idx ON user_metadata_audit (key);
CREATE INDEX user_metadata_audit_deleted_at_idx ON user_metadata_audit ("deletedAt");

CREATE TABLE asset_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "assetId" uuid NOT NULL,
    "ownerId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX asset_audit_asset_id_idx ON asset_audit ("assetId");
CREATE INDEX asset_audit_owner_id_idx ON asset_audit ("ownerId");
CREATE INDEX asset_audit_deleted_at_idx ON asset_audit ("deletedAt");

CREATE TABLE album_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "albumId" uuid NOT NULL,
    "userId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX album_audit_album_id_idx ON album_audit ("albumId");
CREATE INDEX album_audit_user_id_idx ON album_audit ("userId");
CREATE INDEX album_audit_deleted_at_idx ON album_audit ("deletedAt");

CREATE TABLE album_user_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "albumId" uuid NOT NULL,
    "userId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX album_user_audit_album_id_idx ON album_user_audit ("albumId");
CREATE INDEX album_user_audit_user_id_idx ON album_user_audit ("userId");
CREATE INDEX album_user_audit_deleted_at_idx ON album_user_audit ("deletedAt");

CREATE TABLE album_asset_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "albumId" uuid NOT NULL REFERENCES album(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "assetId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX album_asset_audit_asset_id_idx ON album_asset_audit ("assetId");
CREATE INDEX album_asset_audit_deleted_at_idx ON album_asset_audit ("deletedAt");

CREATE TABLE partner_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "sharedById" uuid NOT NULL,
    "sharedWithId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX partner_audit_shared_by_id_idx ON partner_audit ("sharedById");
CREATE INDEX partner_audit_shared_with_id_idx ON partner_audit ("sharedWithId");
CREATE INDEX partner_audit_deleted_at_idx ON partner_audit ("deletedAt");

CREATE TABLE stack_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "stackId" uuid NOT NULL,
    "userId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX stack_audit_deleted_at_idx ON stack_audit ("deletedAt");

CREATE TABLE person_group_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "personGroupId" uuid NOT NULL,
    "clusterGroupId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX "person_group_audit_personGroupId_idx" ON person_group_audit ("personGroupId");
CREATE INDEX "person_group_audit_clusterGroupId_idx" ON person_group_audit ("clusterGroupId");
CREATE INDEX "person_group_audit_deletedAt_idx" ON person_group_audit ("deletedAt");

CREATE TABLE person_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "personGroupId" uuid NOT NULL,
    "ownerId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX "person_audit_personGroupId_idx" ON person_audit ("personGroupId");
CREATE INDEX person_audit_owner_id_idx ON person_audit ("ownerId");
CREATE INDEX person_audit_deleted_at_idx ON person_audit ("deletedAt");

CREATE TABLE memory_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "memoryId" uuid NOT NULL,
    "userId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX memory_audit_memory_id_idx ON memory_audit ("memoryId");
CREATE INDEX memory_audit_user_id_idx ON memory_audit ("userId");
CREATE INDEX memory_audit_deleted_at_idx ON memory_audit ("deletedAt");

CREATE TABLE memory_asset_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "memoryId" uuid NOT NULL REFERENCES memory(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "assetId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX memory_asset_audit_asset_id_idx ON memory_asset_audit ("assetId");
CREATE INDEX memory_asset_audit_deleted_at_idx ON memory_asset_audit ("deletedAt");

CREATE TABLE asset_face_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "assetFaceId" uuid NOT NULL,
    "assetId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX asset_face_audit_asset_face_id_idx ON asset_face_audit ("assetFaceId");
CREATE INDEX asset_face_audit_asset_id_idx ON asset_face_audit ("assetId");
CREATE INDEX asset_face_audit_deleted_at_idx ON asset_face_audit ("deletedAt");

CREATE TABLE asset_metadata_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "assetId" uuid NOT NULL,
    key varchar NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX asset_metadata_audit_asset_id_idx ON asset_metadata_audit ("assetId");
CREATE INDEX asset_metadata_audit_key_idx ON asset_metadata_audit (key);
CREATE INDEX asset_metadata_audit_deleted_at_idx ON asset_metadata_audit ("deletedAt");

CREATE TABLE asset_edit_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "editId" uuid NOT NULL,
    "assetId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX asset_edit_audit_asset_id_idx ON asset_edit_audit ("assetId");
CREATE INDEX asset_edit_audit_deleted_at_idx ON asset_edit_audit ("deletedAt");

CREATE TABLE asset_ocr_audit (
    id uuid PRIMARY KEY DEFAULT immich_uuid_v7(),
    "assetId" uuid NOT NULL,
    "deletedAt" timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX asset_ocr_audit_asset_id_idx ON asset_ocr_audit ("assetId");
CREATE INDEX asset_ocr_audit_deleted_at_idx ON asset_ocr_audit ("deletedAt");

CREATE TABLE session_sync_checkpoint (
    "sessionId" uuid NOT NULL REFERENCES session(id) ON UPDATE CASCADE ON DELETE CASCADE,
    type character varying NOT NULL,
    ack character varying NOT NULL,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    PRIMARY KEY ("sessionId", type)
);
CREATE INDEX session_sync_checkpoint_update_id_idx ON session_sync_checkpoint ("updateId");
CREATE TRIGGER session_sync_checkpoint_updated_at BEFORE UPDATE ON session_sync_checkpoint FOR EACH ROW EXECUTE FUNCTION updated_at();

-- ---------------------------------------------------------------------------
-- Triggers
-- ---------------------------------------------------------------------------

-- updatedAt triggers
CREATE TRIGGER user_updated_at BEFORE UPDATE ON "user" FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER session_updated_at BEFORE UPDATE ON session FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER api_key_updated_at BEFORE UPDATE ON api_key FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER user_metadata_updated_at BEFORE UPDATE ON user_metadata FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER library_updated_at BEFORE UPDATE ON library FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER stack_updated_at BEFORE UPDATE ON stack FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_updated_at BEFORE UPDATE ON asset FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_exif_updated_at BEFORE UPDATE ON asset_exif FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_file_updated_at BEFORE UPDATE ON asset_file FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_metadata_updated_at BEFORE UPDATE ON asset_metadata FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_edit_updated_at BEFORE UPDATE ON asset_edit FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER album_updated_at BEFORE UPDATE ON album FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER album_user_updated_at BEFORE UPDATE ON album_user FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER album_asset_updated_at BEFORE UPDATE ON album_asset FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER activity_updated_at BEFORE UPDATE ON activity FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER tag_updated_at BEFORE UPDATE ON tag FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER partner_updated_at BEFORE UPDATE ON partner FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_face_updated_at BEFORE UPDATE ON asset_face FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER person_updated_at BEFORE UPDATE ON person FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER memory_updated_at BEFORE UPDATE ON memory FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER memory_asset_updated_at BEFORE UPDATE ON memory_asset FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER notification_updated_at BEFORE UPDATE ON notification FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER workflow_updated_at BEFORE UPDATE ON workflow FOR EACH ROW EXECUTE FUNCTION updated_at();

CREATE TRIGGER tag_closure_after_insert
    AFTER INSERT ON tag
    FOR EACH ROW
    EXECUTE FUNCTION tag_closure_after_insert();

-- sync audit delete triggers
CREATE TRIGGER user_delete_audit
    AFTER DELETE ON "user"
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION user_delete_audit();

CREATE TRIGGER partner_delete_audit
    AFTER DELETE ON partner
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION partner_delete_audit();

CREATE TRIGGER asset_delete_audit
    AFTER DELETE ON asset
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION asset_delete_audit();

CREATE TRIGGER album_asset_delete_audit
    AFTER DELETE ON album_asset
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() <= 1)
    EXECUTE FUNCTION album_asset_delete_audit();

CREATE TRIGGER album_user_after_insert
    AFTER INSERT ON album_user
    REFERENCING NEW TABLE AS inserted_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION album_user_after_insert();

CREATE TRIGGER album_user_delete_audit
    AFTER DELETE ON album_user
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() <= 1)
    EXECUTE FUNCTION album_user_delete_audit();

CREATE TRIGGER memory_delete_audit
    AFTER DELETE ON memory
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION memory_delete_audit();

CREATE TRIGGER memory_asset_delete_audit
    AFTER DELETE ON memory_asset
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() <= 1)
    EXECUTE FUNCTION memory_asset_delete_audit();

CREATE TRIGGER stack_delete_audit
    AFTER DELETE ON stack
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION stack_delete_audit();

CREATE TRIGGER person_delete_audit
    AFTER DELETE ON person
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() <= 1)
    EXECUTE FUNCTION person_delete_audit();

CREATE TRIGGER user_metadata_delete_audit
    AFTER DELETE ON user_metadata
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION user_metadata_audit();

CREATE TRIGGER asset_metadata_delete_audit
    AFTER DELETE ON asset_metadata
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION asset_metadata_audit();

CREATE TRIGGER asset_face_delete_audit
    AFTER DELETE ON asset_face
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION asset_face_audit();

CREATE TRIGGER asset_edit_insert
    AFTER INSERT ON asset_edit
    REFERENCING NEW TABLE AS inserted_edit
    FOR EACH STATEMENT
    EXECUTE FUNCTION asset_edit_insert();

CREATE TRIGGER asset_edit_delete
    AFTER DELETE ON asset_edit
    REFERENCING OLD TABLE AS deleted_edit
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION asset_edit_delete();

CREATE TRIGGER asset_edit_audit
    AFTER DELETE ON asset_edit
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION asset_edit_audit();

CREATE TRIGGER asset_ocr_delete_audit
    AFTER DELETE ON asset_ocr
    REFERENCING OLD TABLE AS old
    FOR EACH STATEMENT
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION asset_ocr_delete_audit();

-- ---------------------------------------------------------------------------
-- Seed data
-- ---------------------------------------------------------------------------

INSERT INTO system_metadata (key, value)
VALUES (
    'system-config',
    '{
        "oauth": { "enabled": false, "autoRegister": false, "autoLaunch": false, "buttonText": "Login with OAuth" },
        "passwordLogin": { "enabled": true },
        "machineLearning": { "enabled": false, "urls": [], "clip": { "enabled": false, "modelName": "ViT-B-32__openai" } }
    }'::jsonb
);
