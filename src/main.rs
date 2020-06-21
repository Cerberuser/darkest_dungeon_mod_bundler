use log::LevelFilter;
use simplelog::{ConfigBuilder, WriteLogger};
use std::fs::File;

fn main() {
    let log_level = match std::env::args().nth(1).as_deref() {
        Some("--debug") => LevelFilter::Debug,
        _ => LevelFilter::Error,
    };

    WriteLogger::init(
        log_level,
        ConfigBuilder::new()
            .add_filter_allow_str("darkest_dungeon_mod_bundler")
            .set_time_level(LevelFilter::Error)
            .set_target_level(LevelFilter::Trace)
            .set_location_level(LevelFilter::Trace)
            .set_thread_level(LevelFilter::Trace)
            .build(),
        File::create("log").unwrap(),
    )
    .unwrap();
    darkest_dungeon_mod_bundler::run();
}
