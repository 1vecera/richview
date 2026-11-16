//! The editorial desk under `/_editor/`: approve or reject, the author's quick source edit, urgent hide and restore, roles.
//!
//! The browser offers two actions (FEA-BR-27, INT-26): an editor approves or rejects a pending change on its exact
//! head, and an item's author rewrites its source, which is validated and rendered through the core before it
//! becomes an attributed pull request. Creating items and editing `site.yaml` happen in Git. The rest of this
//! module is the authenticated, non-browser administration the cards require: urgent hide and restore, roles,
//! resume after a rollback, retry of a failed preview, the audit trail and publishing an approved head.
use crate::{
    access::{Access, RoleName, can_edit, in_scope, same_actor},
    forge,
    host::*,
    ledger::{Approval, BuildState, Edit, EditState, Hide},
    model::{Item, Release, identifier},
    respond,
};
use serde_json::{Value, json};

/// The largest source the desk reads or saves.
const SOURCE_LIMIT: usize = 256_000;
const REASON_LIMIT: usize = 500;

pub fn handle(ctx: &mut Ctx, req: &Request, access: &Access) -> R<Response> {
    if !access.is_editor() {
        return Err(error(403, "Editorial access is required"));
    }
    let path = req.path.as_str();
    if req.is_read() {
        return match path {
            // The embedded editor page and its script belong to the host adapter's bundle, not to the rules.
            "/_editor/" | "/_editor/app.js" => Err(error(404, "Not found")),
            "/_editor/api/session" => session(ctx, access),
            "/_editor/api/items" => items(ctx, access),
            "/_editor/api/source" => read_source(ctx, req, access),
            "/_editor/api/audit" => audit(ctx, access),
            _ => Err(error(404, "Unknown editorial action")),
        };
    }
    if req.method != "POST" {
        return Err(error(405, "Method not allowed"));
    }
    let input: Value = req.json(300_000)?;
    match path {
        "/_editor/api/resume" => resume(ctx, access, &input),
        "/_editor/api/retry" => retry(ctx, access, &input),
        "/_editor/api/edit" => edit(ctx, access, &input),
        "/_editor/api/hide" => hide(ctx, access, &input),
        "/_editor/api/restore" => restore(ctx, access, &input),
        "/_editor/api/approve" => approve(ctx, access, &input),
        "/_editor/api/reject" => reject(ctx, access, &input),
        "/_editor/api/publish" => publish(ctx, access, &input),
        "/_editor/api/roles" => roles(ctx, access, &input),
        _ => Err(error(404, "Unknown editorial action")),
    }
}

fn super_editor(access: &Access) -> R<()> {
    if access.name == RoleName::SuperEditor {
        Ok(())
    } else {
        Err(error(403, "A super editor is required"))
    }
}
fn principal<'a>(ctx: &Ctx<'a>) -> R<&'a Principal> {
    ctx.principal
        .ok_or_else(|| error(401, "Authentication required."))
}
fn same(ctx: &Ctx, left: &str, right: &str) -> bool {
    same_actor(left, right, ctx.cfg, &ctx.state.roles)
}
fn is_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}
fn text<'a>(input: &'a Value, key: &str) -> &'a str {
    input.get(key).and_then(Value::as_str).unwrap_or("")
}
fn number(input: &Value, key: &str) -> Option<u64> {
    input.get(key).and_then(Value::as_u64).filter(|n| *n > 0)
}
fn reason(input: &Value) -> R<String> {
    let reason = text(input, "reason").trim();
    if reason.is_empty() {
        return Err(error(400, "A reason is required"));
    }
    Ok(reason.chars().take(REASON_LIMIT).collect())
}
fn said(status: u16, message: &str) -> R<Response> {
    Ok(respond::json(&json!({ "error": message }), status))
}

/// The active release, or none before the first activation.
fn active(ctx: &Ctx) -> R<Option<Release>> {
    let Some(id) = &ctx.state.active else {
        return Ok(None);
    };
    serde_json::from_str(&ctx.blob_text(&format!("releases/{id}.json"))?)
        .map(Some)
        .map_err(|_| error(503, "Release object unavailable"))
}
fn find<'a>(release: &'a Option<Release>, id: &str) -> Option<&'a Item> {
    release
        .as_ref()
        .and_then(|r| r.items.iter().find(|i| i.meta.id == id))
}
/// The item a source path belongs to: `items/<id>/report.rmd`.
fn item_of(path: &str) -> Option<&str> {
    path.strip_prefix("items/")?.strip_suffix("/report.rmd")
}

