use axum::extract::{Query, State};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::memory::MemoryResponse;
use crate::models::response::response::ErrorResp;
use crate::service::memory::MemorySearchQuery;

pub async fn search_memories_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<MemorySearchQuery>,
) -> Result<Json<Vec<MemoryResponse>>, ErrorResp> {
    Ok(Json(
        state.services.memory.search(&auth, &query).await?,
    ))
}
