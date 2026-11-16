//! Native helper for the trusted host's bounded, sandboxed SQL protocol.
fn main() {
    std::process::exit(richview_app::connectors::sql_worker::run_stdio());
}
