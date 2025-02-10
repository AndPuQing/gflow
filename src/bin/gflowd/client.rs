use gflow::get_config_temp_file;
use reqwest::{Error, Response};

pub struct Client {
    re_client: reqwest::Client,
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
            let re_client = reqwest::Client::new();
            Ok(Self { re_client, port })
        }
    }

    pub async fn add_job(&self, job: gflow::Job) -> Result<Response, Error> {
        log::debug!("Client added job: {:?}", job);
        let url = format!("http://localhost:{}/job", self.port);
        self.re_client.post(&url).json(&job).send().await
    }
}
