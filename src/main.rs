// main.rs
use actix_web::{get, web, App, Error, HttpResponse, HttpServer, Responder};
use actix_files as fs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::env;

struct AppState {
    api_key: String,
    game_sizes: Mutex<HashMap<String, GameSize>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct GameSize {
    name: String,
    size_gb: f64,
}

#[derive(Serialize, Deserialize)]
struct SteamResolveResponse {
    response: SteamResolveData,
}

#[derive(Serialize, Deserialize)]
struct SteamResolveData {
    success: u8,
    steamid: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SteamGamesResponse {
    response: SteamGamesData,
}

#[derive(Serialize, Deserialize)]
struct SteamGamesData {
    game_count: Option<u32>,
    games: Option<Vec<SteamGame>>,
}

#[derive(Serialize, Deserialize)]
struct SteamGame {
    appid: u64,
    name: Option<String>,
    playtime_forever: Option<u32>,
}

#[derive(Serialize)]
struct SizeResult {
    total_size_gb: f64,
    total_size_display: String,
    total_games: usize,
    games: Vec<GameWithSize>,
}

#[derive(Serialize)]
struct GameWithSize {
    name: String,
    size: f64,
}

#[derive(Deserialize)]
struct ResolveRequest {
    id: String,
}

#[derive(Deserialize)]
struct GamesRequest {
    id: String,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/index.html"))
}

#[get("/api/resolve")]
async fn resolve_vanity_url(
    query: web::Query<ResolveRequest>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let url = format!(
        "https://api.steampowered.com/ISteamUser/ResolveVanityURL/v0001/?key={}&vanityurl={}",
        data.api_key, query.id
    );
    
    println!("Attempting to resolve vanity URL: {}", query.id);
    
    match ureq::get(&url).call() {
        Ok(response) => {
            println!("Received response from Steam API (resolve): status {}", response.status());
            match response.into_json::<SteamResolveResponse>() {
                Ok(steam_response) => {
                    println!("Successfully parsed resolve response: success={}", steam_response.response.success);
                    Ok(HttpResponse::Ok().json(steam_response))
                },
                Err(e) => {
                    println!("Error parsing Steam API resolve response: {}", e);
                    Ok(HttpResponse::InternalServerError().body(format!("Failed to parse Steam API response: {}", e)))
                }
            }
        },
        Err(e) => {
            println!("Error contacting Steam API for vanity URL: {}", e);
            Ok(HttpResponse::InternalServerError().body(format!("Failed to contact Steam API: {}", e)))
        }
    }
}

#[get("/api/games")]
async fn get_games(
    query: web::Query<GamesRequest>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let url = format!(
        "https://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/?key={}&steamid={}&format=json&include_appinfo=true",
        data.api_key, query.id
    );
    
    println!("Attempting to get games for Steam ID: {}", query.id);
    
    match ureq::get(&url).call() {
        Ok(response) => {
            println!("Received response from Steam API (games): status {}", response.status());
            match response.into_json::<SteamGamesResponse>() {
                Ok(steam_response) => {
                    let game_count = steam_response.response.game_count.unwrap_or(0);
                    println!("Successfully parsed games response: {} games", game_count);
                    Ok(HttpResponse::Ok().json(steam_response))
                },
                Err(e) => {
                    println!("Error parsing Steam API games response: {}", e);
                    Ok(HttpResponse::InternalServerError().body(format!("Failed to parse Steam API response: {}", e)))
                }
            }
        },
        Err(e) => {
            println!("Error contacting Steam API for games: {}", e);
            Ok(HttpResponse::InternalServerError().body(format!("Failed to contact Steam API: {}", e)))
        }
    }
}

#[get("/api/calculate-size")]
async fn calculate_size(
    query: web::Query<GamesRequest>,
    data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    println!("Calculating size for Steam ID: {}", query.id);
    
    let url = format!(
        "https://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/?key={}&steamid={}&format=json&include_appinfo=true",
        data.api_key, query.id
    );
    
    let steam_response = match ureq::get(&url).call() {
        Ok(response) => {
            println!("Received response from Steam API (calculate): status {}", response.status());
            match response.into_json::<SteamGamesResponse>() {
                Ok(steam_response) => {
                    println!("Successfully parsed calculate response");
                    steam_response
                },
                Err(e) => {
                    println!("Error parsing Steam API calculate response: {}", e);
                    return Ok(HttpResponse::InternalServerError().body(format!("Failed to parse Steam API response: {}", e)))
                }
            }
        },
        Err(e) => {
            println!("Error contacting Steam API for calculate: {}", e);
            return Ok(HttpResponse::InternalServerError().body(format!("Failed to contact Steam API: {}", e)))
        }
    };
    
    let games = match steam_response.response.games {
        Some(games) => {
            println!("Found {} games in Steam library", games.len());
            games
        },
        None => {
            println!("No games found in response - profile might be private");
            return Ok(HttpResponse::BadRequest().body("Could not retrieve games list - profile might be private"))
        },
    };
    
    // Debug current database
    let game_sizes = data.game_sizes.lock().unwrap();
    println!("Database contains {} games", game_sizes.len());
    if game_sizes.len() > 0 {
        println!("First few game IDs in database: {:?}", 
            game_sizes.keys().take(5).collect::<Vec<_>>());
    }
    
    // Process games and calculate sizes
    let mut total_size_gb = 0.0;
    let mut games_with_size: Vec<GameWithSize> = Vec::new();
    
    for game in &games {
        let app_id = game.appid.to_string();
        if let Some(game_size) = game_sizes.get(&app_id) {
            println!("Found size for game {}: {} GB", 
                game.name.as_deref().unwrap_or(&app_id), game_size.size_gb);
            let size = game_size.size_gb;
            total_size_gb += size;
            games_with_size.push(GameWithSize {
                name: game.name.clone().unwrap_or_else(|| game_size.name.clone()),
                size,
            });
        }
    }
    
    println!("Total size: {:.2} GB, Found sizes for {} games", 
        total_size_gb, games_with_size.len());
    
    games_with_size.sort_by(|a, b| b.size.partial_cmp(&a.size).unwrap());
    
    let games_with_size = games_with_size.into_iter().take(20).collect::<Vec<_>>();
    
    let total_size_display = if total_size_gb >= 1024.0 {
        format!("{:.2} TB", total_size_gb / 1024.0)
    } else {
        format!("{:.2} GB", total_size_gb)
    };
    
    let result = SizeResult {
        total_size_gb,
        total_size_display,
        total_games: games.len(),
        games: games_with_size,
    };
    
    println!("Returning size result with {} sized games out of {} total games", 
        result.games.len(), result.total_games);
    
    Ok(HttpResponse::Ok().json(result))
}

fn load_game_sizes() -> HashMap<String, GameSize> {
    // Try multiple possible locations for the file
    let possible_paths = [
        "static/game_sizes_database.json",
        "../static/game_sizes_database.json",
        "../../static/game_sizes_database.json",
        "./static/game_sizes_database.json"
    ];
    
    for file_path in possible_paths {
        println!("Attempting to load game sizes from: {}", file_path);
        
        match std::fs::read_to_string(file_path) {
            Ok(content) => {
                println!("Successfully read file from {}", file_path);
                match serde_json::from_str(&content) {
                    Ok(sizes) => {
                        let sizes_map: HashMap<String, GameSize> = sizes;
                        println!("Successfully parsed JSON. Found {} entries", sizes_map.len());
                        return sizes_map;
                    },
                    Err(e) => {
                        println!("JSON parse error: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("File not found at {}: {}", file_path, e);
            }
        }
    }
    
    // If we reached here, none of the paths worked
    println!("Could not find game_sizes_database.json in any location");
    HashMap::new()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Add path checking code
    let test_path = "static/game_sizes_database.json";
    match std::fs::metadata(test_path) {
        Ok(metadata) => println!("File exists with relative path! Size: {} bytes", metadata.len()),
        Err(e) => println!("File not found with relative path: {}", e),
    }
    
    let absolute_path = std::env::current_dir().unwrap().join("static").join("game_sizes_database.json");
    match std::fs::metadata(&absolute_path) {
        Ok(metadata) => println!("File exists at absolute path: {:?}, Size: {} bytes", absolute_path, metadata.len()),
        Err(e) => println!("File not found at absolute path: {:?}, Error: {}", absolute_path, e),
    }
    
    // Also try finding in the src directory
    let src_path = std::env::current_dir().unwrap().join("src").join("../static").join("game_sizes_database.json");
    match std::fs::metadata(&src_path) {
        Ok(metadata) => println!("File exists at src path: {:?}, Size: {} bytes", src_path, metadata.len()),
        Err(e) => println!("File not found at src path: {:?}, Error: {}", src_path, e),
    }
    
    // Print current directory for debugging
    println!("Current directory: {:?}", std::env::current_dir().unwrap());
    
    // Get Steam API key from environment variable
    let steam_api_key = env::var("STEAM_API_KEY").expect("STEAM_API_KEY environment variable must be set");
    println!("Using Steam API key: {}...", &steam_api_key[0..5]);
    
    // Load game sizes database
    let game_sizes = load_game_sizes();
    println!("Loaded {} game sizes", game_sizes.len());
    
    println!("Starting server at 0.0.0.0:8080");
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                api_key: steam_api_key.clone(),
                game_sizes: Mutex::new(game_sizes.clone()),
            }))
            .service(index)
            .service(resolve_vanity_url)
            .service(get_games)
            .service(calculate_size)
            .service(fs::Files::new("/css", "./static/css").show_files_listing())
            .service(fs::Files::new("/js", "./static/js").show_files_listing())
            .service(fs::Files::new("/", "./static").show_files_listing())
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
