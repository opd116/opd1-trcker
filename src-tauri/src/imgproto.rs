//! The `img` URI scheme (R11/R21): `img://localhost/<kind>/<id>` (on
//! Windows/WebView2: `http://img.localhost/<kind>/<id>`) → the disk-cached
//! valorant-api image, downloaded once on first request.
//!
//! The id is validated twice before it can touch a path or URL: here
//! (shape: a single path segment, no `%`/`/`/`..`) and again by
//! `AssetCache::ensure_image` (uuid shape / tier range). Anything invalid
//! is a 404 — nothing remote is ever requested for it.

use std::sync::Arc;

use tauri::http::{header, Response, StatusCode};

use opd1_riot::assets::{AssetError, AssetKind};

use crate::logbuf::short;
use crate::state::AppState;

/// Cached images never change for a given id (a new skin/card/map gets a
/// new uuid), so the webview may keep them for a year.
const CACHE_FOREVER: &str = "public, max-age=31536000, immutable";

/// `/<kind>/<id>` → the asset it names; `None` for anything else.
pub fn parse_path(path: &str) -> Option<(AssetKind, &str)> {
    let rest = path.strip_prefix('/')?;
    let (kind, id) = rest.split_once('/')?;
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    let kind = match kind {
        "agent" => AssetKind::Agent,
        "cardSmall" => AssetKind::CardSmall,
        "cardWide" => AssetKind::CardWide,
        "cardLarge" => AssetKind::CardLarge,
        "tier" => AssetKind::Tier,
        "mapList" => AssetKind::MapList,
        "mapSplash" => AssetKind::MapSplash,
        "chroma" => AssetKind::Chroma,
        "contentTier" => AssetKind::ContentTier,
        // R189: the Skins screen.
        "weaponIcon" => AssetKind::WeaponIcon,
        "skinLevel" => AssetKind::SkinLevel,
        _ => return None,
    };
    Some((kind, id))
}

/// The part of a request URI the handler reads: the path only. The UI adds
/// `?r=n` to a retried image URL (R163) to get past the webview's cache; the
/// query never reaches [`parse_path`], so a retry is the same asset.
pub fn request_path(uri: &tauri::http::Uri) -> String {
    uri.path().to_string()
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Vec::new())
        .unwrap_or_default()
}

/// Serves one `img` request (runs on the async runtime; the protocol
/// handler hands the response to the webview's responder).
pub async fn serve(st: Arc<AppState>, method: String, path: String) -> Response<Vec<u8>> {
    if method != "GET" {
        return empty(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Some((kind, id)) = parse_path(&path) else {
        return empty(StatusCode::NOT_FOUND);
    };

    let mut result = st.assets.ensure_image(kind, id).await;
    if matches!(result, Err(AssetError::TablesNotLoaded)) && st.ensure_tables(false).await.is_ok() {
        result = st.assets.ensure_image(kind, id).await;
    }
    let file = match result {
        Ok(file) => file,
        Err(AssetError::NotFound) => return empty(StatusCode::NOT_FOUND),
        Err(e) => {
            log::warn!("img {kind:?}/{} failed: {e}", short(id));
            return empty(StatusCode::SERVICE_UNAVAILABLE);
        }
    };
    let bytes = match tauri::async_runtime::spawn_blocking(move || std::fs::read(file)).await {
        Ok(Ok(bytes)) => bytes,
        _ => return empty(StatusCode::SERVICE_UNAVAILABLE),
    };
    Response::builder()
        .status(StatusCode::OK)
        // From the bytes: AssetCache re-encodes a downscaled image to PNG but
        // keeps one that needs no downscale as valorant-api served it.
        .header(header::CONTENT_TYPE, image_type(&bytes))
        .header(header::CACHE_CONTROL, CACHE_FOREVER)
        .body(bytes)
        .unwrap_or_else(|_| empty(StatusCode::INTERNAL_SERVER_ERROR))
}

/// The image's media type from its first bytes (PNG when unknown).
fn image_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else {
        "image/png"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_media_type_follows_the_bytes() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\n...."), "image/png");
        assert_eq!(image_type(b"\xFF\xD8\xFF\xE0...."), "image/jpeg");
        assert_eq!(image_type(b"RIFF\x00\x00\x00\x00WEBPVP8 "), "image/webp");
        assert_eq!(image_type(b"GIF89a"), "image/gif");
        assert_eq!(image_type(b""), "image/png");
    }

    const UUID: &str = "e370fa57-4757-3604-3648-499e1f642d3f";

    #[test]
    fn parses_every_kind() {
        for (name, kind) in [
            ("agent", AssetKind::Agent),
            ("cardSmall", AssetKind::CardSmall),
            ("cardWide", AssetKind::CardWide),
            ("cardLarge", AssetKind::CardLarge),
            ("mapList", AssetKind::MapList),
            ("mapSplash", AssetKind::MapSplash),
            ("chroma", AssetKind::Chroma),
            ("contentTier", AssetKind::ContentTier),
            ("weaponIcon", AssetKind::WeaponIcon),
            ("skinLevel", AssetKind::SkinLevel),
        ] {
            assert_eq!(parse_path(&format!("/{name}/{UUID}")), Some((kind, UUID)));
        }
        assert_eq!(parse_path("/tier/21"), Some((AssetKind::Tier, "21")));
    }

    #[test]
    fn rejects_unknown_kinds_and_malformed_paths() {
        for bad in [
            "",
            "/",
            "/agent",
            "/agent/",
            "agent/x",
            "/weapon/abc",
            "/weaponicon/abc",
            "/SkinLevel/abc",
            "/Agent/abc",
            "/agent/../../etc/passwd",
            "/agent/a/b",
            "/agent/%2e%2e",
            "/tier/21.png",
            "/agent/ spaced",
        ] {
            assert_eq!(parse_path(bad), None, "{bad:?} must be rejected");
        }
        let long = format!("/agent/{}", "a".repeat(65));
        assert_eq!(parse_path(&long), None);
    }

    #[test]
    fn a_retry_query_is_ignored() {
        // R163: the UI retries a failed image as `<url>?r=n`, on both schemes.
        for (uri, want) in [
            (
                format!("img://localhost/agent/{UUID}?r=1"),
                format!("/agent/{UUID}"),
            ),
            (
                format!("http://img.localhost/agent/{UUID}?r=3"),
                format!("/agent/{UUID}"),
            ),
            (
                "img://localhost/tier/21?r=2".to_string(),
                "/tier/21".to_string(),
            ),
        ] {
            let uri: tauri::http::Uri = uri.parse().unwrap();
            let path = request_path(&uri);
            assert_eq!(path, want);
            assert!(
                parse_path(&path).is_some(),
                "{path} is served like the plain URL"
            );
        }
    }
}
