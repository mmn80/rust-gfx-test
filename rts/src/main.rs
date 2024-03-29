// There's a decent amount of code that's just for example and isn't called
#![allow(dead_code)]

use rts::DemoArgs;
use structopt::StructOpt;
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder};

pub fn logging_init() {
    #[cfg(not(debug_assertions))]
    let log_level = log::LevelFilter::Info;
    #[cfg(debug_assertions)]
    let log_level = log::LevelFilter::Debug;

    // Setup logging
    env_logger::Builder::from_default_env()
        .default_format_timestamp_nanos(true)
        .filter_module(
            "rafx_assets::resources::descriptor_sets",
            log::LevelFilter::Info,
        )
        .filter_module("rafx_framework::nodes", log::LevelFilter::Info)
        .filter_module("rafx_framework::visibility", log::LevelFilter::Info)
        .filter_module("rafx_framework::graph", log::LevelFilter::Debug)
        .filter_module("rafx_framework::resources", log::LevelFilter::Debug)
        .filter_module("rafx_framework::graph::graph_plan", log::LevelFilter::Info)
        .filter_module("rafx_api", log::LevelFilter::Debug)
        .filter_module("rafx_framework", log::LevelFilter::Debug)
        .filter_module("rts::phases", log::LevelFilter::Debug)
        .filter_module("mio", log::LevelFilter::Debug)
        // .filter_module(
        //     "rafx_assets::resources::command_buffers",
        //     log::LevelFilter::Trace,
        // )
        .filter_level(log_level)
        // .format(|buf, record| { //TODO: Get a frame count in here
        //     writeln!(buf,
        //              "{} [{}] - {}",
        //              chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
        //              record.level(),
        //              record.args()
        //     )
        // })
        .init();
}

fn main() {
    logging_init();

    let args = DemoArgs::from_args();

    let event_loop = EventLoop::new();
    let size = PhysicalSize::new(1920.0, 1080.0);
    let window = WindowBuilder::new()
        .with_title("Voxel RTS")
        .with_inner_size(size)
        //.with_fullscreen(Some(Fullscreen::Borderless(None)))
        .build(&event_loop)
        .expect("Failed to create window");

    rts::update_loop(&args, window, event_loop).unwrap();
}

// cli command:
//   cargo run --package cli -- assets ../../rafx/rafx-plugins/assets pack assets.pack
// run with pack command:
//   cargo run --bin rts --release -- --packfile assets.pack
// add backtrace command prefix for powershell:
//   $env:RUST_BACKTRACE=1;
