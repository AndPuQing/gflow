use shared::random_run_name;
use std::path::PathBuf;
use tmux_interface::{HasSession, NewSession, SendKeys, Tmux, WaitFor};

// pub struct Job {
//     pub script: PathBuf,
//     pub gpus: Option<u32>,
//     worker_name: Option<String>,
// }

// impl Job {
//     pub fn new(script: PathBuf, gpus: Option<u32>) -> Self {
//         Self {
//             script,
//             gpus,
//             worker_name: None,
//         }
//     }

//     pub fn run(&mut self) {
//         let worker_name = random_run_name();
//         self.worker_name = Some(worker_name.clone());
//         Tmux::new()
//             .add_command(NewSession::new().detached().session_name(&worker_name))
//             .add_command(
//                 SendKeys::new()
//                     .target_client(&worker_name)
//                     .key(format!("sh {}", self.script.display())),
//             )
//             .output()
//             .unwrap();
//     }
// }
