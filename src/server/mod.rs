pub mod stats;
pub mod app_data;

use actix_cors::Cors;
use actix_http::{body::Body, Request, Error};
use actix_rt;
use actix_service::Service;
use actix_web::{
    App, http, HttpServer, test,
    middleware::Logger,
    dev::ServiceResponse,
};
use dotenv::dotenv;
use log::{info, error};
use serde::Serialize;

#[cfg(not(feature = "swagger"))]
use actix_web::web;

#[cfg(feature = "swagger")]
use paperclip::actix::{web, OpenApiExt};

use super::threads;

#[cfg(feature = "pgsql")]
use super::db_pool;

use super::logger;
use stats::{BaseStats, StatsWrapper, StatsPresenter, default_healthcheck_handler, default_readiness_handler, default_stats_handler};

pub use app_data::{DefaultAppData, default_app_data};

fn default_cors_factory() -> Cors {
    Cors::new()
        .send_wildcard()
        .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
        .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT, http::header::CONTENT_TYPE])
        .max_age(3600)
}

pub fn start<D, T, F>
(
    name: &str,
    prepare_app_data: impl Fn() -> T,
    configure_app: F,
    app_port: &str,
)
where
    D: Serialize + 'static,
    T: StatsPresenter<D> + 'static + Clone + Send,
    F: Fn(&mut web::ServiceConfig) + Send + Clone + Copy + 'static,
{
    start_with_cors(
        name,
        prepare_app_data,
        configure_app,
        app_port,
        default_cors_factory
    )
}

pub fn start_with_cors<D, T, F, C>
(
    name: &str,
    prepare_app_data: impl Fn() -> T,
    configure_app: F,
    app_port: &str,
    cors_factory: C,
)
where
    D: Serialize + 'static,
    T: StatsPresenter<D> + 'static + Clone + Send,
    F: Fn(&mut web::ServiceConfig) + Send + Clone + Copy + 'static,
    C: Fn() -> Cors + Send + Clone + 'static,
{
    dotenv().ok();

    match logger::init_logger() {
        Ok(_) => info!("Logger has been initialized"),
        Err(_) => error!("Error logger initialization")
    };

    //env::set_var("RUST_LOG", "actix_web=debug");

    let numthreads = threads::num_threads();
    info!("Configuring for {} threads", numthreads);

    info!("Creating actix event loop");
    let sys = actix_rt::System::new(name);

    let app_data = prepare_app_data();

    let stats = BaseStats::default();

    info!("Starting HTTP server");
    #[allow(clippy::let_and_return)]
    HttpServer::new(move || {
        let app = App::new();

        #[cfg(feature = "swagger")]
        let app = app.wrap_api()
            .with_json_spec_at("/spec");

        let app = app
            .data(app_data.clone())
            .data(stats.clone())
            .configure(configure_app)
            .route("_healthcheck", web::get().to(default_healthcheck_handler))
            .route("_ready", web::get().to_async(default_readiness_handler::<T, D>))
            .route("_stats", web::get().to_async(default_stats_handler::<T, D>))
            .wrap(Logger::default())
            .wrap(StatsWrapper::default())
            .wrap(cors_factory());

        #[cfg(feature = "swagger")]
        let app = app.build();

        app
    })
    .workers(numthreads)
    .bind(format!("0.0.0.0:{}", app_port))
    .expect("Can't bind")
    .start();

    info!("Activating actix event loop");
    let _ = sys.run();
}

pub fn test_init<T, F>(prepare_app_data: impl Fn() -> T, configure_app: F) -> impl Service<Request = Request, Response = ServiceResponse<Body>, Error = Error>
where
    T: 'static,
    F: Fn(&mut web::ServiceConfig),
{
    let app_data = test::run_on(|| prepare_app_data());

    #[allow(clippy::let_and_return)]
    test::init_service({
        let app = App::new();

        #[cfg(feature = "swagger")]
        let app = app.wrap_api();

        let app = app
            .data(app_data)
            .configure(configure_app);

        #[cfg(feature = "swagger")]
        let app = app.build();

        app
    })
}