/// The publication metadata of a source, with the fields the desk relies on.
struct Meta {
    id: String,
    section: String,
    audience: String,
    state: String,
    byline: String,
}
/// Reads `publish` from loaded frontmatter; the same patterns the builder enforces.
fn metadata(frontmatter: &Value) -> R<Meta> {
    let publish = frontmatter
        .get("publish")
        .filter(|p| p.is_object())
        .ok_or_else(|| error(400, "Publication metadata is required"))?;
    let field = |key: &str| publish.get(key).and_then(Value::as_str).unwrap_or("");
    if !["id", "slug", "section", "audience"]
        .iter()
        .all(|k| identifier(field(k)))
        || !["draft", "published", "hidden"].contains(&field("state"))
        || !frontmatter.get("title").is_some_and(Value::is_string)
    {
        return Err(error(400, "Invalid publication metadata"));
    }
    Ok(Meta {
        id: field("id").into(),
        section: field("section").into(),
        audience: field("audience").into(),
        state: field("state").into(),
        byline: field("byline").into(),
    })
}
/// Loads the frontmatter of a source the Git host returned; a source the core cannot read is refused.
fn frontmatter(source: &str) -> R<Value> {
    if source.len() > SOURCE_LIMIT {
        return Err(error(400, "Source exceeds the editor limit"));
    }
    richview_core::document::parse(source)
        .map(|d| d.frontmatter)
        .map_err(|_| error(400, "Rich Markdown needs valid YAML frontmatter"))
}

/// What an edit must pass before it is saved: the core reads it and renders it without an error. Failures come
/// back as the core's diagnostics, unchanged (ARC-08).
fn check_source(source: &str) -> R<Result<Value, Vec<richview_core::document::Diagnostic>>> {
    if source.len() > SOURCE_LIMIT {
        return Err(error(400, "Source exceeds the editor limit"));
    }
    let document = match richview_core::document::parse(source) {
        Ok(document) => document,
        Err(diagnostics) => return Ok(Err(diagnostics)),
    };
    if document.diagnostics.iter().any(|d| d.level == "error") {
        return Ok(Err(document.diagnostics));
    }
    let frontmatter = document.frontmatter.clone();
    // The server holds one file; the folder it hands the core lists the files the report refers to, so the check
    // judges the source and not what a Git checkout would add.
    let mut files = vec!["report.rmd".to_string()];
    files.extend(
        richview_core::document::report_resources(&document)
            .into_iter()
            .map(|r| r.path),
    );
    let options = richview_core::document::RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "report".into(),
            source: "report.rmd".into(),
            files,
        }),
        ..Default::default()
    };
    match richview_core::document::render_document(document, &options) {
        Err(diagnostics) => Ok(Err(diagnostics)),
        Ok(report) if report.diagnostics.iter().any(|d| d.level == "error") => {
            Ok(Err(report.diagnostics))
        }
        Ok(_) => Ok(Ok(frontmatter)),
    }
}

/// Sets `key: value` under `publish:` in the frontmatter, keeping everything else byte for byte.
fn set_publish(source: &str, fields: &[(&str, &str)]) -> R<String> {
    let bad = || error(400, "Rich Markdown needs YAML frontmatter");
    let mut lines: Vec<String> = source.split_inclusive('\n').map(String::from).collect();
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return Err(bad());
    }
    let end = lines
        .iter()
        .skip(1)
        .position(|l| l.trim_end() == "---")
        .map(|i| i + 1)
        .ok_or_else(bad)?;
    let start = lines[1..end]
        .iter()
        .position(|l| l.trim_end() == "publish:")
        .map(|i| i + 1)
        .ok_or_else(|| error(400, "Publication metadata is required"))?;
    let block = lines[start + 1..end]
        .iter()
        .position(|l| !l.trim().is_empty() && !l.starts_with([' ', '\t']))
        .map_or(end, |i| start + 1 + i);
    let newline = if lines[start].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    for (key, value) in fields {
        let found = (start + 1..block).find(|&i| {
            lines[i].trim_start().starts_with(&format!("{key}:"))
                && lines[i].len() - lines[i].trim_start().len() <= 4
        });
        match found {
            Some(i) => {
                let indent = &lines[i][..lines[i].len() - lines[i].trim_start().len()];
                let quoted = lines[i].trim_start()[key.len() + 1..]
                    .trim_start()
                    .starts_with('"');
                let rendered = if quoted {
                    format!("\"{value}\"")
                } else {
                    (*value).into()
                };
                lines[i] = format!("{indent}{key}: {rendered}{newline}");
            }
            None => {
                let indent = lines
                    .get(start + 1)
                    .filter(|_| block > start + 1)
                    .map_or("  ", |l| &l[..l.len() - l.trim_start().len()])
                    .to_string();
                lines.insert(start + 1, format!("{indent}{key}: {value}{newline}"));
            }
        }
    }
    Ok(lines.concat())
}

