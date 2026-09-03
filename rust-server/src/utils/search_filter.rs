use crate::models::dto::auth::AuthDto;
use crate::models::dto::search::{EnumFilterString, SearchFilter, SearchFilterBranch};
use crate::models::response::response::ErrorResp;

const VISIBILITY_LOCKED: &str = "locked";

pub fn apply_locked_visibility_policy(auth: &AuthDto, filter: SearchFilter) -> Result<SearchFilter, ErrorResp> {
    let elevated = auth
        .session
        .as_ref()
        .is_some_and(|session| session.has_elevated_permission);
    if elevated {
        return Ok(filter);
    }

    if deciding_conditions(&filter, VisibilityField::Visibility)
        .iter()
        .any(|condition| can_match_visibility(condition, VISIBILITY_LOCKED))
    {
        return Err(ErrorResp::Forbidden("Forbidden".to_string()));
    }

    if filter.branch.visibility.is_some() {
        return Ok(filter);
    }

    Ok(SearchFilter {
        branch: SearchFilterBranch {
            visibility: Some(EnumFilterString {
                ne: Some(VISIBILITY_LOCKED.to_string()),
                ..Default::default()
            }),
            ..filter.branch
        },
        or: filter.or,
    })
}

enum VisibilityField {
    Visibility,
}

fn filter_branches(filter: &SearchFilter) -> Vec<&SearchFilterBranch> {
    let mut branches = vec![&filter.branch];
    if let Some(or) = &filter.or {
        branches.extend(or.iter());
    }
    branches
}

fn deciding_conditions(filter: &SearchFilter, field: VisibilityField) -> Vec<&EnumFilterString> {
    match field {
        VisibilityField::Visibility => {
            if filter.branch.visibility.is_some() {
                return vec![filter.branch.visibility.as_ref().unwrap()];
            }
            filter_branches(filter)
                .into_iter()
                .filter_map(|branch| branch.visibility.as_ref())
                .collect()
        }
    }
}

fn can_match_visibility(condition: &EnumFilterString, value: &str) -> bool {
    (condition.eq.is_none() || condition.eq.as_deref() == Some(value))
        && (condition.ne.is_none() || condition.ne.as_deref() != Some(value))
        && (condition.in_values.is_none() || condition.in_values.as_ref().is_some_and(|values| values.iter().any(|v| v == value)))
        && (condition.not_in.is_none() || condition.not_in.as_ref().is_some_and(|values| !values.iter().any(|v| v == value)))
}

pub use crate::models::dto::search::{collect_filter_ids, is_album_confined, is_fully_album_confined};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::db::users::AuthUserDb;

    fn auth_without_elevation() -> AuthDto {
        AuthDto {
            user: AuthUserDb {
                id: uuid::Uuid::new_v4(),
                email: "test@example.com".to_string(),
                name: "test".to_string(),
                is_admin: false,
                quota_usage_in_bytes: 0,
                quota_size_in_bytes: None,
            },
            session: None,
            shared_link: None,
            api_key: None,
        }
    }

    #[test]
    fn adds_locked_visibility_exclusion() {
        let filter = SearchFilter::default();
        let effective = apply_locked_visibility_policy(&auth_without_elevation(), filter).unwrap();
        assert_eq!(
            effective.branch.visibility.as_ref().and_then(|v| v.ne.as_deref()),
            Some(VISIBILITY_LOCKED)
        );
    }

    #[test]
    fn rejects_locked_visibility_without_elevation() {
        let filter = SearchFilter {
            branch: SearchFilterBranch {
                visibility: Some(EnumFilterString {
                    eq: Some(VISIBILITY_LOCKED.to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            or: None,
        };
        assert!(apply_locked_visibility_policy(&auth_without_elevation(), filter).is_err());
    }
}
