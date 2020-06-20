use log::LevelFilter;
use simplelog::{ConfigBuilder, WriteLogger};
use std::fs::File;

fn main() {
    let loglevel = match std::env::args().next().as_deref() {
        Some("-vvvv") => LevelFilter::Trace,
        Some("-vvv") => LevelFilter::Debug,
        Some("-vv") => LevelFilter::Info,
        Some("-v") => LevelFilter::Warn,
        _ => LevelFilter::Error,
    };

    WriteLogger::init(
        loglevel,
        ConfigBuilder::new()
            .add_filter_allow_str("darkest_dungeon_mod_bundler")
            .build(),
        File::create("log").unwrap(),
    )
    .unwrap();
    darkest_dungeon_mod_bundler::run();
}
