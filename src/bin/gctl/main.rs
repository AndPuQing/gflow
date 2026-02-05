#[path = "../../bin_helpers/multicall_wrapper.rs"]
mod multicall;

fn main() -> std::process::ExitCode {
    multicall::exec("gctl")
}
