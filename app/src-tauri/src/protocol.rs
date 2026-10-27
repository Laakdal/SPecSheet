//! The read-only `ets2://` protocol serving converted game assets:
//!
//! * `/model?path=<pmd>&look=<look>&variant=<variant>[&parts=a,b]` → `ETSM` frame
//! * `/texture?path=<tobj>` → `ETST` frame
//!
//! Requests are answered on a worker thread. Only paths inside the mounted
//! archives can be reached; nothing on the host file system is served.

use std::borrow::Cow;
use std::sync::Arc;

use game_data::render::{build_mesh, load_texture};
use game_data::session::GameSession;
use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder};

use crate::state::AppState;
use crate::wire;

pub const SCHEME: &str = "ets2";

pub fn handle<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let game = ctx.app_handle().state::<AppState>().game();
    std::thread::spawn(move || {
        let response = match game {
            Ok(game) => serve(&game, &request),
            Err(error) => text(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        };
        responder.respond(response);
    });
}

fn serve(game: &Arc<GameSession>, request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let uri = request.uri();
    let query = uri.query().unwrap_or_default();
    let param = |name: &str| query_param(query, name);
    let Some(path) = param("path") else {
        return text(StatusCode::BAD_REQUEST, "missing path".to_owned());
    };
    let result = match uri.path() {
        "/model" => game
            .models()
            .load(game.fs(), &path)
            .map_err(|e| e.to_string())
            .and_then(|model| {
                let parts: Vec<String> = param("parts")
                    .map(|p| {
                        p.split(',')
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                let (mesh, _warnings) = build_mesh(
                    game.fs(),
                    &model,
                    param("look").as_deref(),
                    param("variant").as_deref(),
                    &parts,
                );
                wire::encode_model(&mesh).map_err(|e| e.to_string())
            }),
        "/texture" => load_texture(game.fs(), &path)
            .map_err(|e| e.to_string())
            .and_then(|texture| wire::encode_texture(&texture).map_err(|e| e.to_string())),
        other => return text(StatusCode::NOT_FOUND, format!("unknown endpoint {other}")),
    };
    match result {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/octet-stream")
            .header("Access-Control-Allow-Origin", "*")
            .body(Cow::Owned(bytes))
            .unwrap_or_else(|_| text(StatusCode::INTERNAL_SERVER_ERROR, "response".to_owned())),
        Err(message) => text(StatusCode::NOT_FOUND, message),
    }
}

fn text(status: StatusCode, message: String) -> Response<Cow<'static, [u8]>> {
    let mut response = Response::new(Cow::Owned(message.into_bytes()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        "Access-Control-Allow-Origin",
        tauri::http::HeaderValue::from_static("*"),
    );
    response
}

/// Value of query parameter `name`, percent-decoded.
fn query_param(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| percent_decode(value))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_query_parameters() {
        let query = "path=%2Fvehicle%2Ftruck%2Fa.pmd&look=default&variant=Default";
        assert_eq!(
            query_param(query, "path").as_deref(),
            Some("/vehicle/truck/a.pmd")
        );
        assert_eq!(query_param(query, "variant").as_deref(), Some("Default"));
        assert_eq!(query_param(query, "missing"), None);
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
