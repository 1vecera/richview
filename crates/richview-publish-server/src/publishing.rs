//! The publishing API under `/_publish/`: object upload, staging, build status, activation, rollback and Git sync.
//!
//! The trusted publisher uploads and activates rendered artifacts; a super editor may only read the queue, sync
//! with Git and roll back. Activation is guarded five ways: the generation the build started from, the
//! publication branch head, an independent approval of that exact commit, a base that is the live commit, and the
//! pause a rollback leaves behind.
use crate::{
    access::{Access, RoleName},
    forge,
    host::*,
    ledger::{Approval, Build, BuildState, Edit, EditState, Policy, Private, State},
    model::{Release, State as ItemState},
    respond,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const OBJECT_LIMIT: usize = 20 * 1024 * 1024;
const RELEASE_LIMIT: usize = 256 * 1024 * 1024;
const BODY_LIMIT: usize = 8_000_000;
const CONTENT_TYPES: [&str; 8] = [
    "text/html; charset=utf-8",
    "text/javascript; charset=utf-8",
    "application/wasm",
    "application/octet-stream",
    "image/svg+xml",
    "image/png",
    "image/jpeg",
    "image/webp",
];

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}
fn is_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}
/// A lowercase identifier of 1 to 100 characters that does not start with a hyphen.
fn is_id(value: &str) -> bool {
    (1..=100).contains(&value.len())
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn is_source(value: &str) -> bool {
    value
        .strip_prefix("items/")
        .and_then(|r| r.strip_suffix("/report.rmd"))
        .is_some_and(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
}
fn bad(message: &str) -> Stop {
    error(400, message)
}

/// Checks a release contract before it is stored (the port of `verifyManifest`).
pub fn verify_manifest(m: &Release) -> R<()> {
    if m.schema != 1 || !is_hash(&m.id) || !is_commit(&m.commit) {
        return Err(bad("Invalid release contract"));
    }
    if m.items.len() > 1000 || m.objects.len() > 10_000 {
        return Err(bad("Release exceeds bounded limits"));
    }
    let mut seen = BTreeSet::new();
    for item in &m.items {
        let meta = &item.meta;
        if !is_id(&meta.id)
            || !seen.insert(meta.id.as_str())
            || !is_id(&meta.slug)
            || !is_id(&meta.section)
            || !is_id(&meta.audience)
            || !is_source(&item.source)
        {
            return Err(bad("Invalid item contract"));
        }
        let registered = [&item.article, &item.content]
            .into_iter()
            .chain(item.assets.values())
            .all(|h| m.objects.contains_key(h));
        if !registered {
            return Err(bad("Unregistered item object"));
        }
    }
    let mut total = 0usize;
    for (hash, object) in &m.objects {
        if !is_hash(hash)
            || object.sha256 != *hash
            || object.bytes > OBJECT_LIMIT
            || !CONTENT_TYPES.contains(&object.content_type.as_str())
        {
            return Err(bad("Invalid object descriptor"));
        }
        total += object.bytes;
    }
    if total > RELEASE_LIMIT {
        return Err(bad("Release exceeds 256 MiB"));
    }
    Ok(())
}

/// The release stored under `id`, or a 404.
pub fn release(ctx: &Ctx, id: &str) -> R<Release> {
    if !is_hash(id) {
        return Err(error(404, "Release not found"));
    }
    let text = ctx.blob_text(&format!("releases/{id}.json"))?;
    serde_json::from_str(&text).map_err(|_| error(503, "Release object unavailable"))
}

/// The GitHub logins of the super editors whose approval may authorize a release.
pub fn reviewers(cfg: &Config, state: &State) -> Vec<String> {
    let mut logins: Vec<(&str, Option<&str>)> = vec![];
    if !cfg.bootstrap_super_editor.is_empty()
        && !state.roles.contains_key(&cfg.bootstrap_super_editor)
    {
        logins.push((
            &cfg.bootstrap_super_editor,
            cfg.bootstrap_github_login.as_deref(),
        ));
    }
    let mut out: Vec<String> = logins
        .iter()
        .filter_map(|(_, l)| l.map(String::from))
        .collect();
    out.extend(
        state
            .roles
            .values()
            .filter(|r| r.name == RoleName::SuperEditor)
            .filter_map(|r| r.github_login.clone()),
    );
    out
}

/// The independent approval that lets `commit` be published, or none.
///
/// An already authorized commit keeps its authorization. Otherwise the commit must be the merge of a pull request
/// whose exact head a super editor other than its author approved, and the merge's first parent must be the commit
/// that is live, so a merge can never carry in an unreviewed base.
pub fn approval(ctx: &Ctx, commit: &str, state: &State) -> R<Option<Approval>> {
    if let Some(known) = state.authorized.get(commit) {
        return Ok(Some(known.clone()));
    }
    let Some(found) = forge::approved_commit(ctx, commit, &reviewers(ctx.cfg, state))? else {
        return Ok(None);
    };
    let merged = ctx.forge("GET", &format!("/git/commits/{commit}"), None)?;
    let base = merged
        .pointer("/parents/0/sha")
        .and_then(Value::as_str)
        .unwrap_or("");
    let live = match &state.active {
        Some(id) => Some(release(ctx, id)?.commit),
        None => None,
    };
    if live.as_deref() != Some(base) {
        return Err(error(
            409,
            "The publication merge includes a base that is not the live approved commit. Recover the branch before publishing.",
        ));
    }
    Ok(Some(Approval {
        base: Some(base.into()),
        ..found
    }))
}

pub fn handle(ctx: &mut Ctx, req: &Request, access: &Access) -> R<Response> {
    if !matches!(access.name, RoleName::Publisher | RoleName::SuperEditor) {
        return Err(error(
            403,
            "A publishing service or super editor is required",
        ));
    }
    let path = req.path.as_str();
    if access.name != RoleName::Publisher
        && (path.starts_with("/_publish/objects/")
            || [
                "/_publish/stage",
                "/_publish/building",
                "/_publish/failed",
                "/_publish/activate",
            ]
            .contains(&path))
    {
        return Err(error(
            403,
            "Only the trusted content publisher may upload or activate rendered artifacts",
        ));
    }
    let state = ctx.state;
    if req.method == "GET" && path == "/_publish/state" {
        return Ok(respond::json(
            &json!({"generation": state.generation, "active": state.active, "builds": state.builds}),
            200,
        ));
    }
    if req.method == "GET" && path == "/_publish/queue" {
        let mut queue = overview(ctx, state, None);
        queue["authorized"] = state
            .authorized
            .keys()
            .filter(|c| state.builds.get(*c).map(|b| b.state) != Some(BuildState::Live))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .into();
        return Ok(respond::json(&queue, 200));
    }
    if req.method == "PUT"
        && let Some(hash) = path.strip_prefix("/_publish/objects/")
    {
        return upload(ctx, req, hash);
    }
    if req.method != "POST" {
        return Err(error(405, "Method not allowed"));
    }
    let input: Input = req.json(BODY_LIMIT)?;
    match path {
        "/_publish/sync" => sync(ctx),
        "/_publish/stage" => stage(ctx, req),
        "/_publish/building" => progress(ctx, &input, BuildState::Building),
        "/_publish/failed" => progress(ctx, &input, BuildState::Failed),
        "/_publish/activate" => activate(ctx, access, &input, false),
        "/_publish/rollback" => activate(ctx, access, &input, true),
        _ => Err(error(404, "Unknown publication action")),
    }
}

/// The fields the status and activation calls read from the body.
#[derive(Deserialize, Default)]
struct Input {
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    release: Option<String>,
    #[serde(default)]
    generation: Option<u64>,
}

/// What the publisher still has to build: branch, live release and the changes without a preview.
fn overview(ctx: &Ctx, state: &State, only: Option<&BTreeSet<u64>>) -> Value {
    let previews: Vec<Value> = state
        .edits
        .values()
        .filter(|e| {
            matches!(e.state, EditState::PendingReview | EditState::Approved)
                && !state.builds.contains_key(&e.head)
                && only.is_none_or(|n| n.contains(&e.number))
        })
        .map(|e| json!({"number": e.number, "head": e.head}))
        .collect();
    json!({
        "branch": ctx.cfg.branch,
        "content_root": ctx.cfg.content_root,
        "active": state.active,
        "generation": state.generation,
        "previews": previews,
    })
}

/// Stores one verified object; an existing object is left alone.
fn upload(ctx: &mut Ctx, req: &Request, hash: &str) -> R<Response> {
    if !is_hash(hash) || req.body.len() > OBJECT_LIMIT {
        return Err(bad("Invalid object"));
    }
    if sha256(&req.body) != hash {
        return Err(bad("Object hash or size mismatch"));
    }
    ctx.put_blob(
        format!("objects/{hash}"),
        req.body.clone(),
        true,
        Some(hash.into()),
    );
    Ok(respond::json(&json!({"stored": hash}), 200))
}

/// A pull request in scope, or none; only a missing answer stops the sync.
/// Fetches the pull requests and their files in one round trip.
fn prefetch_pulls(ctx: &Ctx, numbers: &[u64]) -> R<()> {
    let paths: Vec<String> = numbers
        .iter()
        .flat_map(|n| {
            [
                format!("/pulls/{n}"),
                format!("/pulls/{n}/files?per_page=100"),
            ]
        })
        .collect();
    ctx.prefetch(&paths)
}
fn try_pull(ctx: &Ctx, number: u64) -> R<Option<Value>> {
    match forge::inspect_pull(ctx, number) {
        Ok(pr) => Ok(Some(pr)),
        Err(Stop::Need(needs)) => Err(Stop::Need(needs)),
        Err(Stop::Http(_)) => Ok(None),
    }
}
fn encode_component(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
fn string(v: &Value, pointer: &str) -> String {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or("")
        .into()
}

/// Whether a verified GitHub login belongs to someone who holds the editor role now.
fn login_is_editor(ctx: &Ctx, login: &str) -> bool {
    let cfg = ctx.cfg;
    let bootstrap = (!cfg.bootstrap_super_editor.is_empty()
        && !ctx.next.roles.contains_key(&cfg.bootstrap_super_editor))
    .then_some(cfg.bootstrap_github_login.as_deref())
    .flatten();
    bootstrap
        .into_iter()
        .chain(
            ctx.next
                .roles
                .values()
                .filter(|r| r.is_editor())
                .filter_map(|r| r.github_login.as_deref()),
        )
        .any(|l| l.eq_ignore_ascii_case(login))
}

/// Observes open pull requests, reconciles closed ones and says which commit is ready to publish.
fn sync(ctx: &mut Ctx) -> R<Response> {
    let pulls = ctx.forge(
        "GET",
        &format!(
            "/pulls?state=open&base={}&per_page=100",
            encode_component(&ctx.cfg.branch)
        ),
        None,
    )?;
    let numbers: Vec<u64> = pulls
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|c| c.get("number").and_then(Value::as_u64).unwrap_or(0))
        .collect();
    prefetch_pulls(ctx, &numbers)?;
    let mut candidates: Vec<Value> = vec![];
    for number in numbers {
        // Out-of-scope pull requests are never built.
        if let Some(pr) = try_pull(ctx, number)? {
            candidates.push(pr);
        }
    }
    let mut open = BTreeSet::new();
    for pr in &candidates {
        let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
        open.insert(number);
        let head = string(pr, "/head/sha");
        let previous = ctx.next.edits.get(&number).cloned();
        if previous.as_ref().is_some_and(|p| p.head == head) {
            continue;
        }
        let login = string(pr, "/user/login");
        ctx.change("git.change.observed", json!({"pull": number, "head": head}));
        if let Some(p) = &previous {
            ctx.next.forget_approval(&p.head);
        }
        let git_actor = format!("github:{login}");
        let mut edit = previous.unwrap_or_else(|| Edit {
            number,
            url: String::new(),
            head: String::new(),
            branch: String::new(),
            actor: git_actor.clone(),
            git_author: None,
            imported: true,
            state: EditState::PendingReview,
            path: None,
            contributors: vec![],
            commit: None,
            release: None,
            author_is_editor: login_is_editor(ctx, &login),
        });
        edit.url = string(pr, "/html_url");
        edit.head = head;
        edit.branch = string(pr, "/head/ref");
        edit.git_author = Some(login);
        edit.imported = true;
        edit.state = EditState::PendingReview;
        for who in [edit.actor.clone(), git_actor] {
            if !edit.contributors.contains(&who) {
                edit.contributors.push(who);
            }
        }
        ctx.next.edits.insert(number, edit);
    }
    let watched: Vec<u64> = ctx
        .next
        .edits
        .values()
        .filter(|e| {
            matches!(
                e.state,
                EditState::PendingReview
                    | EditState::Approved
                    | EditState::Building
                    | EditState::Failed
            ) && !open.contains(&e.number)
        })
        .map(|e| e.number)
        .collect();
    prefetch_pulls(ctx, &watched)?;
    for number in watched {
        let Some(pr) = try_pull(ctx, number)? else {
            continue;
        };
        if string(&pr, "/state") != "closed" {
            continue;
        }
        let merged = string(&pr, "/merged_at");
        ctx.change(
            "git.change.closed",
            json!({"pull": number, "merged": !merged.is_empty()}),
        );
        let Some(current) = ctx.next.edits.get(&number).cloned() else {
            continue;
        };
        if merged.is_empty() {
            ctx.next.forget_approval(&current.head);
            if let Some(e) = ctx.next.edits.get_mut(&number) {
                e.state = EditState::Closed;
            }
        } else {
            let commit = string(&pr, "/merge_commit_sha");
            let built = ctx.next.builds.get(&commit).map(|b| b.state);
            if let Some(e) = ctx.next.edits.get_mut(&number) {
                e.state = match built {
                    Some(BuildState::Live) => EditState::Live,
                    Some(BuildState::Failed) => EditState::Failed,
                    _ => EditState::Building,
                };
                e.commit = Some(commit);
            }
        }
    }
    let head = forge::head(ctx)?;
    let next = ctx.next.clone();
    let approved = next.authorized.contains_key(&head)
        || (next.active.is_none() && ctx.cfg.bootstrap_commit.as_deref() == Some(head.as_str()))
        || match approval(ctx, &head, &next) {
            Ok(found) => found.is_some(),
            Err(Stop::Need(needs)) => return Err(Stop::Need(needs)),
            // A merge with an unreviewed base is simply not ready.
            Err(Stop::Http(_)) => false,
        };
    let mut out = overview(ctx, &next, Some(&open));
    out["ready"] = if approved && !next.paused {
        head.into()
    } else {
        Value::Null
    };
    Ok(respond::json(&out, 200))
}

/// Registers a built release: verified contract, identity and objects; immutable and private until activated.
fn stage(ctx: &mut Ctx, req: &Request) -> R<Response> {
    let mut input: Release = req.json(BODY_LIMIT).map_err(|e| match e {
        Stop::Http(h) if h.status == 413 => Stop::Http(h),
        _ => bad("Invalid release contract"),
    })?;
    verify_manifest(&input)?;
    let nonpublic = input.items.iter().any(|i| {
        i.meta.state != ItemState::Published
            || i.meta.audience != "public"
            || input.site.mode == crate::model::Mode::Internal
    });
    if nonpublic && !forge::is_private(ctx)? {
        return Err(error(
            403,
            "Nonpublic sources require a private canonical repository",
        ));
    }
    let id = std::mem::take(&mut input.id);
    let encoded = |r: &Release| serde_json::to_vec(r).map_err(|_| bad("Invalid release contract"));
    if sha256(&encoded(&input)?) != id {
        return Err(bad("Manifest identity mismatch"));
    }
    input.id = id.clone();
    // Ask for every object head at once.
    let missing: Vec<Need> = input
        .objects
        .keys()
        .map(|h| format!("objects/{h}"))
        .filter(|k| !ctx.io.heads.contains_key(k))
        .map(|key| Need::Head { key })
        .collect();
    if !missing.is_empty() {
        return Err(Stop::Need(missing));
    }
    for (hash, object) in &input.objects {
        let ok = ctx
            .head(&format!("objects/{hash}"))?
            .is_some_and(|h| h.size == object.bytes && h.sha256 == *hash);
        if !ok {
            return Err(bad("Release is missing a verified object"));
        }
    }
    // Registered releases are immutable and remain private until activation or an editorial preview.
    ctx.put_blob(format!("releases/{id}.json"), encoded(&input)?, true, None);
    let runtime =
        serde_json::to_vec(&input.runtime).map_err(|_| bad("Invalid release contract"))?;
    ctx.change(
        "release.staged",
        json!({"release": id, "commit": input.commit}),
    );
    ctx.next
        .runtimes
        .insert(sha256(&runtime), input.runtime.clone());
    ctx.next.builds.insert(
        input.commit.clone(),
        Build {
            state: BuildState::Built,
            release: Some(id.clone()),
            reason: None,
        },
    );
    Ok(respond::json(
        &json!({"release": id, "preview": format!("/_preview/{id}/")}),
        200,
    ))
}

/// Records that a build started or failed; the live release is untouched.
fn progress(ctx: &mut Ctx, input: &Input, to: BuildState) -> R<Response> {
    let commit = input
        .commit
        .as_deref()
        .filter(|c| is_commit(c))
        .ok_or_else(|| bad("Invalid commit"))?;
    let label = if to == BuildState::Building {
        "building"
    } else {
        "failed"
    };
    let reason: String = input
        .reason
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(300)
        .collect();
    ctx.change(&format!("build.{label}"), json!({"commit": commit}));
    ctx.next.builds.insert(
        commit.into(),
        Build {
            state: to,
            release: None,
            reason: Some(reason),
        },
    );
    let edit_state = if to == BuildState::Building {
        EditState::Building
    } else {
        EditState::Failed
    };
    for edit in ctx.next.edits.values_mut() {
        if edit.commit.as_deref() == Some(commit) {
            edit.state = edit_state;
        }
    }
    Ok(respond::json(
        &json!({"state": label, "active": ctx.state.active}),
        200,
    ))
}

/// Makes a built release live, or rolls back to one that was.
fn activate(ctx: &mut Ctx, access: &Access, input: &Input, rollback: bool) -> R<Response> {
    let id = input.release.as_deref().unwrap_or("");
    let state = ctx.state;
    if rollback
        && (access.name != RoleName::SuperEditor
            || input.reason.as_deref().is_none_or(str::is_empty)
            || !state.activated.contains_key(id))
    {
        return Err(error(
            403,
            "Rollback requires a super editor, a reason and a previously live release",
        ));
    }
    let manifest = release(ctx, id)?;
    let mut authorization = None;
    if !rollback {
        if state.paused {
            return Err(error(
                409,
                "Publication is paused after rollback. A super editor must resume it explicitly.",
            ));
        }
        if manifest.commit != forge::head(ctx)? {
            return Err(error(
                409,
                "Build is superseded by the current publication branch",
            ));
        }
        let bootstrap = state.active.is_none()
            && ctx.cfg.bootstrap_commit.as_deref() == Some(manifest.commit.as_str());
        authorization = if bootstrap {
            Some(Approval {
                actor: "bootstrap".into(),
                pull: 0,
                at: None,
                head: Some(manifest.commit.clone()),
                base: None,
                origin: None,
            })
        } else {
            approval(ctx, &manifest.commit, state)?
        };
        if authorization.is_none() {
            return Err(error(
                403,
                "This commit has no independent publication approval",
            ));
        }
    }
    // The build ran against an older live state, so a release that activated meanwhile wins.
    if input.generation != Some(state.generation) {
        return Err(error(409, "Another release activated while this build ran"));
    }
    // What this release newly publishes or changes, against the release that is live now.
    let changed = match (&authorization, &state.active) {
        (Some(_), Some(live)) => {
            let live = release(ctx, live)?;
            changed_items(&live, &manifest)
        }
        (Some(_), None) => changed_items(
            &Release {
                items: vec![],
                ..manifest.clone()
            },
            &manifest,
        ),
        _ => vec![],
    };
    ctx.change(
        if rollback {
            "release.rollback"
        } else {
            "release.activated"
        },
        json!({"release": manifest.id, "commit": manifest.commit, "reason": input.reason}),
    );
    let now = ctx.now.to_string();
    let next = &mut ctx.next;
    if let Some(mut found) = authorization {
        if found.at.is_none() {
            found.at = Some(now.clone());
        }
        // OPN-54: an editor's own change lands private; a change a second person approved goes public.
        if found.origin.as_deref() == Some("editor") {
            for (id, published) in &changed {
                if *published {
                    next.private.insert(
                        id.clone(),
                        Private {
                            actor: found.actor.clone(),
                            since: now.clone(),
                        },
                    );
                } else {
                    next.private.remove(id);
                }
            }
        } else {
            for (id, _) in &changed {
                next.private.remove(id);
            }
        }
        next.authorized.insert(manifest.commit.clone(), found);
        let mut floors: std::collections::BTreeMap<String, Policy> = manifest
            .items
            .iter()
            .map(|i| {
                (
                    i.meta.id.clone(),
                    Policy {
                        id: i.meta.id.clone(),
                        section: i.meta.section.clone(),
                        state: i.meta.state.clone(),
                        audience: i.meta.audience.clone(),
                        deleted: false,
                    },
                )
            })
            .collect();
        for (id, policy) in &next.policies {
            floors.entry(id.clone()).or_insert_with(|| Policy {
                deleted: true,
                ..policy.clone()
            });
        }
        next.policies = floors;
    }
    next.active = Some(manifest.id.clone());
    if rollback {
        next.paused = true;
    }
    next.generation += 1;
    next.builds.insert(
        manifest.commit.clone(),
        Build {
            state: BuildState::Live,
            release: Some(manifest.id.clone()),
            reason: None,
        },
    );
    for edit in next.edits.values_mut() {
        if edit.commit.as_deref() == Some(manifest.commit.as_str()) {
            edit.state = EditState::Live;
            edit.release = Some(manifest.id.clone());
        }
    }
    // Recovery eligibility and the live pointer move together.
    next.activated
        .insert(manifest.id.clone(), manifest.commit.clone());
    Ok(respond::json(
        &json!({"state": "live", "release": manifest.id, "generation": state.generation + 1}),
        200,
    ))
}

/// The items whose publication state changed between `live` and `next`, each with whether it is published in `next`.
///
/// Only a story that becomes published counts (OPN-54's "publish"): a correction to a story that was already live,
/// a site-shell change or a new share image alters hashes but publishes nothing, so it never makes a story private.
fn changed_items(live: &Release, next: &Release) -> Vec<(String, bool)> {
    let was_published = |id: &str| {
        live.items
            .iter()
            .any(|old| old.meta.id == id && old.meta.state == ItemState::Published)
    };
    next.items
        .iter()
        .filter_map(|item| {
            let now = item.meta.state == ItemState::Published;
            (now != was_published(&item.meta.id)).then(|| (item.meta.id.clone(), now))
        })
        .collect()
}
