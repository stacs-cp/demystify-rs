
use std::net::SocketAddr;
use axum_session::{Session, SessionNullPool, SessionConfig, SessionStore, SessionLayer};
use axum::{
    routing::get, Json, Router
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use tower_http::cors::{Any, CorsLayer};


#[tokio::main]
async fn main() {
    let session_config = SessionConfig::default()
        .with_table_name("sessions_table");

    // create SessionStore and initiate the database tables
    let session_store = SessionStore::<SessionNullPool>::new(None, session_config).await.unwrap();

    let cors = CorsLayer::new().allow_origin(Any);

    // build our application with some routes
    let app = Router::new()
        .route("/greet", get(greet))
        .route("/greetX", get(greet_x))
        .layer(cors)
        .layer(SessionLayer::new(session_store));

    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));

    println!("listening on {}", addr);
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
    val: i32
}

async fn greet_x(session: Session<SessionNullPool>, Json(obj): Json<Obj>) -> Result<Json<Obj>, ()> {
    let mut count: i32 = session.get("count").unwrap_or(0);

    count += 1;
    session.set("count", count);

    let o : Obj = Obj{val: obj.val * 100 + count};

    Ok(Json(o))
}
