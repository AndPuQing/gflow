use reqwest::{Error, Response};
use shared::get_config_temp_file;

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

    pub async fn add_job(&self, job: shared::Job) -> Result<Response, Error> {
        log::debug!("Client added job: {:?}", job);
        let url = format!("http://localhost:{}/job", self.port);
        self.re_client.post(&url).json(&job).send().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_build() {
        let gflowd_file = get_config_temp_file();
        let port = 1234;
        let mut file = File::create(gflowd_file).unwrap();
        file.write_all(port.to_string().as_bytes()).unwrap();
        let client = Client::build().unwrap();
        assert_eq!(client.port, port);
    }
}
