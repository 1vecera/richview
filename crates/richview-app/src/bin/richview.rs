#[tokio::main]
async fn main() {
    if let Err(error) = richview_app::cli::run().await {
        eprintln!("richview: {error}");
        std::process::exit(2);
    }
}
