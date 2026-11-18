//! Temporary: replay an export from a seat with the log on, then show the move. why2 file= convention= turn= seat= [variants= colours=] [log=build|move]
use hanab_bot::logger::{self, LogLevel};
use hanab_bot::settings::ConventionKind;
use hanab_bot::tools::game_data::{GameData, build_game, load_variant};
use std::collections::HashMap;
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let map: HashMap<String, String> = args
        .iter()
        .filter_map(|a| a.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut data = GameData::from_file(&map["file"])?;
    data.actions.truncate(map["turn"].parse().unwrap());
    let variant = load_variant(&map["variant"], &map)?;
    let log = map.get("log").map_or("build", String::as_str);
    logger::set_level(if log == "build" {
        LogLevel::Info
    } else {
        LogLevel::Off
    });
    let game = build_game(
        ConventionKind::parse(&map["convention"]).map_err(|e| e.to_string())?,
        &data,
        variant,
        map["seat"].parse().unwrap(),
        true,
    )?;
    logger::set_level(if log == "move" {
        LogLevel::Info
    } else {
        LogLevel::Off
    });
    println!("ACTION {:?}", game.take_action());
    Ok(())
}
