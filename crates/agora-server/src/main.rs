mod auth;
mod db;
mod models;
mod rate_limit;
mod routes;
mod validation;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post, put},
    Router,
};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use crate::db::Db;
use crate::rate_limit::RateLimiterState;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub rate_limiter: RateLimiterState,
}

#[tokio::main]
async fn main() {
    // Handle --help / --version
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("AGORA server");
        println!();
        println!("Usage: agora-server [command]");
        println!();
        println!("Commands:");
        println!("  (none)        Start the server");
        println!("  invite-code   Show unused invite codes from the database");
        println!("  --help        Show this help message");
        println!("  --version     Show version");
        println!();
        println!("Environment variables:");
        println!("  AGORA_NAME  Server name shown to users (default: none)");
        println!("  AGORA_URL   Server address used in download URLs on the landing page (default: <server-address>)");
        println!("  AGORA_DB    SQLite database path (default: agora.db)");
        println!("  AGORA_BIND  Listen address (default: 127.0.0.1:8080)");
        println!();
        println!("On first run, creates the database and prints a bootstrap invite code.");
        println!("To retrieve the code later: agora-server invite-code");
        println!();
        println!("Quick setup: sudo ./install-server.sh");
        println!("Full docs:   SERVER-GUIDE.md");
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("agora-server {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "invite-code") {
        // Show unused bootstrap invite code from the database
        let db_path = std::env::var("AGORA_DB").unwrap_or_else(|_| "agora.db".to_string());
        let conn = rusqlite::Connection::open(&db_path).expect("failed to open database");
        let mut stmt = conn
            .prepare("SELECT code FROM invite_codes WHERE used_by IS NULL ORDER BY id ASC")
            .expect("failed to query invite codes");
        let codes: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("failed to read invite codes")
            .filter_map(|r| r.ok())
            .collect();
        if codes.is_empty() {
            println!("No unused invite codes found.");
            println!("All invite codes have been used. Ask an existing user to run: agora invite");
        } else {
            println!("Unused invite codes:");
            for code in &codes {
                println!("  {}", code);
            }
        }
        return;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agora_server=info,tower_http=info".parse().unwrap()),
        )
        .init();

    let db_path = std::env::var("AGORA_DB").unwrap_or_else(|_| "agora.db".to_string());
    let bind_addr = std::env::var("AGORA_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    // Safety: refuse to touch the production database unless running as the agora user.
    // This prevents `cargo run` in a dev checkout from accidentally clobbering prod data.
    if db_path == "/var/lib/agora/forum.db" {
        let is_agora_user = std::env::var("USER")
            .map(|u| u == "agora")
            .unwrap_or(false);
        if !is_agora_user {
            error!(
                "Refusing to use production database ({}) as non-agora user. \
                 Set AGORA_DB to a different path for development.",
                db_path
            );
            std::process::exit(1);
        }
    }

    let database = db::open(&db_path);
    db::migrate(&database);

    if let Some(code) = db::seed(&database) {
        info!("BOOTSTRAP INVITE CODE: {}", code);
    }

    let limiter = rate_limit::RateLimiter::new();
    rate_limit::spawn_cleanup_task(limiter.clone());

    let state = AppState {
        db: database,
        rate_limiter: limiter,
    };

    // Public routes
    let public = Router::new()
        .route("/", get(routes::landing))
        .route("/version", get(routes::version))
        .route("/register", post(routes::register))
        .route("/download/:filename", get(routes::download));

    // Authenticated routes
    let authed = Router::new()
        .route("/boards", get(routes::list_boards))
        .route("/boards/:slug", get(routes::list_threads))
        .route(
            "/boards/:slug/threads",
            post(routes::create_thread),
        )
        .route("/threads/:id", get(routes::get_thread))
        .route("/threads/:id/posts", post(routes::create_post))
        .route(
            "/threads/:thread_id/posts/:post_id",
            put(routes::edit_post),
        )
        .route(
            "/threads/:thread_id/posts/:post_id/history",
            get(routes::post_history),
        )
        // Moderation
        .route("/threads/:id/mod", post(routes::mod_thread))
        .route(
            "/threads/:thread_id/posts/:post_id/mod",
            post(routes::mod_post),
        )
        .route("/users/:username/mod", post(routes::mod_user))
        // Bookmarks
        .route("/bookmarks", get(routes::list_bookmarks))
        .route(
            "/bookmarks/:thread_id",
            post(routes::toggle_bookmark),
        )
        // Attachments
        .route(
            "/threads/:thread_id/posts/:post_id/attachments",
            post(routes::upload_attachment),
        )
        .route(
            "/attachments/:id",
            get(routes::download_attachment),
        )
        // Reactions
        .route(
            "/threads/:thread_id/posts/:post_id/react",
            post(routes::react_post),
        )
        // Bio
        .route("/me/bio", put(routes::update_bio))
        // Mentions
        .route("/mentions", get(routes::get_mentions))
        // Invites
        .route("/invites", get(routes::list_invites).post(routes::create_invite))
        .route("/me", get(routes::me))
        .route("/users", get(routes::list_users))
        .route("/users/:username/key", get(routes::get_user_public_key))
        .route("/search", get(routes::search))
        .route("/dm", get(routes::dm_inbox).post(routes::send_dm))
        .route("/dm/:username", get(routes::dm_conversation))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    let app = Router::new()
        .merge(public)
        .merge(authed)
        .layer(TraceLayer::new_for_http())
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-content-type-options"),
            axum::http::HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-frame-options"),
            axum::http::HeaderValue::from_static("DENY"),
        ))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024)) // 8 MB max request body
        .with_state(state);

    info!("AGORA server listening on {}", bind_addr);

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Could not listen on {}: {}", bind_addr, e);
            if e.kind() == std::io::ErrorKind::AddrInUse {
                error!(
                    "Port already in use. If the production server is running, \
                     use a different port for development: AGORA_BIND=127.0.0.1:9090 cargo run"
                );
            }
            std::process::exit(1);
        }
    };

    // Limit concurrent connections to prevent resource exhaustion
    let app = tower::ServiceBuilder::new()
        .concurrency_limit(256)
        .service(app.into_make_service());

    axum::serve(listener, app).await.expect("server error");
}
