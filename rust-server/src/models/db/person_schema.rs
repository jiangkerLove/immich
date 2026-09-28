use std::sync::OnceLock;

use sqlx::{Pool, Postgres};

use super::schema_check::{PersonSchemaVariant, detect_person_schema_variant};

static CACHED_VARIANT: OnceLock<PersonSchemaVariant> = OnceLock::new();

/// Runtime person/face schema compatibility layer for legacy vs cluster-groups databases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonSchema {
    pub variant: PersonSchemaVariant,
}

impl PersonSchema {
    pub async fn get(pool: &Pool<Postgres>) -> Result<Self, sqlx::Error> {
        if let Some(variant) = CACHED_VARIANT.get() {
            return Ok(Self { variant: *variant });
        }
        let variant = detect_person_schema_variant(pool).await?;
        let _ = CACHED_VARIANT.set(variant);
        Ok(Self { variant })
    }

    #[cfg(test)]
    pub fn for_variant(variant: PersonSchemaVariant) -> Self {
        Self { variant }
    }

    pub fn is_cluster_groups(self) -> bool {
        self.variant == PersonSchemaVariant::ClusterGroups
    }

    /// API `person.id` column expression with optional table prefix (e.g. `person.`).
    pub fn person_id_expr(&self, prefix: &str) -> String {
        match self.variant {
            PersonSchemaVariant::Legacy => format!("{prefix}id"),
            PersonSchemaVariant::ClusterGroups => format!(r#"{prefix}"personGroupId""#),
        }
    }

    /// Same as [`person_id_expr`] but aliased as `id` for SELECT lists.
    pub fn person_id_as_id(&self, prefix: &str) -> String {
        match self.variant {
            PersonSchemaVariant::Legacy => format!("{prefix}id"),
            PersonSchemaVariant::ClusterGroups => format!(r#"{prefix}"personGroupId" AS id"#),
        }
    }

    /// `asset_face` foreign-key column referencing a person group / person.
    pub fn face_person_col(&self) -> &'static str {
        match self.variant {
            PersonSchemaVariant::Legacy => "personId",
            PersonSchemaVariant::ClusterGroups => "personGroupId",
        }
    }

    pub fn face_person_col_quoted(&self) -> String {
        format!(r#""{}""#, self.face_person_col())
    }

    /// Join condition between a `person` row and an `asset_face` row.
    pub fn join_person_to_face(&self, person_alias: &str, face_alias: &str) -> String {
        match self.variant {
            PersonSchemaVariant::Legacy => {
                format!(r#"{person_alias}.id = {face_alias}."personId""#)
            }
            PersonSchemaVariant::ClusterGroups => {
                format!(r#"{person_alias}."personGroupId" = {face_alias}."personGroupId""#)
            }
        }
    }

    /// Join `person` to `asset_face` including owner scope (required on cluster-groups schema).
    pub fn join_person_to_face_with_owner(
        &self,
        person_alias: &str,
        face_alias: &str,
        asset_alias: &str,
    ) -> String {
        let base = self.join_person_to_face(person_alias, face_alias);
        if self.is_cluster_groups() {
            format!(r#"{base} AND {person_alias}."ownerId" = {asset_alias}."ownerId""#)
        } else {
            base
        }
    }

    /// WHERE clause matching API person id against the `person` table.
    pub fn where_person_id(&self, prefix: &str, param: &str) -> String {
        format!("{} = {param}", self.person_id_expr(prefix))
    }

    /// Match a person by owner and API id (`id` or `personGroupId`).
    pub fn where_owner_and_id(&self, prefix: &str, owner_param: &str, id_param: &str) -> String {
        let owner = if prefix.is_empty() {
            r#""ownerId""#.to_string()
        } else {
            format!(r#"{prefix}"ownerId""#)
        };
        format!(
            "{owner} = {owner_param} AND {}",
            self.where_person_id(prefix, id_param)
        )
    }

    /// Sync payload column: audit table person reference exposed as `personId`.
    pub fn audit_person_id_select(&self) -> String {
        match self.variant {
            PersonSchemaVariant::Legacy => r#""personId""#.to_string(),
            PersonSchemaVariant::ClusterGroups => r#""personGroupId" AS "personId""#.to_string(),
        }
    }

    /// Sync payload column: face person reference exposed as `personId`.
    pub fn sync_face_person_id_select(&self, face_alias: &str) -> String {
        format!(
            r#"{}.{}"#,
            face_alias,
            match self.variant {
                PersonSchemaVariant::Legacy => r#""personId""#.to_string(),
                PersonSchemaVariant::ClusterGroups => {
                    r#""personGroupId" AS "personId""#.to_string()
                }
            }
        )
    }

    pub fn person_select_columns(&self, prefix: &str) -> String {
        format!(
            r#"
    {person_id},
    {prefix}name,
    {prefix}"birthDate" as birth_date,
    {prefix}"thumbnailPath" as thumbnail_path,
    {prefix}"isHidden" as is_hidden,
    {prefix}"isFavorite" as is_favorite,
    {prefix}color,
    {prefix}"updatedAt" as updated_at"#,
            person_id = self.person_id_as_id(prefix),
            prefix = prefix
        )
    }

    pub fn person_list_select_columns(&self) -> String {
        self.person_select_columns("person.")
    }

    /// Exclude a face when any person in its group was born after the asset date.
    /// `$5` is the asset `fileCreatedAt`. Matches TypeScript `searchFaces` `minBirthDate`.
    pub fn face_birth_date_exclusion(&self, face_alias: &str) -> String {
        let person_match = match self.variant {
            PersonSchemaVariant::Legacy => {
                format!(r#"person.id = {face_alias}."personId""#)
            }
            PersonSchemaVariant::ClusterGroups => {
                format!(r#"person."personGroupId" = {face_alias}."personGroupId""#)
            }
        };
        format!(
            r#"($5::timestamptz IS NULL OR NOT EXISTS (
                SELECT 1 FROM person
                WHERE {person_match}
                  AND person."birthDate" > $5::date
            ))"#
        )
    }
}

pub async fn create_person_group(
    pool: &Pool<Postgres>,
    owner_id: &uuid::Uuid,
    group_id: Option<&uuid::Uuid>,
) -> Result<uuid::Uuid, sqlx::Error> {
    match group_id {
        Some(id) => {
            sqlx::query_scalar(
                r#"
                INSERT INTO person_group (id, "clusterGroupId")
                SELECT $1, "clusterGroupId" FROM "user" WHERE id = $2
                RETURNING id
                "#,
            )
            .bind(id)
            .bind(owner_id)
            .fetch_one(pool)
            .await
        }
        None => {
            sqlx::query_scalar(
                r#"
                INSERT INTO person_group ("clusterGroupId")
                SELECT "clusterGroupId" FROM "user" WHERE id = $1
                RETURNING id
                "#,
            )
            .bind(owner_id)
            .fetch_one(pool)
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_person_id_expr() {
        let schema = PersonSchema::for_variant(PersonSchemaVariant::Legacy);
        assert_eq!(schema.person_id_expr("person."), "person.id");
        assert_eq!(schema.face_person_col(), "personId");
    }

    #[test]
    fn cluster_groups_person_id_expr() {
        let schema = PersonSchema::for_variant(PersonSchemaVariant::ClusterGroups);
        assert_eq!(
            schema.person_id_expr("person."),
            r#"person."personGroupId""#
        );
        assert_eq!(
            schema.person_id_as_id("person."),
            r#"person."personGroupId" AS id"#
        );
        assert_eq!(schema.face_person_col(), "personGroupId");
    }

    #[test]
    fn birth_date_exclusion_covers_the_person_group() {
        let legacy = PersonSchema::for_variant(PersonSchemaVariant::Legacy);
        let legacy_sql = legacy.face_birth_date_exclusion("asset_face");
        assert!(legacy_sql.contains(r#"person.id = asset_face."personId""#));

        let cluster = PersonSchema::for_variant(PersonSchemaVariant::ClusterGroups);
        let cluster_sql = cluster.face_birth_date_exclusion("asset_face");
        assert!(cluster_sql.contains(r#"person."personGroupId" = asset_face."personGroupId""#));
        assert!(cluster_sql.contains(r#"person."birthDate" > $5::date"#));
    }

    #[test]
    fn cluster_groups_join_includes_owner() {
        let schema = PersonSchema::for_variant(PersonSchemaVariant::ClusterGroups);
        let join = schema.join_person_to_face_with_owner("p", "af", "a");
        assert!(join.contains(r#"p."ownerId" = a."ownerId""#));
    }

    #[test]
    fn where_owner_and_id_matches_schema() {
        let legacy = PersonSchema::for_variant(PersonSchemaVariant::Legacy);
        assert_eq!(
            legacy.where_owner_and_id("person.", "$1", "$2"),
            r#"person."ownerId" = $1 AND person.id = $2"#
        );
        let groups = PersonSchema::for_variant(PersonSchemaVariant::ClusterGroups);
        assert_eq!(
            groups.where_owner_and_id("person.", "$1", "$2"),
            r#"person."ownerId" = $1 AND person."personGroupId" = $2"#
        );
    }
}
