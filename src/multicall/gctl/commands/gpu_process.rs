use anyhow::Result;
use gflow::client::Client;

fn print_warning() {
    eprintln!("Warning: manually ignoring a GPU process is unsafe.");
    eprintln!("gflow may schedule onto that GPU even though the process is still attached.");
    eprintln!("This override is runtime-only and will be cleared after gflowd restarts.");
}

pub async fn handle_ignore_gpu_process(client: &Client, gpu: u32, pid: u32) -> Result<()> {
    print_warning();
    client.ignore_gpu_process(gpu, pid).await?;
    eprintln!("Warning: override applied for GPU {} PID {}.", gpu, pid);
    println!("Ignoring GPU process PID {} on GPU {}", pid, gpu);
    Ok(())
}

pub async fn handle_unignore_gpu_process(client: &Client, gpu: u32, pid: u32) -> Result<()> {
    client.unignore_gpu_process(gpu, pid).await?;
    println!(
        "Removed GPU process ignore override for PID {} on GPU {}",
        pid, gpu
    );
    Ok(())
}

pub async fn handle_list_gpu_processes(client: &Client) -> Result<()> {
    let processes = client.list_ignored_gpu_processes().await?;
    if processes.is_empty() {
        println!("No ignored GPU processes");
        return Ok(());
    }

    print_warning();
    for process in processes {
        println!("gpu={}\tpid={}", process.gpu_index, process.pid);
    }
    Ok(())
}
