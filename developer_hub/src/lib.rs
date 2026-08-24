use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "./"]
#[exclude = "*.rs"]
#[exclude = "Cargo.toml"]
struct Asset;

#[derive(RustEmbed)]
#[folder = "../docs/api/"]
struct DocsAsset;

pub fn devhub_routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/", get(index_handler))
        .route("/index.html", get(index_handler))
        .route("/{*file}", get(static_handler))
}

async fn index_handler() -> impl IntoResponse {
    static_handler("/index.html".parse::<Uri>().unwrap()).await
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        path = "index.html";
    }

    // 1. If path points to documentation (/docs/...), serve from DocsAsset
    if path.starts_with("docs/") {
        let doc_subpath = path.trim_start_matches("docs/").trim_start_matches('/');
        if let Some(content) = DocsAsset::get(doc_subpath) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let content_type = if path.ends_with(".md") {
                "text/plain; charset=utf-8"
            } else {
                mime.as_ref()
            };
            return Response::builder()
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(content.data))
                .unwrap();
        }
    }

    // 2. Check main Developer Hub assets
    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // 3. Check DocsAsset as fallback
            if let Some(content) = DocsAsset::get(path) {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                return Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(Body::from(content.data))
                    .unwrap();
            }

            // 4. SPA Fallback: serve index.html for any unknown UI route
            if let Some(content) = Asset::get("index.html") {
                let mime = mime_guess::from_path("index.html").first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(Body::from(content.data))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("404 Not Found"))
                    .unwrap()
            }
        }
    }
}
