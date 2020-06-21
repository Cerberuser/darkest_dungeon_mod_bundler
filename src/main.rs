use log::LevelFilter;
use simplelog::{ConfigBuilder, WriteLogger};
use std::fs::File;

fn main() {
    let log_level = match std::env::args().next().as_deref() {
        Some("--debug") => LevelFilter::Trace,
        _ => LevelFilter::Error,
    };

    WriteLogger::init(
        log_level,
        ConfigBuilder::new()
            .add_filter_allow_str("darkest_dungeon_mod_bundler")
            .build(),
        File::create("log").unwrap(),
    )
    .unwrap();
    darkest_dungeon_mod_bundler::run();
}
