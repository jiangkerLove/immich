use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use axum::http;

use crate::app_state::AppState;
use crate::models::response::response::{handler_err, ErrorResp};
use crate::utils::cookie::{parse_immich_cookies, ImmichCookie};

pub async fn auth(State(app_state): State<AppState>, mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    match path {
        "/api/auth/register" => {
            Ok(next.run(req).await)
        }
        "/api/auth/login" => {
            Ok(next.run(req).await)
        }
        _ => {
            let cookie_header = req.headers()
                .get(http::header::COOKIE)
                .and_then(|header| header.to_str().ok());
            match cookie_header {
                None => {
                    Ok(next.run(req).await)
                }
                Some(cookie) => {
                    let cookies = parse_immich_cookies(cookie);

                    if let Some(token) = cookies.get(&ImmichCookie::AccessToken) {
                        let auth_dto_opt = app_state.auth_service.validate_session(&token).await.map_err(ErrorResp::from);
                        match auth_dto_opt {
                            Ok(auth_req) => {
                                req.extensions_mut().insert(auth_req);
                                Ok(next.run(req).await)
                            }
                            Err(err) => {
                                Ok(handler_err(err))
                            }
                        }
                    } else {
                        Ok(handler_err(ErrorResp::Unauthorized(String::from("Authentication required"))))
                    }
                }
            }
        }
    }
}
