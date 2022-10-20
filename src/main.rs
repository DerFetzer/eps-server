mod config;
mod error;
mod image_handler;

use axum::{
    body::{Body, StreamBody},
    debug_handler,
    extract::{Path, State},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use clap::Parser;
use eyre::Result;
use hyper::header;
use mime::Mime;
use std::{net::SocketAddr, sync::Arc};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{config::Config, error::AppError, image_handler::ImageHandler};

struct AppState {
    image_handler: ImageHandler,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "eps_server=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // parse args
    let config = Config::parse();
    tracing::debug!("{config:?}");

    // run it
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app(config).into_make_service())
        .await
        .unwrap();
}

fn app(config: Config) -> Router<Arc<AppState>, Body> {
    let image_handler = ImageHandler::new(config);
    let state = Arc::new(AppState { image_handler });

    // build our application with a route
    Router::with_state(state)
        .route("/macs", get(get_macs))
        .route("/macs/:mac", delete(delete_images))
        .route("/macs/:mac/svg", get(get_svg))
        .route("/macs/:mac/render_svg", post(render_svg))
        .route("/macs/:mac/png", get(get_png))
        .layer(TraceLayer::new_for_http())
}

#[debug_handler]
async fn get_macs(state: State<Arc<AppState>>) -> Result<Json<Vec<String>>, AppError> {
    let mut macs = state.image_handler.get_macs().await?;
    macs.sort();
    Ok(Json(macs.iter().map(|mac| format!("{mac}")).collect()))
}

#[debug_handler]
async fn delete_images(
    Path(mac): Path<String>,
    state: State<Arc<AppState>>,
) -> Result<(), AppError> {
    let mac = mac.parse().map_err(AppError::BadRequest)?;
    state.image_handler.delete_images(mac).await
}

#[debug_handler]
async fn render_svg(
    Path(mac): Path<String>,
    state: State<Arc<AppState>>,
    body: String,
) -> Result<(), AppError> {
    let mac = mac.parse().map_err(AppError::BadRequest)?;
    state.image_handler.post_svg_body(mac, &body).await
}

#[debug_handler]
async fn get_svg(
    Path(mac): Path<String>,
    state: State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mac = mac.parse().map_err(AppError::BadRequest)?;
    let stream = state.image_handler.get_svg(mac).await?;
    Ok(stream_to_response(stream, mime::IMAGE_SVG))
}
#[debug_handler]
async fn get_png(
    Path(mac): Path<String>,
    state: State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mac = mac.parse().map_err(AppError::BadRequest)?;
    let stream = state.image_handler.get_png(mac).await?;
    Ok(stream_to_response(stream, mime::IMAGE_PNG))
}

fn stream_to_response(
    stream: ReaderStream<File>,
    content_type: Mime,
) -> impl IntoResponse + 'static {
    let body = StreamBody::new(stream);
    ([(header::CONTENT_TYPE, content_type.to_string())], body)
}

#[cfg(test)]
mod tests {
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use test_dir::{DirBuilder, FileType, TestDir};
    use tower::{Service, ServiceExt};

    use super::*;

    struct Fixture {
        config: Config,
        temp_dir: TestDir,
    }

    fn get_test_fixture() -> Fixture {
        let temp_dir = TestDir::temp()
            .create("0011223344556677.png", FileType::EmptyFile)
            .create("aabbccddeeffaabb.png", FileType::EmptyFile)
            .create("aabbccddeeffaabb.svg", FileType::EmptyFile);

        Fixture {
            config: Config {
                image_dir: temp_dir.path(""),
                epd_height: 296,
                epd_width: 128,
            },
            temp_dir,
        }
    }

    #[tokio::test]
    async fn get_macs() {
        let fix = get_test_fixture();
        let app = app(fix.config).into_service();

        let response = app
            .oneshot(Request::builder().uri("/macs").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(body, json!(["0011223344556677", "AABBCCDDEEFFAABB"]));
    }

    #[tokio::test]
    async fn delete_images() {
        let fix = get_test_fixture();
        let mut app = app(fix.config).into_service();

        let png_path = fix.temp_dir.path("0011223344556677.png");
        assert!(png_path.exists());

        let request = Request::builder()
            .uri("/macs/0011223344556677")
            .method("DELETE")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(!png_path.exists());

        let request = Request::builder()
            .uri("/macs/0011223344556677")
            .method("DELETE")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_svg() {
        let fix = get_test_fixture();
        let mut app = app(fix.config).into_service();

        let request = Request::builder()
            .uri("/macs/0011223344556677/svg")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let request = Request::builder()
            .uri("/macs/aabbccddeeffaabb/svg")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_png() {
        let fix = get_test_fixture();
        let mut app = app(fix.config).into_service();

        let request = Request::builder()
            .uri("/macs/0011223344556677/png")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .uri("/macs/1111111111111111/png")
            .body(Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn render_svg() {
        let fix = get_test_fixture();
        let app = app(fix.config).into_service();

        let png_path = fix.temp_dir.path("123456789abcdef1.png");
        let svg_path = fix.temp_dir.path("123456789abcdef1.svg");
        assert!(!png_path.exists());
        assert!(!svg_path.exists());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/macs/123456789abcdef1/render_svg")
                    .method("POST")
                    .body(Body::from("<circle cx=\"125\" cy=\"125\" r=\"75\" />"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(png_path.exists());
        assert!(svg_path.exists());
    }
}
