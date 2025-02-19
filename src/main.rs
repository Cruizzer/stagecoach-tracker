use dotenv::dotenv;
use std::env;
use reqwest::Client;
use tokio::time;
use chrono::{Local, Timelike};
use std::time::Duration;
use serde_json::Value;
use std::f64::consts::PI;

#[derive(Debug)]
struct BusStop {
    name: String,
    lat: f64,
    lng: f64,
}

const API_URL: &str = "https://api.stagecoach-technology.net/vehicle-tracking/v1/vehicles";


#[tokio::main]
async fn main() {
    dotenv().ok(); // Load .env file

    let client = Client::new();
    let bus_stops = load_bus_stops();

    loop {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();
        let second = now.second();

        println!("\nCurrent time: {:02}:{:02}:{:02}", hour, minute, second);

        if let Err(e) = check_buses(&client, &bus_stops).await {
            eprintln!("Error checking buses: {}", e);
        }

        time::sleep(Duration::from_secs(10)).await;
    }
}

// Load bus stops from .env file
fn load_bus_stops() -> Vec<BusStop> {
    let stops_str = env::var("BUS_STOPS").unwrap_or_default();
    
    println!("============== Bus Stops Loaded ==============");
    let stops = stops_str
        .split(';')  
        .filter_map(|s| {
            let mut parts = s.split(',');
            if let (Some(name), Some(lat), Some(lng)) = (
                parts.next().map(|x| x.trim()),
                parts.next().map(|x| x.trim()), 
                parts.next().map(|x| x.trim())
            ) {
                Some(BusStop {
                    name: name.to_string(),
                    lat: lat.parse().ok()?,
                    lng: lng.parse().ok()?,
                })
            } else {
                None
            }
        })
        .collect::<Vec<BusStop>>();  // Collect into a vector of BusStop objects

    // Print the bus stops
    for stop in &stops {
        println!(
            "Stop Name: {}, Latitude: {}, Longitude: {}",
            stop.name, stop.lat, stop.lng
        );
    }

    println!("=============================================");

    stops
}

async fn check_buses(client: &Client, bus_stops: &[BusStop]) -> Result<(), reqwest::Error> {
    let lat: f64 = env::var("LAT")
    .expect("Missing LAT in environment variables. Please set LAT to the correct latitude.")
    .parse()
    .expect("LAT must be a valid floating-point number.");

    let lng: f64 = env::var("LNG")
        .expect("Missing LNG in environment variables. Please set LNG to the correct longitude.")
        .parse()
        .expect("LNG must be a valid floating-point number.");

    let radius: u32 = env::var("RADIUS")
        .expect("Missing RADIUS in environment variables. Please set RADIUS to a valid integer (in meters).")
        .parse()
        .expect("RADIUS must be a valid integer.");

    println!("Checking buses within {} meters of location ({}, {})", radius, lat, lng);

    let url = format!(
        "{}?client_version=UKBUS_APP&descriptive_fields=1&lat={}&lng={}&radius={}",
        API_URL, lat, lng, radius
    );

    let response = client.get(&url).send().await?.json::<Value>().await?;

    if let Some(services) = response["services"].as_array() {
        for service in services {
            if let (Some(bus_lat), Some(bus_lng)) = (
                service["latitude"].as_str().and_then(|s| s.parse::<f64>().ok()),
                service["longitude"].as_str().and_then(|s| s.parse::<f64>().ok()),
            ) {
                // Get the service number (serviceNumber) and description (serviceDescription)
                let service_number = service["serviceNumber"].as_str().unwrap_or("Unknown");
                let service_description = service["serviceDescription"].as_str().unwrap_or("No description");
        
                // Print the current bus's location and service details
                println!("Found (Bus {} [{}]): lat = {}, lng = {}", service_number, service_description, bus_lat, bus_lng);
        
                if let Some(nearby_stop) = find_nearest_stop(bus_lat, bus_lng, bus_stops) {
                    let message = format!(
                        "Bus ({}) {} is near **{}**!",
                        service_number, service_description, nearby_stop
                    );
        
                    send_telegram_message(&message).await?;
                    println!("Bus {} found near: {}", service_number, nearby_stop);
                } else {
                    println!("Bus {} is not near any stops.", service_number);
                }
            }
        }
    } else {
        println!("No services found in the response.");
    }
    
    
    Ok(())
}


/// Haversine formula to calculate the distance (in meters) between two latitude/longitude points
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    // Earth radius in meters
    const EARTH_RADIUS: f64 = 6371e3; // meters

    // Convert degrees to radians
    let lat1_rad = lat1 * PI / 180.0;
    let lat2_rad = lat2 * PI / 180.0;
    let delta_lat = (lat2 - lat1) * PI / 180.0;
    let delta_lon = (lon2 - lon1) * PI / 180.0;

    // Haversine formula
    let a = f64::sin(delta_lat / 2.0).powi(2)
        + f64::cos(lat1_rad) * f64::cos(lat2_rad) * f64::sin(delta_lon / 2.0).powi(2);
    let c = 2.0 * f64::atan2(f64::sqrt(a), f64::sqrt(1.0 - a));

    // Distance in meters
    EARTH_RADIUS * c
}


/// Finds the nearest bus stop within 200 meters using the Haversine formula
fn find_nearest_stop(bus_lat: f64, bus_lng: f64, bus_stops: &[BusStop]) -> Option<String> {
    const MAX_DISTANCE_METERS: f64 = 200.0; // If the bus is within a radius of 200m from any bus stop.

    for stop in bus_stops {
        let distance = haversine_distance(bus_lat, bus_lng, stop.lat, stop.lng);

        // Print debugging information
        println!("Stop {} - {:.2} meters away from bus (Lat: {}, Lng: {})", stop.name, distance, stop.lat, stop.lng);

        // Check if the bus is within 200 meters of the stop
        if distance <= MAX_DISTANCE_METERS {
            println!("Bus is within range of stop: {}", stop.name);
            return Some(stop.name.clone());
        }
    }

    println!("No bus found near any stop.");
    None
}

// Send notification to Telegram
async fn send_telegram_message(message: &str) -> Result<(), reqwest::Error> {
    let client = Client::new();
    let bot_token = env::var("TELEGRAM_BOT_TOKEN").expect("Missing TELEGRAM_BOT_TOKEN in .env");
    let chat_id = env::var("TELEGRAM_CHAT_ID").expect("Missing TELEGRAM_CHAT_ID in .env");

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage?chat_id={}&text={}",
        bot_token, chat_id, message
    );

    let response = client.get(&url).send().await?;
    println!("Telegram message sent: {:?}", response.text().await?);
    Ok(())
}
