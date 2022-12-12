pub mod models;

use axum::{
    extract::{Extension, Query},
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
    routing::get,
    routing::post,
    Form, Router,
};
use chrono::{prelude::*, Duration};
use chrono_tz::Tz;
use clap::Parser;
use elasticsearch::{http::transport::Transport, Elasticsearch, SearchParts};
use geodate::moon_transit::get_moonrise;
use icalendar::Component;
use serde_json::{json, Value};
use std::{
    error::Error,
    net::{IpAddr, Ipv6Addr, SocketAddr},
};
use std::{str::FromStr, sync::Arc};
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub use models::*;

// Setup the command line interface with clap.
#[derive(Parser, Debug)]
#[clap(name = "server", about = "A server for our wasm project!")]
struct Opt {
    /// set the log level
    #[clap(short = 'l', long = "log", default_value = "debug")]
    log_level: String,

    /// set the elaticsearch addr
    #[clap(
        short = 'e',
        long = "elasticsearch",
        default_value = "http://localhost:9200"
    )]
    es: String,

    /// set the listen addr
    #[clap(short = 'a', long = "addr", default_value = "::1")]
    addr: String,

    /// set the listen port
    #[clap(short = 'p', long = "port", default_value = "8080")]
    port: u16,
}

#[tokio::main]
async fn main() {
    let opt = Opt::parse();

    // Setup logging & RUST_LOG from args
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", format!("{},hyper=info,mio=info", opt.log_level))
    }

    // enable console logging
    tracing_subscriber::fmt::init();

    let shared_state = Arc::new(DBConnections { es: opt.es });

    let app = Router::new()
        .route("/ical", post(generate_calendar))
        .route("/search_location", get(search_locations))
        .route("/robots.txt", get(robots))
        .layer(Extension(shared_state))
        .layer(CorsLayer::new().allow_origin(Any))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    let sock_addr = SocketAddr::from((
        IpAddr::from_str(opt.addr.as_str()).unwrap_or(IpAddr::V6(Ipv6Addr::LOCALHOST)),
        opt.port,
    ));

    log::info!("listening on http://{}", sock_addr);

    axum::Server::bind(&sock_addr)
        .serve(app.into_make_service())
        .await
        .expect("Unable to start server");
}

async fn generate_calendar(Form(payload): Form<CreateCalendar>) -> impl IntoResponse {
    // add input validation
    let mut calendar = icalendar::Calendar::new();
    let moonrises = generate_moonrises(payload.lat, payload.lon, payload.number_of_days);

    let tz: Tz = payload
        .clone()
        .timezone
        .unwrap_or_else(|| "UTC".to_string())
        .parse()
        .unwrap();
    for moonrise in moonrises {
        let moonrise_date =
            DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp_opt(moonrise, 0).unwrap(), Utc);
        let start = moonrise_date - Duration::minutes(payload.before as i64);
        let end = moonrise_date + Duration::minutes(payload.after as i64);

        let event = icalendar::Event::new()
            .summary(
                &payload
                    .clone()
                    .summary
                    .unwrap_or_else(|| "Moonrise".to_string()),
            )
            .description(format!("Moonrise @ {}", moonrise_date.with_timezone(&tz)).as_str())
            .starts(start)
            .ends(end)
            .done();

        calendar.push(event);
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"moonrises.ical\""),
    );

    let content = calendar.to_string();
    (headers, content)
}

fn unix_to_julian(timestamp: i64) -> f64 {
    (timestamp as f64 / 86400.0) + 2440587.5
}

fn julian_to_unix(jd: f64) -> i64 {
    ((jd - 2440587.5) * 86400.0).round() as i64
}

fn generate_moonrises(lat: f64, lon: f64, number_of_days: usize) -> Vec<i64> {
    let local: DateTime<Utc> = Utc::now();
    let mut moonrises = Vec::with_capacity(number_of_days);
    let mut previous_moonrise = 0;
    for i in 0..number_of_days {
        let l = local + Duration::days(i as i64);
        let jd = (unix_to_julian(l.timestamp()) + lon / 360.0 + 0.5).floor() - 0.5;
        let mut next_moonrise = get_moonrise(julian_to_unix(jd), lon, lat);

        // Check to see if there is an issue with generating moonrises too close to each other
        // This might have to do with daylight savings times, not sure
        if next_moonrise.is_some() && next_moonrise.unwrap() - previous_moonrise <= 500 {
            next_moonrise = get_moonrise(julian_to_unix(jd + 1.), lon, lat);
        }

        if let Some(moonrise) = next_moonrise {
            previous_moonrise = moonrise;
            moonrises.push(moonrise);
        } else {
            log::info!("No moonrise for {}", l);
        }
    }
    log::info!("{:?}", &moonrises);

    moonrises
}

// Really this is get population centers, until we can differentiate better on the data
async fn get_locations(
    client: elasticsearch::Elasticsearch,
    query: String,
) -> Result<Vec<LocationResponse>, Box<dyn Error>> {
    let response = client
        .search(SearchParts::Index(&["geolocations"]))
        .body(json!({"query":
        { "bool": {
            "must": {
                "multi_match": {
                    "fields": ["name", "country_code"],
                    "query": query,
                    "fuzziness": "AUTO"
                }
            },
            "filter": {
                "range": { "population": { "gt": 0}}
            } }
        }}))
        .send()
        .await?;

    let body = response.json::<Value>().await?;
    let mut data: Vec<LocationResponse> = Vec::new();
    for hit in body["hits"]["hits"].as_array().unwrap() {
        data.push(LocationResponse::from_source_with_id(
            hit["_id"].as_str().unwrap(),
            hit["_source"].clone(),
        ));
    }

    Ok(data)
}

async fn search_locations(
    search_query: Query<SearchQuery>,
    Extension(state): Extension<Arc<DBConnections>>,
) -> impl IntoResponse {
    let query = search_query.0.query;

    let client = Elasticsearch::new(Transport::single_node(&state.es).unwrap());
    let results = get_locations(client, query).await.unwrap();

    println!("{} results", results.len());
    let body = serde_json::to_string(&results).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );

    (headers, body)
}

async fn robots() -> &'static str {
    "User-Agent: *\nDisallow: /"
}
