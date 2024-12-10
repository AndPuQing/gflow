use std::env;

pub fn get_current_dir() -> String {
    let current_dir = env::current_dir().unwrap();
    current_dir.to_str().unwrap().to_string()
}

pub fn get_current_user() -> String {
    let current_user = whoami::username();
    current_user
}
