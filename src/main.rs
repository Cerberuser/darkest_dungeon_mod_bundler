use log::LevelFilter;
use simplelog::WriteLogger;
use std::fs::File;

fn main() {
    WriteLogger::init(
        LevelFilter::Debug,
        simplelog::ConfigBuilder::new()
            .add_filter_ignore_str("serde_xml_rs")
            .build(),
        File::create("log").unwrap(),
    )
    .unwrap();
    darkest_mod_bundler::run();
}