fn session(ctx: &Ctx, access: &Access) -> R<Response> {
    // A Git outage must not lock editors out of the desk; the revision is simply unknown.
    let revision = match forge::head(ctx) {
        Ok(head) => Some(head),
        Err(Stop::Need(needs)) => return Err(Stop::Need(needs)),
        Err(Stop::Http(_)) => None,
    };
    Ok(respond::json(
        &json!({
            "actor": ctx.actor_key(),
            "role": access,
            "revision": revision,
            "release": ctx.state.active,
            "generation": ctx.state.generation,
            "paused": ctx.state.paused,
        }),
        200,
    ))
}

fn items(ctx: &Ctx, access: &Access) -> R<Response> {
    let release = active(ctx)?;
    let me = ctx.actor_key();
    let rows: Vec<Value> = release
        .iter()
        .flat_map(|r| &r.items)
        .filter(|i| can_edit(access, &i.meta.section, &i.meta.audience))
        .map(|i| {
            let state = if ctx.state.hidden.contains_key(&i.meta.id) {
                "hidden".to_string()
            } else if i.meta.state == crate::model::State::Published {
                "live".into()
            } else {
                serde_json::to_value(&i.meta.state)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
            };
            json!({
                "id": i.meta.id, "title": i.title, "section": i.meta.section,
                "audience": i.meta.audience, "source": i.source, "state": state,
                "private": ctx.state.private.contains_key(&i.meta.id),
            })
        })
        .collect();
    // A change is visible to its authors, to super editors, and to editors in scope of its item.
    let changes: Vec<Value> = ctx
        .state
        .edits
        .values()
        .filter(|e| {
            access.name == RoleName::SuperEditor
                || same(ctx, &e.actor, &me)
                || e.path
                    .as_deref()
                    .and_then(item_of)
                    .and_then(|id| find(&release, id))
                    .is_some_and(|i| can_edit(access, &i.meta.section, &i.meta.audience))
        })
        .map(|e| {
            let mut value = serde_json::to_value(e).unwrap_or(Value::Null);
            value["validation"] =
                serde_json::to_value(ctx.state.builds.get(&e.head)).unwrap_or(Value::Null);
            value
        })
        .collect();
    Ok(respond::json(
        &json!({ "items": rows, "changes": changes, "builds": ctx.state.builds }),
        200,
    ))
}

fn read_source(ctx: &Ctx, req: &Request, access: &Access) -> R<Response> {
    if let Some(pull) = req.query.get("pull").and_then(|p| p.parse::<u64>().ok()) {
        let change = ctx.state.edits.get(&pull);
        let Some(path) = change.and_then(|c| c.path.clone()) else {
            return Err(error(404, "This change must be edited in Git"));
        };
        let pr = forge::inspect_pull(ctx, pull)?;
        let head = pr["head"]["sha"].as_str().unwrap_or("");
        let file = forge::source(ctx, &path, Some(head))?;
        let meta = metadata(&frontmatter(&file.source)?)?;
        if !can_edit(access, &meta.section, &meta.audience) {
            return Err(error(403, "Draft is outside your scope"));
        }
        return Ok(respond::json(
            &json!({"source": file.source, "revision": file.revision, "blob": file.blob, "path": file.path, "pull": pull}),
            200,
        ));
    }
    let release = active(ctx)?;
    let item = find(&release, req.query.get("id").map_or("", String::as_str))
        .filter(|i| can_edit(access, &i.meta.section, &i.meta.audience))
        .ok_or_else(|| error(404, "Item unavailable in your scope"))?;
    let revision = req.query.get("revision").map(String::as_str);
    if revision.is_some_and(|r| !is_revision(r)) {
        return Err(error(400, "Choose a full commit as the source revision"));
    }
    let file = forge::source(ctx, &item.source, revision)?;
    Ok(respond::json(
        &json!({"source": file.source, "revision": file.revision, "blob": file.blob, "path": file.path}),
        200,
    ))
}

