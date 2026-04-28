use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use axum::routing::{get, post};
use axum_session::{SessionConfig, SessionLayer, SessionNullPool, SessionStore};
use demystify_web::util::AppState;
use demystify_web::wrap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};

use demystify::problem::util::exec::ProgramRunner;

fn build_templates() -> tera::Tera {
    let mut tera = tera::Tera::default();
    tera.add_raw_templates(vec![
        (
            "layout.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/layout.html"
            )),
        ),
        (
            "landing.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/landing.html"
            )),
        ),
        (
            "solver.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/solver.html"
            )),
        ),
        (
            "no_puzzle.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/no_puzzle.html"
            )),
        ),
        (
            "partials/solver_stage.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/partials/solver_stage.html"
            )),
        ),
        (
            "partials/uploader.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/partials/uploader.html"
            )),
        ),
        (
            "partials/param_editor.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/partials/param_editor.html"
            )),
        ),
        (
            "solvetree.html",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/templates/solvetree.html"
            )),
        ),
    ])
    .expect("Failed to load templates");

    tera.autoescape_on(vec![]);
    tera
}

#[tokio::main]
async fn main() {
    println!("Finding conjure...");
    let conjure_version = ProgramRunner::get_conjure_version();

    match conjure_version {
        Ok(s) => println!("{s}"),
        Err(s) => {
            eprintln!("ERROR: Could not find 'conjure': {s}");
            eprintln!("Install conjure from https://github.com/conjure-cp/conjure/releases");
            eprintln!("or pass --conjure Docker / --conjure Podman to use a container image.");
            std::process::exit(1);
        }
    };

    let session_config = SessionConfig::default().with_table_name("sessions_table");
    let session_store = SessionStore::<SessionNullPool>::new(None, session_config)
        .await
        .unwrap();

    let cors = CorsLayer::new().allow_origin(Any);

    let tera = build_templates();
    let app_state = AppState {
        tera: Arc::new(tera),
    };

    macro_rules! serve_static {
        ($path:expr, $content_type:expr) => {
            get(move |_: Request<Body>| async {
                let content: &'static str =
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), $path));
                Ok::<_, Infallible>(
                    Response::builder()
                        .header("Content-Type", $content_type)
                        .body(Body::from(content))
                        .unwrap(),
                )
            })
        };
    }

    let app = Router::new()
        // Page routes
        .route("/", get(wrap::landing))
        .route("/solver", get(wrap::solver_page))
        // htmx partial routes
        .route("/solver/advance", post(wrap::solver_advance))
        .route("/solver/reset", post(wrap::solver_reset))
        .route("/solver/difficulties", post(wrap::solver_difficulties))
        .route("/solver/explain", post(wrap::solver_explain))
        // Explore mode
        .route("/solver/explore/toggle", post(wrap::solver_explore_toggle))
        .route(
            "/solver/explore/navigate",
            post(wrap::solver_explore_navigate),
        )
        // Upload / example loading
        .route("/upload", post(wrap::upload_files))
        .route("/previewExample", post(wrap::preview_example))
        .route("/submitExample", post(wrap::submit_example))
        .route("/loadExample", post(wrap::load_example_and_redirect))
        // Solve tree
        .route("/solvetree", get(wrap::solvetree_page))
        .route("/solvetree/build", post(wrap::solvetree_build))
        // Legacy JSON endpoint
        .route("/quickFullSolve", post(wrap::dump_full_solve))
        // Static assets
        .route(
            "/static/demystify.css",
            serve_static!("/static/demystify.css", "text/css; charset=utf-8"),
        )
        .route(
            "/static/demystify.js",
            serve_static!(
                "/static/demystify.js",
                "application/javascript; charset=utf-8"
            ),
        )
        // Legacy static assets (base.css/base.js from demystify core, for CLI compatibility)
        .route(
            "/base/base.css",
            get(move |_: Request<Body>| async {
                Ok::<_, Infallible>(
                    Response::builder()
                        .header("Content-Type", "text/css")
                        .body(Body::from(demystify::web::base_css()))
                        .unwrap(),
                )
            }),
        )
        .route(
            "/base/base.js",
            get(move |_: Request<Body>| async {
                Ok::<_, Infallible>(
                    Response::builder()
                        .header("Content-Type", "application/javascript")
                        .body(Body::from(demystify::web::base_javascript()))
                        .unwrap(),
                )
            }),
        )
        .with_state(app_state)
        .layer(cors)
        .layer(SessionLayer::new(session_store));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8008));
    eprintln!("listening on {addr}");
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
