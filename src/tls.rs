use std::sync::Once;

pub fn ensure_rustls_provider_installed() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Ignore the error in case another part of the process already installed
        // a provider (it is a process-wide singleton).
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