fn audit(ctx: &Ctx, access: &Access) -> R<Response> {
    super_editor(access)?;
    match &ctx.io.audit {
        Some(entries) => Ok(respond::json(entries, 200)),
        None => Err(Stop::Need(vec![Need::Audit { limit: 100 }])),
    }
}

fn resume(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    super_editor(access)?;
    let reason = reason(input)?;
    ctx.next.paused = false;
    ctx.change("publication.resumed", json!({ "reason": reason }));
    Ok(respond::json(&json!({ "paused": false }), 200))
}

fn retry(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    let me = ctx.actor_key();
    let n = number(input, "number").unwrap_or(0);
    let change = ctx.state.edits.get(&n).cloned();
    let Some(change) =
        change.filter(|c| access.name == RoleName::SuperEditor || same(ctx, &c.actor, &me))
    else {
        return Err(error(403, "This change is outside your scope"));
    };
    let pr = forge::inspect_pull(ctx, n)?;
    let failed = ctx
        .state
        .builds
        .get(&change.head)
        .is_some_and(|b| b.state == BuildState::Failed);
    if pr["state"] != "open" || pr["head"]["sha"] != change.head.as_str() || !failed {
        return Err(error(409, "Only the current failed preview can be retried"));
    }
    ctx.next.builds.remove(&change.head);
    ctx.change(
        "build.retry.requested",
        json!({ "pull": n, "head": change.head }),
    );
    Ok(respond::json(&json!({ "state": "pending review" }), 200))
}

/// Whether the signed-in editor is the author of the item whose source is `path`: named in its byline or the
/// author of an earlier desk change to it.
fn is_author(ctx: &Ctx, principal: &Principal, byline: &str, path: &str) -> bool {
    let byline = byline.trim();
    let mapped = ctx
        .state
        .roles
        .get(&principal.key)
        .and_then(|r| r.byline.as_deref());
    (!byline.is_empty()
        && [
            Some(principal.name.as_str()),
            principal.email.as_deref(),
            mapped,
        ]
        .into_iter()
        .flatten()
        .any(|n| n.trim().eq_ignore_ascii_case(byline)))
        || ctx
            .state
            .edits
            .values()
            .any(|e| e.path.as_deref() == Some(path) && same(ctx, &e.actor, &principal.key))
}

/// The author's quick source edit: validate, render, then open or update an attributed pull request.
fn edit(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    let me = principal(ctx)?;
    let revision = text(input, "revision");
    if !is_revision(revision) {
        return Err(error(400, "Reload the inspected source revision"));
    }
    let path = text(input, "path");
    let source = text(input, "source");
    let pull = number(input, "pull");
    let known = pull.and_then(|n| ctx.state.edits.get(&n).cloned());
    if pull.is_some() && known.as_ref().and_then(|c| c.path.as_deref()) != Some(path) {
        return Err(error(403, "Draft path differs from its inspected change"));
    }
    let next = match check_source(source)? {
        Ok(frontmatter) => metadata(&frontmatter)?,
        Err(diagnostics) => {
            return Ok(respond::json(
                &json!({"error": "The edited report does not validate, so it was not saved.", "diagnostics": diagnostics}),
                422,
            ));
        }
    };
    if !can_edit(access, &next.section, &next.audience)
        || path != format!("items/{}/report.rmd", next.id)
    {
        return Err(error(403, "New metadata is outside your editorial scope"));
    }
    let prior = forge::source(ctx, path, Some(revision))?;
    let previous = metadata(&frontmatter(&prior.source)?)?;
    if !can_edit(access, &previous.section, &previous.audience) || previous.id != next.id {
        return Err(error(403, "Stable identity and scope must be preserved"));
    }
    if !is_author(ctx, me, &previous.byline, path) {
        return Err(error(
            403,
            "Only an item's author may edit it in the browser",
        ));
    }
    if (previous.state == "hidden" || ctx.state.hidden.contains_key(&previous.id))
        && next.state != "hidden"
        && access.name != RoleName::SuperEditor
    {
        return Err(error(
            403,
            "Only a super editor may propose restoring hidden content",
        ));
    }
    // A protected preview cannot make a publicly committed draft private.
    if !forge::is_private(ctx)? {
        return Err(error(
            403,
            "Browser editorial changes require a private canonical repository",
        ));
    }
    let mut change = forge::edit(
        ctx,
        forge::EditRequest {
            path: path.into(),
            revision: revision.into(),
            source: source.into(),
            title: format!("Edit {}", next.id),
            actor: me.clone(),
            pull,
        },
    )?;
    record_change(ctx, &mut change, known, true);
    ctx.change(
        "editor.change",
        json!({ "pull": change.number, "head": change.head, "path": path }),
    );
    Ok(respond::json(&change, 201))
}

