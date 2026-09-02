pub mod activity;
pub mod map;
pub mod face;
pub mod view;
pub mod download;
pub mod album;
pub mod api_key;
pub mod shared_link;
pub mod asset;
pub mod asset_file;
pub mod asset_media;
pub mod auth;
pub mod memory;
pub mod notification;
pub mod oauth;
pub mod server;
pub mod session;
pub mod system_metadata;
pub mod person;
pub mod partner;
pub mod stack;
pub mod static_web;
pub mod maintenance_worker;
pub mod sync;
pub mod tag;
pub mod search;
pub mod trash;
pub mod timeline;
pub mod config;
pub mod duplicate;
pub mod system_config;
pub mod user;
pub mod user_admin;
pub mod maintenance;
pub mod video_stream;
pub mod job;
pub mod queue;
pub mod library;
pub mod integrity;
pub mod database_backup;
pub mod plugin;
pub mod workflow;

use axum::Router;

use crate::app_state::AppState;

pub fn public_router() -> Router<AppState> {
    Router::new()
        .merge(auth::public_router())
        .merge(oauth::public_router())
        .merge(server::public_router())
        .merge(config::public_router())
        .merge(maintenance::public_router())
        .merge(database_backup::public_router())
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
        .merge(asset_file::router())
        .merge(shared_link::router())
        .merge(asset_media::router())
        .merge(timeline::router())
        .merge(trash::router())
        .merge(search::router())
        .merge(system_metadata::router())
        .merge(memory::router())
        .merge(notification::router())
        .merge(sync::router())
        .merge(partner::router())
        .merge(stack::router())
        .merge(person::router())
        .merge(activity::router())
        .merge(map::router())
        .merge(download::router())
        .merge(view::router())
        .merge(face::router())
        .merge(user_admin::router())
        .merge(duplicate::router())
        .merge(system_config::router())
        .merge(config::protected_router())
        .merge(maintenance::protected_router())
        .merge(video_stream::router())
        .merge(job::router())
        .merge(queue::router())
        .merge(library::router())
        .merge(integrity::router())
        .merge(database_backup::protected_router())
        .merge(plugin::router())
        .merge(workflow::router())
}
