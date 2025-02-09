use shared::get_config_temp_file;

pub struct Client {
    port: u32,
}

impl Client {
    pub fn build() -> Result<Self, &'static str> {
        let gflowd_file = get_config_temp_file();
        if !gflowd_file.exists() {
            Err("gflowd file does not exist")
        } else {
            let port = std::fs::read_to_string(gflowd_file)
                .unwrap()
                .parse::<u32>()
                .unwrap();
            Ok(Self { port })
        }
    }

    pub fn connect(&self) {
        println!("Connecting to port: {}", self.port);
    }
}