/// Stores `change` in the ledger. A new head loses the approval of the old one and keeps every earlier contributor.
fn record_change(ctx: &mut Ctx, change: &mut Edit, known: Option<Edit>, author_is_editor: bool) {
    let mut contributors = known
        .as_ref()
        .map(|k| k.contributors.clone())
        .unwrap_or_default();
    for actor in known
        .iter()
        .map(|k| k.actor.clone())
        .chain([ctx.actor_key()])
    {
        if !contributors.contains(&actor) {
            contributors.push(actor);
        }
    }
    change.contributors = contributors;
    change.author_is_editor = known
        .as_ref()
        .map_or(author_is_editor, |k| k.author_is_editor);
    if let Some(previous) = &known {
        ctx.next.forget_approval(&previous.head);
        change.actor = previous.actor.clone();
    }
    ctx.next.edits.insert(change.number, change.clone());
}

/// Urgent hide (FEA-BR-28). The first request records the hide in the ledger and answers 202, so hosted
/// access is withdrawn whatever Git does; a repeated request reconciles Git with a pull request, and a Git
/// failure leaves the hide in force and reconciliation pending.
fn hide(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    let me = principal(ctx)?;
    let release = active(ctx)?;
    let id = text(input, "id");
    let item = find(&release, id)
        .filter(|i| can_edit(access, &i.meta.section, &i.meta.audience))
        .ok_or_else(|| error(403, "Item is outside your scope"))?;
    let reason = reason(input)?;
    let pending =
        "Hosted access is withdrawn. Git reconciliation needs a retry from the editorial desk.";
    let Some(hidden) = ctx.state.hidden.get(id).cloned() else {
        ctx.next.hidden.insert(
            id.into(),
            Hide {
                revision: ctx.nonce.into(),
                actor: me.key.clone(),
                reason: reason.clone(),
                at: ctx.now.into(),
                reconciliation: "pending".into(),
                pull: None,
            },
        );
        ctx.change("item.hidden", json!({ "id": id, "reason": reason }));
        return Ok(respond::json(
            &json!({"state": "hidden", "reconciliation": "pending", "message": pending}),
            202,
        ));
    };
    if hidden.reconciliation != "pending" {
        return Ok(respond::json(
            &json!({"state": "hidden", "reconciliation": hidden.reconciliation, "pull": hidden.pull}),
            200,
        ));
    }
    let reconciled = (|| {
        let current = forge::source(ctx, &item.source, None)?;
        let source = set_publish(
            &current.source,
            &[("state", "hidden"), ("updated", ctx.now)],
        )?;
        forge::edit(
            ctx,
            forge::EditRequest {
                path: item.source.clone(),
                revision: current.revision,
                source,
                title: format!("Hide {}", item.title),
                actor: me.clone(),
                pull: None,
            },
        )
    })();
    match reconciled {
        Err(Stop::Need(needs)) => Err(Stop::Need(needs)),
        // Hosting is already revoked; a Git failure cannot undo the hide.
        Err(Stop::Http(_)) => Ok(respond::json(
            &json!({"state": "hidden", "reconciliation": "pending", "message": pending}),
            202,
        )),
        Ok(mut change) => {
            record_change(ctx, &mut change, None, true);
            if let Some(h) = ctx.next.hidden.get_mut(id) {
                h.reconciliation = change.url.clone();
                h.pull = Some(change.number);
            }
            ctx.change(
                "hide.reconciled",
                json!({ "id": id, "pull": change.number }),
            );
            Ok(respond::json(
                &json!({"state": "hidden", "change": change}),
                200,
            ))
        }
    }
}

