use std::path::PathBuf;

pub struct Job {
    pub script: PathBuf,
    pub gpus: Option<u32>,
    worker_name: Option<String>,
}

pub trait GPU {
    fn get_gpu_count() -> u32;
}

pub fn random_run_name() -> String {
    let words = vec![
        "Lion", "Tiger", "Elephant", "Giraffe", "Bear", "Monkey", "Zebra", "Kangaroo", "Panda",
        "Penguin", "Happy", "Sad", "Angry", "Sleepy", "Hungry", "Thirsty", "Silly", "Crazy",
        "Funny", "Grumpy",
    ];

    use rand::Rng;
    let mut rng = rand::rng();
    let word = words[rng.random_range(0..words.len())].to_lowercase();
    let number = rng.random_range(0..10);
    format!("{}-{}", word, number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_run_name() {
        let name = random_run_name();
        assert!(name.contains("-"));
    }
}
