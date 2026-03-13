const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("VERGEN_BUILD_TIMESTAMP"),
    ")\n",
    "Branch: ",
    env!("VERGEN_GIT_BRANCH"),
    "\nCommit: ",
    env!("VERGEN_GIT_SHA"),
);

pub fn version() -> &'static str {
    use std::sync::OnceLock;

    static VERSION: OnceLock<String> = OnceLock::new();

    VERSION.get_or_init(|| {
        let author = clap::crate_authors!();
        format!(
            "\
{VERSION_MESSAGE}
Authors: {author}"
        )
    })
}