fn restore(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    super_editor(access)?;
    let me = ctx.actor_key();
    let release = active(ctx)?;
    let id = text(input, "id");
    let hidden = ctx.state.hidden.get(id).cloned();
    let (Some(hidden), Some(item), false) = (
        hidden,
        find(&release, id),
        text(input, "reason").trim().is_empty(),
    ) else {
        return Err(error(400, "An active hide and reason are required"));
    };
    if same(ctx, &hidden.actor, &me) {
        return Err(error(
            403,
            "A different super editor must authorize restoration",
        ));
    }
    let current = metadata(&frontmatter(
        &forge::source(ctx, &item.source, None)?.source,
    )?)?;
    if current.state != "published" || item.meta.state != crate::model::State::Published {
        return Err(error(
            409,
            "First approve and publish the Git restoration; the runtime hide stays in force",
        ));
    }
    if let Some(pull) = hidden.pull
        && forge::inspect_pull(ctx, pull)?["state"] == "open"
    {
        ctx.forge(
            "PATCH",
            &format!("/pulls/{pull}"),
            Some(json!({"state": "closed"})),
        )?;
    }
    let reason = reason(input)?;
    ctx.next.hidden.remove(id);
    if let Some(edit) = hidden.pull.and_then(|p| ctx.next.edits.get_mut(&p)) {
        edit.state = EditState::Closed;
    }
    ctx.change("item.restored", json!({ "id": id, "reason": reason }));
    Ok(respond::json(&json!({"state": "live", "id": id}), 200))
}

/// Why `me` may not approve `change`: its authors and contributors never approve it.
fn independent(ctx: &Ctx, change: &Edit, me: &str) -> bool {
    !same(ctx, &change.actor, me) && !change.contributors.iter().any(|c| same(ctx, c, me))
}

/// Approves a pending change from an author who is not an editor, or an editor's private item (OPN-54).
fn approve(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    let Some(n) = number(input, "number") else {
        return approve_item(ctx, access, text(input, "id"));
    };
    let me = ctx.actor_key();
    let change = ctx.state.edits.get(&n).cloned().ok_or_else(|| {
        error(
            409,
            "Import this pull request through the editorial desk before approval",
        )
    })?;
    if change.author_is_editor {
        return Err(error(
            409,
            "An editor's own change is published by its author; another editor then approves the item",
        ));
    }
    if !independent(ctx, &change, &me) {
        return Err(error(403, "You cannot approve your own publication change"));
    }
    let pr = forge::inspect_pull(ctx, n)?;
    let login = pr["user"]["login"].as_str().unwrap_or("");
    if (change.imported || change.actor.starts_with("github:"))
        && access
            .github_login
            .as_deref()
            .is_none_or(|l| l.eq_ignore_ascii_case(login))
    {
        return Err(error(
            403,
            "An independently mapped GitHub reviewer is required for imported pull requests",
        ));
    }
    let head = pr["head"]["sha"].as_str().unwrap_or("");
    if head != text(input, "head") || head != change.head || pr["state"] != "open" {
        return Err(error(
            409,
            "The pull request changed; inspect its current revision",
        ));
    }
    in_review_scope(ctx, access, &change)?;
    require_built(ctx, head)?;
    // Anyone who put a commit on the change in Git is one of its authors, whatever the desk has recorded.
    let email = ctx.principal.and_then(|p| p.email.as_deref());
    match forge::pushers(ctx, n)? {
        Some(found) if !found.include(access.github_login.as_deref(), email) => {}
        _ => {
            return Err(error(
                403,
                "A reviewer who pushed a commit to the change cannot approve it",
            ));
        }
    }
    let approval = Approval {
        actor: me,
        pull: n,
        at: Some(ctx.now.into()),
        head: Some(head.into()),
        base: None,
        origin: None,
    };
    ctx.next.approvals.insert(head.into(), approval.clone());
    if let Some(edit) = ctx.next.edits.get_mut(&n) {
        edit.state = EditState::Approved;
    }
    ctx.change("change.approved", json!({ "pull": n, "head": head }));
    // An approved submission goes public: merge it now. A merge that fails leaves the approval in force.
    match merge_change(ctx, n, head, approval) {
        Err(Stop::Http(e)) => Ok(respond::json(
            &json!({"error": e.message, "state": "approved", "head": head}),
            e.status,
        )),
        done => done,
    }
}

