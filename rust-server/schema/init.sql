-- rust-server 完整数据库 schema（首次创建，空库执行）
-- 需要 PostgreSQL 14+ 和 pg_trgm
-- smart_search 需要 pgvector 或 VectorChord，没有则自动跳过
--
--   dropdb -h HOST -U USER --if-exists immich
--   createdb -h HOST -U USER immich
--   psql -h HOST -U USER -d immich -f schema/init.sql
--
-- 或在已有库上直接执行（会先清空 public schema 再重建）：

DROP SCHEMA public CASCADE;
CREATE SCHEMA public;
GRANT ALL ON SCHEMA public TO CURRENT_USER;
GRANT ALL ON SCHEMA public TO public;

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

-- ---------------------------------------------------------------------------
-- Types & tables
-- ---------------------------------------------------------------------------

CREATE TYPE assets_status_enum AS ENUM ('active', 'trashed', 'deleted');
CREATE TYPE sourcetype AS ENUM ('machine-learning', 'exif', 'manual');
CREATE TYPE asset_visibility_enum AS ENUM ('archive', 'timeline', 'hidden', 'locked');
CREATE TYPE asset_checksum_algorithm_enum AS ENUM ('sha1', 'sha1-path');
CREATE TYPE album_user_role_enum AS ENUM ('editor', 'owner', 'viewer');

CREATE TABLE "user" (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    email varchar NOT NULL UNIQUE,
    password varchar NOT NULL DEFAULT '',
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
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7()
);

CREATE INDEX user_updated_at_id_idx ON "user" ("updatedAt", id);

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
    "oauthSid" varchar
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

CREATE TABLE album (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "albumName" varchar NOT NULL DEFAULT 'Untitled Album',
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "albumThumbnailAssetId" uuid REFERENCES asset(id) ON UPDATE CASCADE ON DELETE SET NULL,
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    description text NOT NULL DEFAULT '',
    "deletedAt" timestamptz,
    "isActivityEnabled" boolean NOT NULL DEFAULT true,
    "order" varchar NOT NULL DEFAULT 'desc',
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE
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

CREATE TABLE album_asset (
    "albumId" uuid NOT NULL REFERENCES album(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    PRIMARY KEY ("albumId", "assetId")
);

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

CREATE TABLE asset_face (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "personId" uuid,
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

CREATE TABLE person (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "ownerId" uuid NOT NULL REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    name varchar NOT NULL DEFAULT '',
    "thumbnailPath" varchar NOT NULL DEFAULT '',
    "isHidden" boolean NOT NULL DEFAULT false,
    "birthDate" date,
    "faceAssetId" uuid REFERENCES asset_face(id) ON DELETE SET NULL,
    "isFavorite" boolean NOT NULL DEFAULT false,
    color varchar,
    "updateId" uuid NOT NULL DEFAULT immich_uuid_v7(),
    CONSTRAINT person_birth_date_chk CHECK ("birthDate" <= CURRENT_DATE)
);

ALTER TABLE asset_face
    ADD CONSTRAINT asset_face_person_id_fkey
    FOREIGN KEY ("personId") REFERENCES person(id) ON UPDATE CASCADE ON DELETE SET NULL;

CREATE INDEX asset_face_asset_id_person_id_idx ON asset_face ("assetId", "personId");
CREATE INDEX asset_face_person_id_asset_id_not_deleted_is_visible_idx
    ON asset_face ("personId", "assetId")
    WHERE "deletedAt" IS NULL AND "isVisible" IS TRUE;
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
    "updateId" uuid NOT NULL DEFAULT uuid_generate_v4()
);

CREATE INDEX memory_owner_id_idx ON memory ("ownerId");

CREATE TABLE memory_asset (
    "memoriesId" uuid NOT NULL REFERENCES memory(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "assetId" uuid NOT NULL REFERENCES asset(id) ON UPDATE CASCADE ON DELETE CASCADE,
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "updateId" uuid NOT NULL DEFAULT uuid_generate_v4(),
    PRIMARY KEY ("memoriesId", "assetId")
);

CREATE INDEX memory_asset_memories_id_idx ON memory_asset ("memoriesId");
CREATE INDEX memory_asset_asset_id_idx ON memory_asset ("assetId");

CREATE TABLE notification (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    "updatedAt" timestamptz NOT NULL DEFAULT now(),
    "deletedAt" timestamptz,
    "updateId" uuid NOT NULL DEFAULT uuid_generate_v4(),
    "userId" uuid REFERENCES "user"(id) ON UPDATE CASCADE ON DELETE CASCADE,
    level varchar NOT NULL DEFAULT 'info',
    type varchar NOT NULL DEFAULT 'Custom',
    data jsonb,
    title varchar NOT NULL,
    description text,
    "readAt" timestamptz
);

CREATE INDEX notification_user_id_idx ON notification ("userId");

CREATE TABLE version_history (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    "createdAt" timestamptz NOT NULL DEFAULT now(),
    version varchar NOT NULL
);

-- smart_search：无 pgvector 时自动跳过
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
-- Triggers
-- ---------------------------------------------------------------------------

CREATE TRIGGER user_updated_at BEFORE UPDATE ON "user" FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER session_updated_at BEFORE UPDATE ON session FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER api_key_updated_at BEFORE UPDATE ON api_key FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER user_metadata_updated_at BEFORE UPDATE ON user_metadata FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER library_updated_at BEFORE UPDATE ON library FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER stack_updated_at BEFORE UPDATE ON stack FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_updated_at BEFORE UPDATE ON asset FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_exif_updated_at BEFORE UPDATE ON asset_exif FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_file_updated_at BEFORE UPDATE ON asset_file FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER album_updated_at BEFORE UPDATE ON album FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER album_user_updated_at BEFORE UPDATE ON album_user FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER album_asset_updated_at BEFORE UPDATE ON album_asset FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER tag_updated_at BEFORE UPDATE ON tag FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER partner_updated_at BEFORE UPDATE ON partner FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER asset_face_updated_at BEFORE UPDATE ON asset_face FOR EACH ROW EXECUTE FUNCTION updated_at();
CREATE TRIGGER person_updated_at BEFORE UPDATE ON person FOR EACH ROW EXECUTE FUNCTION updated_at();

CREATE TRIGGER tag_closure_after_insert
    AFTER INSERT ON tag
    FOR EACH ROW
    EXECUTE FUNCTION tag_closure_after_insert();

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
