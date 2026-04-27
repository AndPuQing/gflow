use axum::{
    body::Body,
    extract::Path,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use rust_embed::{EmbeddedFile, RustEmbed};

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

pub(super) async fn serve_index() -> Response {
    serve_embedded_file("index.html", false)
}

pub(super) async fn serve_asset(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return serve_index().await;
    }

    match WebAssets::get(path) {
        Some(file) => embedded_response(path, file, path.starts_with("assets/")),
        None => serve_embedded_file("index.html", false),
    }
}

fn serve_embedded_file(path: &str, cache_forever: bool) -> Response {
    match WebAssets::get(path) {
        Some(file) => embedded_response(path, file, cache_forever),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "web UI has not been built; run `bun run --cwd web build` before building gflowd",
        )
            .into_response(),
    }
}

fn embedded_response(path: &str, file: EmbeddedFile, cache_forever: bool) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache_control = if cache_forever {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    let mut response = Response::new(Body::from(file.data.into_owned()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).expect("MIME values are valid header values"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    response
}