/// The approver must be in scope of the item the change touches; a change without a path needs a super editor.
fn in_review_scope(ctx: &Ctx, access: &Access, change: &Edit) -> R<()> {
    let Some(path) = &change.path else {
        return super_editor(access);
    };
    let file = forge::source(ctx, path, Some(&change.head))?;
    let meta = metadata(&frontmatter(&file.source)?)?;
    if can_edit(access, &meta.section, &meta.audience) {
        Ok(())
    } else {
        Err(error(403, "This change is outside your scope"))
    }
}
fn require_built(ctx: &Ctx, head: &str) -> R<()> {
    match ctx.state.builds.get(head) {
        Some(b) if b.state == BuildState::Built => Ok(()),
        _ => Err(error(
            409,
            "A successful protected preview build is required",
        )),
    }
}

/// An editor's own private item becomes public when an editor in scope, its author included, says so (OPN-54).
fn approve_item(ctx: &mut Ctx, access: &Access, id: &str) -> R<Response> {
    let release = active(ctx)?;
    let item = find(&release, id)
        .filter(|i| can_edit(access, &i.meta.section, &i.meta.audience))
        .ok_or_else(|| error(404, "Item unavailable in your scope"))?;
    if !ctx.state.private.contains_key(id) {
        return Err(error(
            409,
            "This item is not waiting for an editor's approval",
        ));
    }
    ctx.next.private.remove(id);
    ctx.change("item.published", json!({ "id": item.meta.id }));
    Ok(respond::json(&json!({"state": "live", "id": id}), 200))
}

fn reject(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    let n = number(input, "number").unwrap_or(0);
    let change = ctx
        .state
        .edits
        .get(&n)
        .cloned()
        .ok_or_else(|| error(404, "Unknown change"))?;
    let pr = forge::inspect_pull(ctx, n)?;
    let head = pr["head"]["sha"].as_str().unwrap_or("");
    if head != text(input, "head") || head != change.head || pr["state"] != "open" {
        return Err(error(
            409,
            "The pull request changed; inspect its current revision",
        ));
    }
    in_review_scope(ctx, access, &change)?;
    let reason: String = text(input, "reason").chars().take(REASON_LIMIT).collect();
    ctx.forge(
        "PATCH",
        &format!("/pulls/{n}"),
        Some(json!({"state": "closed"})),
    )?;
    ctx.next.approvals.remove(head);
    if let Some(edit) = ctx.next.edits.get_mut(&n) {
        edit.state = EditState::Closed;
    }
    ctx.change(
        "change.rejected",
        json!({ "pull": n, "head": head, "reason": reason }),
    );
    Ok(respond::json(
        &json!({"state": "closed", "head": head}),
        200,
    ))
}

/// Merges an approved head, or an editor's own change, and authorizes the merge commit for the publisher.
fn publish(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    let me = ctx.actor_key();
    let n = number(input, "number").unwrap_or(0);
    let head = text(input, "head");
    let change = ctx.state.edits.get(&n).cloned();
    let Some(change) = change.filter(|c| c.head == head) else {
        return Err(error(
            403,
            "Independent approval of this exact head is required",
        ));
    };
    let approval = if change.author_is_editor {
        // The editor's own change needs no second approver; the item lands private (OPN-54).
        if !same(ctx, &change.actor, &me) {
            return Err(error(
                403,
                "Only the author may publish an editor's own change",
            ));
        }
        in_review_scope(ctx, access, &change)?;
        Approval {
            actor: me,
            pull: n,
            at: Some(ctx.now.into()),
            head: Some(head.into()),
            base: None,
            origin: Some("editor".into()),
        }
    } else {
        let approval = ctx.state.approvals.get(head).cloned();
        match approval.filter(|a| a.pull == n) {
            Some(a) if independent(ctx, &change, &me) => a,
            _ => {
                return Err(error(
                    403,
                    "Independent approval of this exact head is required",
                ));
            }
        }
    };
    require_built(ctx, head)?;
    merge_change(ctx, n, head, approval)
}

