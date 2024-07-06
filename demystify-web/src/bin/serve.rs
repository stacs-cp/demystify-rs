use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use axum::routing::post;
use axum::{routing::get, Json, Router};
use axum_session::{Session, SessionConfig, SessionLayer, SessionNullPool, SessionStore};
use demystify_web::wrap;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;

use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() {
    let session_config = SessionConfig::default().with_table_name("sessions_table");

    // create SessionStore and initiate the database tables
    let session_store = SessionStore::<SessionNullPool>::new(None, session_config)
        .await
        .unwrap();

    let cors = CorsLayer::new().allow_origin(Any);

    macro_rules! serve_static_file {
        ($path:expr) => {
            get(move |_: Request<Body>| async {
                let file_content: &'static str =
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), $path));
                Ok::<_, Infallible>(Response::new(Body::from(file_content)))
            })
        };
    }

    // build our application with some routes
    let app = Router::new()
        .route("/greet", get(greet))
        .route("/greetX", get(greet_x))
        .route("/uploadPuzzle", post(wrap::upload_files))
        .route("/quickFullSolve", post(wrap::dump_full_solve))
        .route("/bestNextStep", post(wrap::best_next_step))
        .route("/clickLiteral", post(wrap::click_literal))
        .route(
            "/ext/htmx.js",
            serve_static_file!("/html/website/ext/htmx.js"),
        )
        .route(
            "/ext/bootstrap.min.css",
            serve_static_file!("/html/website/ext/bootstrap.min.css"),
        )
        .route(
            "/ext/bootstrap.bundle.min.js",
            serve_static_file!("/html/website/ext/bootstrap.bundle.min.js"),
        )
        .route(
            "/ext/response-targets.js",
            serve_static_file!("/html/website/ext/response-targets.js"),
        )
        .route("/", serve_static_file!("/html/website/index.html"))
        .route(
            "/base/base.css",
            get(move |_: Request<Body>| async {
                Ok::<_, Infallible>(Response::new(Body::from(demystify_lib::web::base_css())))
            }),
        )
        .route(
            "/base/base.js",
            get(move |_: Request<Body>| async {
                Ok::<_, Infallible>(Response::new(Body::from(
                    demystify_lib::web::base_javascript(),
                )))
            }),
        )
        .layer(cors)
        .layer(SessionLayer::new(session_store));

    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], 8008));

    println!("listening on {addr}");
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn greet(session: Session<SessionNullPool>) -> String {
    let mut count: usize = session.get("count").unwrap_or(0);

    count += 1;
    session.set("count", count);

    count.to_string()
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]

struct Obj {
    val: i32,
}

async fn greet_x(session: Session<SessionNullPool>, Json(obj): Json<Obj>) -> Result<Json<Obj>, ()> {
    let mut count: i32 = session.get("count").unwrap_or(0);

    count += 1;
    session.set("count", count);

    let o: Obj = Obj {
        val: obj.val * 100 + count,
    };

    Ok(Json(o))
}
