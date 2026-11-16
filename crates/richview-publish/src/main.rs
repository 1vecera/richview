use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "richview-publish",
    about = "Build an immutable RichView publication from Git content"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(long)]
        site: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        runtime: PathBuf,
        #[arg(long)]
        source_commit: String,
        #[arg(long, env = "RICHVIEW_CONNECTORS")]
        connectors: Option<PathBuf>,
    },
}
fn main() {
    let Args { command } = Args::parse();
    let result = match command {
        Command::Build {
            site,
            output,
            runtime,
            source_commit,
            connectors,
        } => richview_publish::build_with_connectors(
            &site,
            &output,
            &runtime,
            &source_commit,
            connectors.as_deref(),
        ),
    };
    match result {
        Ok(release) => println!(
            "{}",
            serde_json::json!({"release":release.id,"commit":release.commit,"items":release.items.len(),"objects":release.objects.len()})
        ),
        Err(error) => {
            eprintln!("Publication build failed: {error}");
            std::process::exit(1);
        }
    }
}
