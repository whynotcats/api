use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Location {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationResponse {
    pub id: String,
    pub name: String,
    pub ascii_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub feature_code: String,
    pub country_code: String,
    pub admin1: Option<String>,
    pub admin2: Option<String>,
    pub feature_class: Option<FeatureClass>,
    pub population: Option<i64>,
    pub elevation: Option<i64>,
    pub timezone: String,
    pub modification_date: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FeatureClass {
    City = 0,
    Area,
    WaterBody,
    Region,
    Road,
    Spot,
    Hill,
    Undersea,
    Forest,
}

impl FeatureClass {
    fn from_str(s: &str) -> Option<FeatureClass> {
        if s.len() != 1 {
            match s.chars().collect::<Vec<char>>().first().unwrap() {
                'A' => Some(FeatureClass::Region),
                'H' => Some(FeatureClass::WaterBody),
                'L' => Some(FeatureClass::Area),
                'P' => Some(FeatureClass::City),
                'R' => Some(FeatureClass::Road),
                'S' => Some(FeatureClass::Spot),
                'T' => Some(FeatureClass::Hill),
                'U' => Some(FeatureClass::Undersea),
                'V' => Some(FeatureClass::Forest),
                _ => None,
            }
        } else {
            None
        }
    }
}

impl LocationResponse {
    pub fn from_source_with_id(id: &str, source: Value) -> LocationResponse {
        LocationResponse {
            id: id.to_string(),
            name: source["name"].as_str().unwrap().to_string(),
            ascii_name: source["ascii_name"].as_str().unwrap().to_string(),
            latitude: source["location"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .as_f64()
                .unwrap(),
            longitude: source["location"]
                .as_array()
                .unwrap()
                .first()
                .unwrap()
                .as_f64()
                .unwrap(),
            feature_class: FeatureClass::from_str(source["feature_class"].as_str().unwrap()),
            feature_code: source["feature_code"].as_str().unwrap().to_string(),
            country_code: source["country_code"].as_str().unwrap().to_string(),
            admin1: source["admin1"].as_str().map(str::to_string),
            admin2: source["admin2"].as_str().map(str::to_string),
            population: source["population"].as_i64(),
            elevation: source["elevation"].as_i64(),
            timezone: source["timezone"].as_str().unwrap().to_string(),
            modification_date: source["modification_date"].as_str().unwrap().to_string(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct CreateCalendar {
    pub lat: f64,
    pub lon: f64,
    pub before: usize,
    pub after: usize,
    pub number_of_days: usize,
    pub summary: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub query: String,
}

pub struct DBConnections {
    pub es: String,
}
