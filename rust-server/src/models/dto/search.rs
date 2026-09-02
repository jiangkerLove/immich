use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdFilter {
    pub eq: Option<Uuid>,
    pub ne: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdFilterNullable {
    pub eq: Option<Option<Uuid>>,
    pub ne: Option<Option<Uuid>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdsFilter {
    pub any: Option<Vec<Uuid>>,
    pub all: Option<Vec<Uuid>>,
    pub none: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringFilter {
    pub eq: Option<String>,
    pub ne: Option<String>,
    #[serde(rename = "in")]
    pub in_values: Option<Vec<String>>,
    pub not_in: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringFilterNullable {
    pub eq: Option<Option<String>>,
    pub ne: Option<Option<String>>,
    #[serde(rename = "in")]
    pub in_values: Option<Vec<String>>,
    pub not_in: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringPatternFilter {
    #[serde(flatten)]
    pub base: StringFilterNullable,
    pub like: Option<String>,
    pub not_like: Option<String>,
    pub starts_with: Option<String>,
    pub ends_with: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberFilter {
    pub eq: Option<f64>,
    pub ne: Option<f64>,
    pub lt: Option<f64>,
    pub lte: Option<f64>,
    pub gt: Option<f64>,
    pub gte: Option<f64>,
    #[serde(rename = "in")]
    pub in_values: Option<Vec<f64>>,
    pub not_in: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberFilterNullable {
    pub eq: Option<Option<f64>>,
    pub ne: Option<Option<f64>>,
    pub lt: Option<f64>,
    pub lte: Option<f64>,
    pub gt: Option<f64>,
    pub gte: Option<f64>,
    #[serde(rename = "in")]
    pub in_values: Option<Vec<f64>>,
    pub not_in: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateFilter {
    pub eq: Option<DateTime<Utc>>,
    pub ne: Option<DateTime<Utc>>,
    pub lt: Option<DateTime<Utc>>,
    pub lte: Option<DateTime<Utc>>,
    pub gt: Option<DateTime<Utc>>,
    pub gte: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateFilterNullable {
    pub eq: Option<Option<DateTime<Utc>>>,
    pub ne: Option<Option<DateTime<Utc>>>,
    pub lt: Option<DateTime<Utc>>,
    pub lte: Option<DateTime<Utc>>,
    pub gt: Option<DateTime<Utc>>,
    pub gte: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoolFilter {
    pub eq: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumFilterString {
    pub eq: Option<String>,
    pub ne: Option<String>,
    #[serde(rename = "in")]
    pub in_values: Option<Vec<String>>,
    pub not_in: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringSimilarityFilter {
    pub matches: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SearchOrderField {
    #[default]
    FileCreatedAt,
    LocalDateTime,
    FileSizeInBytes,
    Rating,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SearchOrderDirection {
    #[default]
    Desc,
    Asc,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOrder {
    #[serde(default)]
    pub field: SearchOrderField,
    #[serde(default)]
    pub direction: SearchOrderDirection,
}

impl Default for SearchOrder {
    fn default() -> Self {
        Self {
            field: SearchOrderField::FileCreatedAt,
            direction: SearchOrderDirection::Desc,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilterBranch {
    pub id: Option<IdFilter>,
    pub library_id: Option<IdFilterNullable>,
    #[serde(rename = "type")]
    pub asset_type: Option<EnumFilterString>,
    pub visibility: Option<EnumFilterString>,
    pub is_favorite: Option<BoolFilter>,
    pub is_motion: Option<BoolFilter>,
    pub is_offline: Option<BoolFilter>,
    pub is_encoded: Option<BoolFilter>,
    pub has_albums: Option<BoolFilter>,
    pub has_people: Option<BoolFilter>,
    pub has_tags: Option<BoolFilter>,
    pub city: Option<StringFilterNullable>,
    pub state: Option<StringFilterNullable>,
    pub country: Option<StringFilterNullable>,
    pub make: Option<StringFilterNullable>,
    pub model: Option<StringFilterNullable>,
    pub lens_model: Option<StringFilterNullable>,
    pub description: Option<StringPatternFilter>,
    pub original_file_name: Option<StringPatternFilter>,
    pub original_path: Option<StringPatternFilter>,
    pub ocr: Option<StringSimilarityFilter>,
    pub rating: Option<NumberFilterNullable>,
    pub file_size_in_bytes: Option<NumberFilter>,
    pub taken_at: Option<DateFilter>,
    pub created_at: Option<DateFilter>,
    pub updated_at: Option<DateFilter>,
    pub trashed_at: Option<DateFilterNullable>,
    pub person_ids: Option<IdsFilter>,
    pub tag_ids: Option<IdsFilter>,
    pub album_ids: Option<IdsFilter>,
    pub checksum: Option<StringFilter>,
    pub encoded_video_path: Option<StringFilter>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilter {
    #[serde(flatten)]
    pub branch: SearchFilterBranch,
    pub or: Option<Vec<SearchFilterBranch>>,
}

pub const NEW_SHAPE_FIELDS: &[&str] = &["filter", "orderBy", "cursor"];

pub fn is_new_shape_request(filter: &Option<SearchFilter>, order_by: &Option<SearchOrder>, cursor: &Option<String>) -> bool {
    filter.is_some() || order_by.is_some() || cursor.is_some()
}

pub fn is_album_confined(branch: &SearchFilterBranch) -> bool {
    branch
        .album_ids
        .as_ref()
        .is_some_and(|ids| ids.any.is_some() || ids.all.is_some())
}

pub fn is_fully_album_confined(filter: &SearchFilter) -> bool {
    is_album_confined(&filter.branch)
        || filter
            .or
            .as_ref()
            .is_some_and(|branches| !branches.is_empty() && branches.iter().all(is_album_confined))
}

pub fn collect_filter_ids(filter: &SearchFilter, field: FilterIdsField) -> Vec<Uuid> {
    let mut ids = std::collections::HashSet::new();
    for branch in filter_branches(filter) {
        let ids_filter = match field {
            FilterIdsField::AlbumIds => &branch.album_ids,
            FilterIdsField::PersonIds => &branch.person_ids,
            FilterIdsField::TagIds => &branch.tag_ids,
        };
        if let Some(ids_filter) = ids_filter {
            for list in [&ids_filter.any, &ids_filter.all, &ids_filter.none] {
                if let Some(values) = list {
                    ids.extend(values.iter().copied());
                }
            }
        }
    }
    ids.into_iter().collect()
}

pub enum FilterIdsField {
    AlbumIds,
    PersonIds,
    TagIds,
}

fn filter_branches(filter: &SearchFilter) -> Vec<&SearchFilterBranch> {
    let mut branches = vec![&filter.branch];
    if let Some(or) = &filter.or {
        branches.extend(or.iter());
    }
    branches
}
