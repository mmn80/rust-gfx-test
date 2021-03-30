use sled;

mod config;

pub fn init() -> () {
    let config = config::read_config();
    println!("{}", toml::to_string(&config).unwrap());
    let db = sled::open("./sled-db").expect("db open");
    db.flush().expect("db flush error");
    config::write_config(&config);
}
