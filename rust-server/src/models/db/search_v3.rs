use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::person_schema::PersonSchema;
use crate::models::dto::search::{
    BoolFilter, DateFilter, DateFilterNullable, EnumFilterString, IdFilter, IdFilterNullable,
    IdsFilter, NumberFilter, NumberFilterNullable, SearchFilter, SearchFilterBranch,
    SearchOrder, SearchOrderDirection, SearchOrderField, StringFilter, StringFilterNullable,
    StringPatternFilter, is_album_confined,
};

#[derive(Debug, Clone)]
pub struct AssetSearchScope {
    pub user_ids: Vec<Uuid>,
    pub locked_owner_id: Uuid,
    pub viewing_user_id: Uuid,
}

#[derive(Debug, Clone, Default)]
pub struct AssetSearchBuilderOptions {
    pub filter: Option<SearchFilter>,
    pub with_stacked: Option<bool>,
}

pub struct SearchPagination {
    pub take: i64,
    pub skip: i64,
}

pub async fn search_metadata_v3_ids(
    pool: &Pool<Postgres>,
    options: &AssetSearchBuilderOptions,
    scope: &AssetSearchScope,
    order: Option<&SearchOrder>,
    pagination: &SearchPagination,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let limit = pagination.take + 1;
    let mut query = QueryBuilder::new(r#"SELECT asset.id FROM asset "#);
    append_search_asset_builder(&mut query, options, scope, &schema);
    append_search_order(&mut query, order);
    query.push(" LIMIT ");
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(pagination.skip);
    query.build_query_scalar().fetch_all(pool).await
}

pub async fn search_statistics_v3_count(
    pool: &Pool<Postgres>,
    options: &AssetSearchBuilderOptions,
    scope: &AssetSearchScope,
) -> Result<i64, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let mut query = QueryBuilder::new(r#"SELECT COUNT(*)::bigint FROM asset "#);
    append_search_asset_builder(&mut query, options, scope, &schema);
    query.build_query_scalar().fetch_one(pool).await
}

pub async fn search_random_v3_ids(
    pool: &Pool<Postgres>,
    options: &AssetSearchBuilderOptions,
    scope: &AssetSearchScope,
    size: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let mut query = QueryBuilder::new(r#"SELECT asset.id FROM asset "#);
    append_search_asset_builder(&mut query, options, scope, &schema);
    query.push(" ORDER BY random() LIMIT ");
    query.push_bind(size);
    query.build_query_scalar().fetch_all(pool).await
}

pub async fn search_smart_v3_ids(
    pool: &Pool<Postgres>,
    options: &AssetSearchBuilderOptions,
    scope: &AssetSearchScope,
    embedding: &str,
    take: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let embedding = super::smart_search::normalize_embedding(embedding);
    let mut tx = pool.begin().await?;
    let _ = sqlx::query("SET LOCAL vchordrq.probes = 1")
        .execute(&mut *tx)
        .await;

    let mut query = QueryBuilder::new(r#"SELECT asset.id FROM asset "#);
    append_search_asset_builder(&mut query, options, scope, &schema);
    query.push(r#" INNER JOIN smart_search ON asset.id = smart_search."assetId" "#);
    query.push(" ORDER BY smart_search.embedding <=> CAST(");
    query.push_bind(embedding);
    query.push(r#" AS vector), asset.id ASC LIMIT "#);
    query.push_bind(take + 1);

    let ids = query.build_query_scalar().fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(ids)
}

fn append_search_asset_builder(
    query: &mut QueryBuilder<'_, Postgres>,
    options: &AssetSearchBuilderOptions,
    scope: &AssetSearchScope,
    schema: &PersonSchema,
) {
    query.push(r#" LEFT JOIN asset_exif ON asset.id = asset_exif."assetId" "#);

    let filter = options.filter.clone().unwrap_or_default();
    let branches = filter.or.clone().unwrap_or_default();
    let top_confined = is_album_confined(&filter.branch);
    let any_branch_confined = branches.iter().any(is_album_confined);
    let scope_per_branch = !top_confined && any_branch_confined;
    let scope_globally = !top_confined && !any_branch_confined;

    if scope_globally {
        query.push(r#" WHERE asset."ownerId" = ANY("#);
        query.push_bind(scope.user_ids.clone());
        query.push(") ");
    } else {
        query.push(" WHERE 1=1 ");
    }

    query.push(
        r#" AND (asset.visibility != 'locked' OR asset."ownerId" = "#,
    );
    query.push_bind(scope.locked_owner_id);
    query.push(") ");

    if options.with_stacked == Some(false) {
        query.push(r#" AND asset."stackId" IS NULL "#);
    }

    append_branch_group(query, &filter.branch, scope, scope_per_branch, false, schema);

    if !branches.is_empty() {
        query.push(" AND (FALSE ");
        for branch in &branches {
            query.push(" OR (TRUE ");
            let needs_owner = scope_per_branch && !is_album_confined(branch);
            append_branch_group(query, branch, scope, scope_per_branch, needs_owner, schema);
            query.push(") ");
        }
        query.push(") ");
    }
}

fn append_branch_group(
    query: &mut QueryBuilder<'_, Postgres>,
    branch: &SearchFilterBranch,
    scope: &AssetSearchScope,
    _scope_per_branch: bool,
    add_owner_predicate: bool,
    schema: &PersonSchema,
) {
    if add_owner_predicate {
        query.push(r#" AND asset."ownerId" = ANY("#);
        query.push_bind(scope.user_ids.clone());
        query.push(") ");
    }

    append_branch_predicates(query, branch, schema);
}

fn append_branch_predicates(
    query: &mut QueryBuilder<'_, Postgres>,
    branch: &SearchFilterBranch,
    schema: &PersonSchema,
) {
    append_id_filter(query, r#"asset.id"#, branch.id.as_ref());
    append_id_nullable_filter(query, r#"asset."libraryId""#, branch.library_id.as_ref());
    append_enum_filter(query, r#"asset.type"#, branch.asset_type.as_ref());
    append_enum_filter(query, r#"asset.visibility"#, branch.visibility.as_ref());

    if let Some(filter) = &branch.is_favorite {
        query.push(r#" AND asset."isFavorite" = "#);
        query.push_bind(filter.eq);
    }
    if let Some(filter) = &branch.is_offline {
        query.push(r#" AND asset."isOffline" = "#);
        query.push_bind(filter.eq);
    }
    if let Some(filter) = &branch.is_motion {
        if filter.eq {
            query.push(r#" AND asset."livePhotoVideoId" IS NOT NULL "#);
        } else {
            query.push(r#" AND asset."livePhotoVideoId" IS NULL "#);
        }
    }

    append_exists_filter(
        query,
        branch.is_encoded.as_ref(),
        r#"
        EXISTS (
            SELECT 1 FROM asset_file
            WHERE asset_file."assetId" = asset.id
              AND asset_file.type = 'encoded_video'
        )
        "#,
    );
    append_exists_filter(
        query,
        branch.has_albums.as_ref(),
        r#"
        EXISTS (
            SELECT 1 FROM album_asset
            WHERE album_asset."assetId" = asset.id
        )
        "#,
    );
    append_exists_filter(
        query,
        branch.has_people.as_ref(),
        r#"
        EXISTS (
            SELECT 1 FROM asset_face
            WHERE asset_face."assetId" = asset.id
              AND asset_face."deletedAt" IS NULL
              AND asset_face."isVisible" = TRUE
        )
        "#,
    );
    append_exists_filter(
        query,
        branch.has_tags.as_ref(),
        r#"
        EXISTS (
            SELECT 1 FROM tag_asset
            WHERE tag_asset."assetId" = asset.id
        )
        "#,
    );

    append_string_nullable_filter(query, r#"asset_exif.city"#, branch.city.as_ref());
    append_string_nullable_filter(query, r#"asset_exif.state"#, branch.state.as_ref());
    append_string_nullable_filter(query, r#"asset_exif.country"#, branch.country.as_ref());
    append_string_nullable_filter(query, r#"asset_exif.make"#, branch.make.as_ref());
    append_string_nullable_filter(query, r#"asset_exif.model"#, branch.model.as_ref());
    append_string_nullable_filter(query, r#"asset_exif."lensModel""#, branch.lens_model.as_ref());
    append_string_pattern_filter(query, r#"asset_exif.description"#, branch.description.as_ref());
    append_string_pattern_filter(query, r#"asset."originalFileName""#, branch.original_file_name.as_ref());
    append_string_pattern_filter(query, r#"asset."originalPath""#, branch.original_path.as_ref());

    if let Some(filter) = &branch.ocr {
        let tokens = crate::utils::search::tokenize_for_search(&filter.matches).join(" ");
        query.push(
            r#"
            AND EXISTS (
                SELECT 1 FROM ocr_search
                WHERE ocr_search."assetId" = asset.id
                  AND f_unaccent(ocr_search.text) %>> f_unaccent("#,
        );
        query.push_bind(tokens);
        query.push(") ) ");
    }

    append_number_nullable_filter(query, r#"asset_exif.rating"#, branch.rating.as_ref());
    append_number_filter(query, r#"asset_exif."fileSizeInByte""#, branch.file_size_in_bytes.as_ref());
    append_date_filter(query, r#"asset."fileCreatedAt""#, branch.taken_at.as_ref());
    append_date_filter(query, r#"asset."createdAt""#, branch.created_at.as_ref());
    append_date_filter(query, r#"asset."updatedAt""#, branch.updated_at.as_ref());
    append_date_nullable_filter(query, r#"asset."deletedAt""#, branch.trashed_at.as_ref());

    append_ids_filter(query, IdsFilterKind::Album, branch.album_ids.as_ref(), schema);
    append_ids_filter(query, IdsFilterKind::Person, branch.person_ids.as_ref(), schema);
    append_ids_filter(query, IdsFilterKind::Tag, branch.tag_ids.as_ref(), schema);
    append_checksum_filter(query, branch.checksum.as_ref());

    if let Some(filter) = &branch.encoded_video_path {
        query.push(
            r#"
            AND EXISTS (
                SELECT 1 FROM asset_file
                WHERE asset_file."assetId" = asset.id
                  AND asset_file.type = 'encoded_video'
                  AND asset_file."isEdited" = FALSE
            "#,
        );
        append_string_filter_on_builder(query, r#"asset_file.path"#, filter);
        query.push(") ");
    }
}

enum IdsFilterKind {
    Album,
    Person,
    Tag,
}

fn append_ids_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    kind: IdsFilterKind,
    filter: Option<&IdsFilter>,
    schema: &PersonSchema,
) {
    let Some(filter) = filter else { return };
    let face_col = schema.face_person_col_quoted();

    if let Some(ids) = &filter.any {
        if !ids.is_empty() {
        match kind {
            IdsFilterKind::Album => {
                query.push(
                    r#"
                    AND EXISTS (
                        SELECT 1 FROM album_asset
                        WHERE album_asset."assetId" = asset.id
                          AND album_asset."albumId" = ANY(
                    "#,
                );
                query.push_bind(ids.clone());
                query.push(") ) ");
            }
            IdsFilterKind::Person => {
                query.push(
                    format!(
                        r#"
                    AND EXISTS (
                        SELECT 1 FROM asset_face
                        WHERE asset_face."assetId" = asset.id
                          AND asset_face.{face_col} = ANY(
                    "#
                    ),
                );
                query.push_bind(ids.clone());
                query.push(
                    r#")
                          AND asset_face."deletedAt" IS NULL
                          AND asset_face."isVisible" = TRUE
                    ) "#,
                );
            }
            IdsFilterKind::Tag => {
                query.push(
                    r#"
                    AND EXISTS (
                        SELECT 1
                        FROM tag_closure
                        INNER JOIN tag_asset ON tag_asset."tagId" = tag_closure.id_descendant
                        WHERE tag_asset."assetId" = asset.id
                          AND tag_closure.id_ancestor = ANY(
                    "#,
                );
                query.push_bind(ids.clone());
                query.push(") ) ");
            }
        }
        }
    }

    if let Some(ids) = &filter.all {
        if !ids.is_empty() {
        match kind {
            IdsFilterKind::Album => {
                query.push(
                    r#"
                    AND EXISTS (
                        SELECT 1 FROM album_asset
                        WHERE album_asset."assetId" = asset.id
                          AND album_asset."albumId" = ANY(
                    "#,
                );
                query.push_bind(ids.clone());
                query.push(
                    r#")
                        GROUP BY album_asset."assetId"
                        HAVING COUNT(DISTINCT album_asset."albumId") = "#,
                );
                query.push_bind(ids.len() as i64);
                query.push(") ");
            }
            IdsFilterKind::Person => {
                query.push(
                    format!(
                        r#"
                    AND EXISTS (
                        SELECT 1 FROM asset_face
                        WHERE asset_face."assetId" = asset.id
                          AND asset_face.{face_col} = ANY(
                    "#
                    ),
                );
                query.push_bind(ids.clone());
                query.push(
                    format!(
                        r#")
                          AND asset_face."deletedAt" IS NULL
                          AND asset_face."isVisible" = TRUE
                        GROUP BY asset_face."assetId"
                        HAVING COUNT(DISTINCT asset_face.{face_col}) = "#
                    ),
                );
                query.push_bind(ids.len() as i64);
                query.push(") ");
            }
            IdsFilterKind::Tag => {
                query.push(
                    r#"
                    AND EXISTS (
                        SELECT 1
                        FROM tag_closure
                        INNER JOIN tag_asset ON tag_asset."tagId" = tag_closure.id_descendant
                        WHERE tag_asset."assetId" = asset.id
                          AND tag_closure.id_ancestor = ANY(
                    "#,
                );
                query.push_bind(ids.clone());
                query.push(
                    r#")
                        GROUP BY tag_asset."assetId"
                        HAVING COUNT(DISTINCT tag_closure.id_ancestor) = "#,
                );
                query.push_bind(ids.len() as i64);
                query.push(") ");
            }
        }
        }
    }

    if let Some(ids) = &filter.none {
        if !ids.is_empty() {
        match kind {
            IdsFilterKind::Album => {
                query.push(
                    r#"
                    AND NOT EXISTS (
                        SELECT 1 FROM album_asset
                        WHERE album_asset."assetId" = asset.id
                          AND album_asset."albumId" = ANY(
                    "#,
                );
                query.push_bind(ids.clone());
                query.push(") ) ");
            }
            IdsFilterKind::Person => {
                query.push(
                    format!(
                        r#"
                    AND NOT EXISTS (
                        SELECT 1 FROM asset_face
                        WHERE asset_face."assetId" = asset.id
                          AND asset_face.{face_col} = ANY(
                    "#
                    ),
                );
                query.push_bind(ids.clone());
                query.push(
                    r#")
                          AND asset_face."deletedAt" IS NULL
                          AND asset_face."isVisible" = TRUE
                    ) "#,
                );
            }
            IdsFilterKind::Tag => {
                query.push(
                    r#"
                    AND NOT EXISTS (
                        SELECT 1
                        FROM tag_closure
                        INNER JOIN tag_asset ON tag_asset."tagId" = tag_closure.id_descendant
                        WHERE tag_asset."assetId" = asset.id
                          AND tag_closure.id_ancestor = ANY(
                    "#,
                );
                query.push_bind(ids.clone());
                query.push(") ) ");
            }
        }
        }
    }
}

fn append_exists_filter(query: &mut QueryBuilder<'_, Postgres>, filter: Option<&BoolFilter>, exists_sql: &str) {
    let Some(filter) = filter else { return };
    if filter.eq {
        query.push(" AND ");
        query.push(exists_sql);
    } else {
        query.push(" AND NOT ");
        query.push(exists_sql);
    }
}

fn append_id_filter(query: &mut QueryBuilder<'_, Postgres>, column: &str, filter: Option<&IdFilter>) {
    let Some(filter) = filter else { return };
    if let Some(value) = filter.eq {
        query.push(format!(" AND {column} = "));
        query.push_bind(value);
    }
    if let Some(value) = filter.ne {
        query.push(format!(" AND {column} != "));
        query.push_bind(value);
    }
}

fn append_id_nullable_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    filter: Option<&IdFilterNullable>,
) {
    let Some(filter) = filter else { return };
    if let Some(value) = &filter.eq {
        query.push(format!(" AND {column} "));
        if value.is_none() {
            query.push("IS NULL ");
        } else {
            query.push("= ");
            query.push_bind(value.unwrap());
        }
    }
    if let Some(value) = &filter.ne {
        query.push(format!(" AND {column} "));
        if value.is_none() {
            query.push("IS NOT NULL ");
        } else {
            query.push("!= ");
            query.push_bind(value.unwrap());
        }
    }
}

fn append_enum_filter(query: &mut QueryBuilder<'_, Postgres>, column: &str, filter: Option<&EnumFilterString>) {
    let Some(filter) = filter else { return };
    if let Some(value) = &filter.eq {
        query.push(format!(" AND {column} = "));
        query.push_bind(value.clone());
    }
    if let Some(value) = &filter.ne {
        query.push(format!(" AND {column} != "));
        query.push_bind(value.clone());
    }
    if let Some(values) = &filter.in_values {
        query.push(format!(" AND {column} = ANY("));
        query.push_bind(values.clone());
        query.push(") ");
    }
    if let Some(values) = &filter.not_in {
        query.push(format!(" AND NOT ({column} = ANY("));
        query.push_bind(values.clone());
        query.push(")) ");
    }
}

fn append_string_filter_on_builder(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    filter: &StringFilter,
) {
    if let Some(value) = &filter.eq {
        query.push(format!(" AND {column} = "));
        query.push_bind(value.clone());
    }
    if let Some(value) = &filter.ne {
        query.push(format!(" AND {column} != "));
        query.push_bind(value.clone());
    }
    if let Some(values) = &filter.in_values {
        query.push(format!(" AND {column} = ANY("));
        query.push_bind(values.clone());
        query.push(") ");
    }
    if let Some(values) = &filter.not_in {
        query.push(format!(" AND NOT ({column} = ANY("));
        query.push_bind(values.clone());
        query.push(")) ");
    }
}

fn append_string_nullable_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    filter: Option<&StringFilterNullable>,
) {
    let Some(filter) = filter else { return };
    if let Some(value) = &filter.eq {
        query.push(format!(" AND {column} "));
        if let Some(text) = value {
            query.push("= ");
            query.push_bind(text.clone());
        } else {
            query.push("IS NULL ");
        }
    }
    if let Some(value) = &filter.ne {
        query.push(format!(" AND {column} "));
        if let Some(text) = value {
            query.push("!= ");
            query.push_bind(text.clone());
        } else {
            query.push("IS NOT NULL ");
        }
    }
    if let Some(values) = &filter.in_values {
        query.push(format!(" AND {column} = ANY("));
        query.push_bind(values.clone());
        query.push(") ");
    }
    if let Some(values) = &filter.not_in {
        query.push(format!(" AND NOT ({column} = ANY("));
        query.push_bind(values.clone());
        query.push(")) ");
    }
}

fn append_string_pattern_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    filter: Option<&StringPatternFilter>,
) {
    let Some(filter) = filter else { return };
    append_string_nullable_filter(query, column, Some(&filter.base));
    if let Some(value) = &filter.like {
        query.push(format!(" AND f_unaccent({column}) ILIKE '%' || f_unaccent("));
        query.push_bind(value.clone());
        query.push(") || '%' ");
    }
    if let Some(value) = &filter.not_like {
        query.push(format!(" AND f_unaccent({column}) NOT ILIKE '%' || f_unaccent("));
        query.push_bind(value.clone());
        query.push(") || '%' ");
    }
    if let Some(value) = &filter.starts_with {
        query.push(format!(" AND f_unaccent({column}) ILIKE f_unaccent("));
        query.push_bind(value.clone());
        query.push(") || '%' ");
    }
    if let Some(value) = &filter.ends_with {
        query.push(format!(" AND f_unaccent({column}) ILIKE '%' || f_unaccent("));
        query.push_bind(value.clone());
        query.push(") ");
    }
}

fn append_number_filter(query: &mut QueryBuilder<'_, Postgres>, column: &str, filter: Option<&NumberFilter>) {
    let Some(filter) = filter else { return };
    append_number_comparison(query, column, filter);
}

fn append_number_nullable_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    filter: Option<&NumberFilterNullable>,
) {
    let Some(filter) = filter else { return };
    if let Some(value) = &filter.eq {
        query.push(format!(" AND {column} "));
        if let Some(number) = value {
            query.push("= ");
            query.push_bind(*number);
        } else {
            query.push("IS NULL ");
        }
    }
    if let Some(value) = &filter.ne {
        query.push(format!(" AND {column} "));
        if let Some(number) = value {
            query.push("!= ");
            query.push_bind(*number);
        } else {
            query.push("IS NOT NULL ");
        }
    }
    append_number_comparison(query, column, filter);
}

fn append_number_comparison<T: NumberComparison>(query: &mut QueryBuilder<'_, Postgres>, column: &str, filter: &T) {
    if let Some(value) = filter.lt() {
        query.push(format!(" AND {column} < "));
        query.push_bind(value);
    }
    if let Some(value) = filter.lte() {
        query.push(format!(" AND {column} <= "));
        query.push_bind(value);
    }
    if let Some(value) = filter.gt() {
        query.push(format!(" AND {column} > "));
        query.push_bind(value);
    }
    if let Some(value) = filter.gte() {
        query.push(format!(" AND {column} >= "));
        query.push_bind(value);
    }
    if let Some(values) = filter.in_values() {
        query.push(format!(" AND {column} = ANY("));
        query.push_bind(values.clone());
        query.push(") ");
    }
    if let Some(values) = filter.not_in() {
        query.push(format!(" AND NOT ({column} = ANY("));
        query.push_bind(values.clone());
        query.push(")) ");
    }
    if let Some(value) = filter.eq_number() {
        query.push(format!(" AND {column} = "));
        query.push_bind(value);
    }
    if let Some(value) = filter.ne_number() {
        query.push(format!(" AND {column} != "));
        query.push_bind(value);
    }
}

trait NumberComparison {
    fn eq_number(&self) -> Option<f64> {
        None
    }
    fn ne_number(&self) -> Option<f64> {
        None
    }
    fn lt(&self) -> Option<f64>;
    fn lte(&self) -> Option<f64>;
    fn gt(&self) -> Option<f64>;
    fn gte(&self) -> Option<f64>;
    fn in_values(&self) -> Option<&Vec<f64>>;
    fn not_in(&self) -> Option<&Vec<f64>>;
}

impl NumberComparison for NumberFilter {
    fn eq_number(&self) -> Option<f64> {
        self.eq
    }
    fn ne_number(&self) -> Option<f64> {
        self.ne
    }
    fn lt(&self) -> Option<f64> {
        self.lt
    }
    fn lte(&self) -> Option<f64> {
        self.lte
    }
    fn gt(&self) -> Option<f64> {
        self.gt
    }
    fn gte(&self) -> Option<f64> {
        self.gte
    }
    fn in_values(&self) -> Option<&Vec<f64>> {
        self.in_values.as_ref()
    }
    fn not_in(&self) -> Option<&Vec<f64>> {
        self.not_in.as_ref()
    }
}

impl NumberComparison for NumberFilterNullable {
    fn lt(&self) -> Option<f64> {
        self.lt
    }
    fn lte(&self) -> Option<f64> {
        self.lte
    }
    fn gt(&self) -> Option<f64> {
        self.gt
    }
    fn gte(&self) -> Option<f64> {
        self.gte
    }
    fn in_values(&self) -> Option<&Vec<f64>> {
        self.in_values.as_ref()
    }
    fn not_in(&self) -> Option<&Vec<f64>> {
        self.not_in.as_ref()
    }
}

fn append_date_filter(query: &mut QueryBuilder<'_, Postgres>, column: &str, filter: Option<&DateFilter>) {
    let Some(filter) = filter else { return };
    append_date_comparison(query, column, filter);
}

fn append_date_nullable_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    filter: Option<&DateFilterNullable>,
) {
    let Some(filter) = filter else { return };
    if let Some(value) = &filter.eq {
        query.push(format!(" AND {column} "));
        if let Some(date) = value {
            query.push("= ");
            query.push_bind(*date);
        } else {
            query.push("IS NULL ");
        }
    }
    if let Some(value) = &filter.ne {
        query.push(format!(" AND {column} "));
        if let Some(date) = value {
            query.push("!= ");
            query.push_bind(*date);
        } else {
            query.push("IS NOT NULL ");
        }
    }
    append_date_comparison(query, column, filter);
}

trait DateComparison {
    fn lt(&self) -> Option<DateTime<Utc>>;
    fn lte(&self) -> Option<DateTime<Utc>>;
    fn gt(&self) -> Option<DateTime<Utc>>;
    fn gte(&self) -> Option<DateTime<Utc>>;
    fn eq_date(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn ne_date(&self) -> Option<DateTime<Utc>> {
        None
    }
}

impl DateComparison for DateFilter {
    fn eq_date(&self) -> Option<DateTime<Utc>> {
        self.eq
    }
    fn ne_date(&self) -> Option<DateTime<Utc>> {
        self.ne
    }
    fn lt(&self) -> Option<DateTime<Utc>> {
        self.lt
    }
    fn lte(&self) -> Option<DateTime<Utc>> {
        self.lte
    }
    fn gt(&self) -> Option<DateTime<Utc>> {
        self.gt
    }
    fn gte(&self) -> Option<DateTime<Utc>> {
        self.gte
    }
}

impl DateComparison for DateFilterNullable {
    fn lt(&self) -> Option<DateTime<Utc>> {
        self.lt
    }
    fn lte(&self) -> Option<DateTime<Utc>> {
        self.lte
    }
    fn gt(&self) -> Option<DateTime<Utc>> {
        self.gt
    }
    fn gte(&self) -> Option<DateTime<Utc>> {
        self.gte
    }
}

fn append_date_comparison<T: DateComparison>(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    filter: &T,
) {
    if let Some(value) = filter.eq_date() {
        query.push(format!(" AND {column} = "));
        query.push_bind(value);
    }
    if let Some(value) = filter.ne_date() {
        query.push(format!(" AND {column} != "));
        query.push_bind(value);
    }
    if let Some(value) = filter.lt() {
        query.push(format!(" AND {column} < "));
        query.push_bind(value);
    }
    if let Some(value) = filter.lte() {
        query.push(format!(" AND {column} <= "));
        query.push_bind(value);
    }
    if let Some(value) = filter.gt() {
        query.push(format!(" AND {column} > "));
        query.push_bind(value);
    }
    if let Some(value) = filter.gte() {
        query.push(format!(" AND {column} >= "));
        query.push_bind(value);
    }
}

fn append_checksum_filter(query: &mut QueryBuilder<'_, Postgres>, filter: Option<&StringFilter>) {
    let Some(filter) = filter else { return };
    if let Some(value) = &filter.eq {
        if let Some(checksum) = decode_checksum(value) {
            query.push(r#" AND asset.checksum = "#);
            query.push_bind(checksum);
        }
    }
    if let Some(value) = &filter.ne {
        if let Some(checksum) = decode_checksum(value) {
            query.push(r#" AND asset.checksum != "#);
            query.push_bind(checksum);
        }
    }
    if let Some(values) = &filter.in_values {
        let checksums: Vec<Vec<u8>> = values.iter().filter_map(|value| decode_checksum(value)).collect();
        if !checksums.is_empty() {
            query.push(r#" AND asset.checksum = ANY("#);
            query.push_bind(checksums);
            query.push(") ");
        }
    }
    if let Some(values) = &filter.not_in {
        let checksums: Vec<Vec<u8>> = values.iter().filter_map(|value| decode_checksum(value)).collect();
        if !checksums.is_empty() {
            query.push(r#" AND NOT (asset.checksum = ANY("#);
            query.push_bind(checksums);
            query.push(")) ");
        }
    }
}

fn decode_checksum(value: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    if value.len() == 28 {
        base64::engine::general_purpose::STANDARD
            .decode(value)
            .ok()
    } else {
        hex::decode(value).ok()
    }
}

fn append_search_order(query: &mut QueryBuilder<'_, Postgres>, order: Option<&SearchOrder>) {
    let order = order.cloned().unwrap_or_default();
    let direction = match order.direction {
        SearchOrderDirection::Asc => "ASC",
        SearchOrderDirection::Desc => "DESC",
    };

    let (column, nullable) = match order.field {
        SearchOrderField::FileCreatedAt => (r#"asset."fileCreatedAt""#, false),
        SearchOrderField::LocalDateTime => (r#"asset."localDateTime""#, false),
        SearchOrderField::FileSizeInBytes => (r#"asset_exif."fileSizeInByte""#, true),
        SearchOrderField::Rating => ("asset_exif.rating", true),
    };

    query.push(" ORDER BY ");
    query.push(column);
    if nullable {
        query.push(format!(" {direction} NULLS LAST, asset.id {direction}"));
    } else {
        query.push(format!(" {direction}, asset.id {direction}"));
    }
}
