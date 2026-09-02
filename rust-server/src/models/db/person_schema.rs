use std::sync::OnceLock;

use sqlx::{Pool, Postgres};

use super::schema_check::{detect_person_schema_variant, PersonSchemaVariant};

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
            PersonSchemaVariant::ClusterGroups => format!(
                r#"{person_alias}."personGroupId" = {face_alias}."personGroupId""#
            ),
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
            format!(
                r#"{base} AND {person_alias}."ownerId" = {asset_alias}."ownerId""#
            )
        } else {
            base
        }
    }

    /// WHERE clause matching API person id against the `person` table.
    pub fn where_person_id(&self, prefix: &str, param: &str) -> String {
        format!("{} = {param}", self.person_id_expr(prefix))
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
        assert_eq!(schema.person_id_expr("person."), r#"person."personGroupId""#);
        assert_eq!(schema.person_id_as_id("person."), r#"person."personGroupId" AS id"#);
        assert_eq!(schema.face_person_col(), "personGroupId");
    }

    #[test]
    fn cluster_groups_join_includes_owner() {
        let schema = PersonSchema::for_variant(PersonSchemaVariant::ClusterGroups);
        let join = schema.join_person_to_face_with_owner("p", "af", "a");
        assert!(join.contains(r#"p."ownerId" = a."ownerId""#));
    }
}
