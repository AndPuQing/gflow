use crate::cli;

pub(crate) fn handle_add(add_args: cli::AddArgs) {
    log::debug!("{:?}", add_args);
    // check is absolute path
    let mut script = add_args.script;
    if !script.is_absolute() {
        let pwd = std::env::current_dir().unwrap();
        script = pwd.join(&script);
    }
    log::debug!("Add job: {:?}", script);
}
