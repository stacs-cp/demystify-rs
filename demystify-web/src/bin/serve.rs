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

    // build our application with some routes
    let app = Router::new()
        .route("/greet", get(greet))
        .route("/greetX", get(greet_x))
        .route("/uploadPuzzle", post(wrap::upload_files))
        .route("/quickFullSolve", get(wrap::dump_full_solve))
        .route(
            "/htmx.js",
            get(|_: Request<Body>| async {
                let htmx: &'static str =
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/html/website/htmx.js"));
                Ok::<_, Infallible>(Response::new(Body::from(htmx)))
            }),
        )
        .route(
            "/",
            get(|_: Request<Body>| async {
                let index: &'static str = include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/html/website/index.html"
                ));
                Ok::<_, Infallible>(Response::new(Body::from(index)))
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
