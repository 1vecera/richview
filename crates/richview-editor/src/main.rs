//! The RichView editor service: the Language Server Protocol over stdio (ARC-44).
fn main() -> std::io::Result<()> {
    richview_editor::lsp::run()
}
