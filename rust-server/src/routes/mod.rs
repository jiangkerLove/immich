pub mod album;
pub mod api_key;
pub mod shared_link;
pub mod asset;
pub mod asset_media;
pub mod auth;
pub mod memory;
pub mod notification;
pub mod oauth;
pub mod server;
pub mod session;
pub mod system_metadata;
pub mod stubs;
pub mod tag;
pub mod search;
pub mod trash;
pub mod timeline;
pub mod user;

use axum::Router;

use crate::app_state::AppState;

pub fn public_router() -> Router<AppState> {
    Router::new()
        .merge(auth::public_router())
        .merge(oauth::public_router())
        .merge(server::public_router())
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .merge(auth::protected_router())
        .merge(oauth::protected_router())
        .merge(server::protected_router())
        .merge(user::router())
        .merge(session::router())
        .merge(api_key::router())
        .merge(album::router())
        .merge(tag::router())
        .merge(asset::router())
        .merge(shared_link::router())
        .merge(asset_media::router())
        .merge(timeline::router())
        .merge(trash::router())
        .merge(search::router())
        .merge(system_metadata::router())
        .merge(memory::router())
        .merge(notification::router())
        .merge(stubs::router())
}
