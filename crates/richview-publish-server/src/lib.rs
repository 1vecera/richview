//! The rules of a RichView Publish server: access, the ledger, the editorial workflow, the routes and the desk.
//!
//! Everything here is pure (ARC-02, ARC-03): a host adapter, such as the Cloudflare Worker, passes one request with
//! the state it loaded and answers the `Need`s that come back (storage reads, Git host calls) until it gets a
//! `Done`, then commits the new state. No rule exists only in the adapter.
pub mod access;
pub mod desk;
pub mod forge;
pub mod host;
pub mod ledger;
pub mod migrate;
pub mod model;
pub mod publishing;
pub mod respond;
pub mod routes;
#[cfg(target_arch = "wasm32")]
mod wasm;
pub mod wire;

pub use host::*;
pub use ledger::State;

/// Runs one request against `state` and returns a response, more needs, or the state to commit.
///
/// The host check, the cross-site write check and the identity routes live here; the three route families
/// are in `routes`, `publishing` and `desk`.
pub fn handle(
    cfg: &Config,
    principal: Option<&Principal>,
    state: &State,
    io: &Io,
    now: &str,
    nonce: &str,
    req: &Request,
) -> Outcome {
    let mut ctx = Ctx::new(cfg, principal, state, io, now, nonce);
    let result = dispatch(&mut ctx, req);
    finish(ctx, result)
}

fn dispatch(ctx: &mut Ctx, req: &Request) -> R<Response> {
    let local = ["localhost", "127.0.0.1"].contains(&req.host.as_str());
    if req.host != ctx.cfg.publication_host && !local {
        return Err(error(404, "Unknown publication host."));
    }
    if !req.is_read() {
        let origin = req.header("origin");
        if origin.is_some_and(|o| o != format!("https://{}", req.host) && !local) {
            return Err(error(403, "Cross-origin writes are not accepted."));
        }
        if req.header("sec-fetch-site") == Some("cross-site") {
            return Err(error(403, "Cross-site writes are not accepted."));
        }
    }
    let access = access::role(ctx.principal, ctx.cfg, &ctx.state.roles);
    let path = req.path.as_str();
    if path == "/_auth/login" {
        return match ctx.principal {
            None => Err(error(
                401,
                "Sign in through the configured identity provider.",
            )),
            Some(_) => Ok(respond::redirect(303, "/internal")),
        };
    }
    if path == "/_auth/session" {
        return match ctx.principal {
            Some(p) => Ok(respond::json(
                &serde_json::json!({"identity": p.key, "role": access}),
                200,
            )),
            None => Err(error(401, "Authentication required.")),
        };
    }
    if path.starts_with("/_editor/") {
        return desk::handle(ctx, req, &access);
    }
    if path.starts_with("/_publish/") {
        return publishing::handle(ctx, req, &access);
    }
    if !req.is_read() {
        return Err(error(405, "Method not allowed"));
    }
    routes::read(ctx, req, &access)
}