fn merge_change(ctx: &mut Ctx, n: u64, head: &str, approval: Approval) -> R<Response> {
    let release = active(ctx)?;
    let base = forge::head(ctx)?;
    if release.as_ref().is_none_or(|r| r.commit != base) {
        return Err(error(
            409,
            "Wait for the current approved publication to become live, or recover the canonical branch before merging another change.",
        ));
    }
    let merged = forge::merge(ctx, n, head)?;
    let commit = merged["sha"]
        .as_str()
        .ok_or_else(|| error(502, "Git host returned no merge commit"))?
        .to_string();
    let parent = ctx.forge("GET", &format!("/git/commits/{commit}"), None)?;
    if parent["parents"][0]["sha"] != base.as_str() {
        if let Some(edit) = ctx.next.edits.get_mut(&n) {
            edit.state = EditState::Failed;
            edit.commit = Some(commit.clone());
        }
        ctx.change("change.merge.held", json!({ "pull": n, "commit": commit }));
        return said(
            409,
            "Git changed during the merge. The merged commit is held and has not been authorized for publication.",
        );
    }
    ctx.next.authorized.insert(
        commit.clone(),
        Approval {
            head: Some(head.into()),
            base: Some(base),
            ..approval
        },
    );
    if let Some(edit) = ctx.next.edits.get_mut(&n) {
        edit.state = EditState::Building;
        edit.commit = Some(commit.clone());
    }
    ctx.change(
        "change.merged",
        json!({ "pull": n, "head": head, "commit": commit }),
    );
    Ok(respond::json(
        &json!({
            "state": "building", "commit": commit,
            "message": "The canonical branch changed. The configured publisher will build and activate this commit.",
        }),
        200,
    ))
}

fn roles(ctx: &mut Ctx, access: &Access, input: &Value) -> R<Response> {
    super_editor(access)?;
    let me = ctx.actor_key();
    let identity = text(input, "identity");
    if !ctx.cfg.publisher_identity.is_empty() && identity == ctx.cfg.publisher_identity {
        return Err(error(
            403,
            "The publishing service cannot receive an editorial role",
        ));
    }
    if !verified_identity(identity) {
        return Err(error(400, "Choose a verified identity"));
    }
    let name = match text(input, "role") {
        "reader" => RoleName::Reader,
        "editor" => RoleName::Editor,
        "super editor" => RoleName::SuperEditor,
        _ => return Err(error(400, "Invalid role or scope")),
    };
    let list = |key: &str| -> R<Vec<String>> {
        input
            .get(key)
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .map(|v| v.as_str().map(String::from))
                    .collect::<Option<Vec<_>>>()
            })
            .filter(|l| l.iter().all(|v| v == "*" || scope_id(v)))
            .ok_or_else(|| error(400, "Invalid role or scope"))
    };
    let (sections, audiences) = (list("sections")?, list("audiences")?);
    if identity == me {
        let other_owner = ctx.state.roles.iter().any(|(key, role)| {
            *key != me
                && *key != ctx.cfg.publisher_identity
                && role.name == RoleName::SuperEditor
                && in_scope(role, "*")
                && role.audiences.iter().any(|a| a == "*")
        });
        if name != RoleName::Reader || !sections.is_empty() || !audiences.is_empty() || !other_owner
        {
            return Err(error(
                400,
                "Self-service role changes may only disable your own access, with another full-scope super editor already assigned.",
            ));
        }
    }
    let github_login = input
        .get("github_login")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty() && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
        .map(String::from);
    let byline = input
        .get("byline")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|b| !b.is_empty() && b.chars().count() <= 120 && !b.contains(['<', '>']))
        .map(String::from);
    ctx.next.roles.insert(
        identity.into(),
        Access {
            name,
            sections: sections.clone(),
            audiences: audiences.clone(),
            github_login,
            byline,
        },
    );
    ctx.next.revoke_approvals_of(identity);
    ctx.change(
        "role.updated",
        json!({"identity": identity, "role": text(input, "role"), "sections": sections, "audiences": audiences}),
    );
    Ok(respond::json(&json!({ "saved": true }), 200))
}

/// `email:<address>` or `service:<name>`; the adapter verified the identity, the desk only checks its shape.
fn verified_identity(identity: &str) -> bool {
    if let Some(address) = identity.strip_prefix("email:") {
        let mut parts = address.split('@');
        let (user, domain, rest) = (parts.next(), parts.next(), parts.next());
        return rest.is_none()
            && user.is_some_and(|u| !u.is_empty() && !u.contains(char::is_whitespace))
            && domain.is_some_and(|d| !d.is_empty() && !d.contains(char::is_whitespace));
    }
    identity.strip_prefix("service:").is_some_and(|name| {
        !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-'))
    })
}
/// A section or audience id, as in the item metadata.
fn scope_id(value: &str) -> bool {
    identifier(value)
}
