//! `pin_process_env` (own test binary: it changes the process environment, which must not
//! race the library tests' optimizer runs).

use fc_app::system::{pin_process_env, PINNED_ENV_VARS};

#[test]
fn pinned_variables_are_removed_at_start() {
    assert!(PINNED_ENV_VARS.contains(&"FSRS_NO_OUTLIER"));
    for var in PINNED_ENV_VARS {
        std::env::set_var(var, "1");
    }
    pin_process_env();
    for var in PINNED_ENV_VARS {
        assert!(std::env::var_os(var).is_none(), "{var} still set");
    }
    // Idempotent.
    pin_process_env();
    assert!(std::env::var_os("FSRS_NO_OUTLIER").is_none());
}
