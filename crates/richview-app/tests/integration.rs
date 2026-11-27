//! The crate's integration tests as one test binary: each `tests/<name>.rs` is a module here, so
//! the crate links one executable instead of one for every file. Run one file's tests with
//! `cargo test -p richview-app --test integration -- <name>::`.

#[path = "access.rs"]
mod access;
#[path = "cli_oauth.rs"]
mod cli_oauth;
#[path = "connectors_e2e.rs"]
mod connectors_e2e;
#[path = "docs_match.rs"]
mod docs_match;
#[path = "hosted_download_admission.rs"]
mod hosted_download_admission;
#[path = "hosted_links.rs"]
mod hosted_links;
#[path = "hosted_mail.rs"]
mod hosted_mail;
#[path = "hosted_manual_outbox.rs"]
mod hosted_manual_outbox;
#[path = "hosted_route_matrix.rs"]
mod hosted_route_matrix;
#[path = "hosted_scope.rs"]
mod hosted_scope;
#[path = "hosted_shutdown.rs"]
mod hosted_shutdown;
#[path = "hosted_signin.rs"]
mod hosted_signin;
#[path = "hosted_signup_http.rs"]
mod hosted_signup_http;
#[path = "hosted_startup.rs"]
mod hosted_startup;
#[path = "hosted_teams.rs"]
mod hosted_teams;
#[path = "identity.rs"]
mod identity;
#[path = "metricflow_e2e.rs"]
mod metricflow_e2e;
#[path = "native_contract.rs"]
mod native_contract;
#[path = "parquet.rs"]
mod parquet;
#[path = "pdf_startup.rs"]
mod pdf_startup;
#[path = "registration.rs"]
mod registration;
#[path = "report_folders.rs"]
mod report_folders;
#[path = "schema2.rs"]
mod schema2;
#[path = "sign_in.rs"]
mod sign_in;
#[path = "sign_in_browser.rs"]
mod sign_in_browser;
#[path = "social_sign_in.rs"]
mod social_sign_in;
#[path = "unsplash.rs"]
mod unsplash;

/// With `autotests = false` a `tests/<name>.rs` that is not declared above is no target and never runs.
#[test]
fn every_test_file_is_a_module_of_this_binary() {
    let root = include_str!("integration.rs");
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.ends_with(".rs")
            && name != "integration.rs"
            && !root.contains(&format!("#[path = \"{name}\"]"))
        {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "declare these in tests/integration.rs, or they never run: {missing:?}"
    );
}
