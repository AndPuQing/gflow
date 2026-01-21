/// Macro for printing formatted key-value pairs in the style: Key=Value
///
/// Usage:
/// ```ignore
/// use gflow::print_field;
///
/// print_field!("ID", "{}", job.id);
/// print_field!("State", "{} ({})", job.state, job.state.short_form());
/// ```
#[macro_export]
macro_rules! print_field {
    ($key:expr, $($arg:tt)*) => {
        println!("  {}={}", $key, format!($($arg)*))
    };
}

/// Macro for printing optional fields (only prints if Some)
///
/// Usage:
/// ```ignore
/// use gflow::print_optional_field;
///
/// print_optional_field!("Script", job.script, |s| s.display());
/// print_optional_field!("CondaEnv", job.conda_env);
/// ```
#[macro_export]
macro_rules! print_optional_field {
    ($key:expr, $value:expr) => {
        if let Some(ref val) = $value {
            println!("  {}={}", $key, val);
        }
    };
    ($key:expr, $value:expr, |$param:ident| $formatter:expr) => {
        if let Some(ref $param) = $value {
            println!("  {}={}", $key, $formatter);
        }
    };
}
