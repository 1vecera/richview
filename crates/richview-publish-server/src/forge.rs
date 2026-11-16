//! The narrow Git host bridge: one fixed repository, branch and content subtree (FEA-BR-26).
//!
//! Every function asks the host for its Git calls through `Ctx::forge`, so a missing answer stops the call with a
//! `Need` and the call replays once the host has it. Nothing here can reach outside the configured repository.
use crate::{
    host::*,
    ledger::{Approval, Edit, EditState},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::{Value, json};

/// The largest source file the browser editor may open or save.
const SOURCE_LIMIT: u64 = 256_000;

/// One file read from the content subtree at an exact revision.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceFile {
    pub source: String,
    pub revision: String,
    pub blob: String,
    pub path: String,
}

/// A browser editorial change: the whole new source of one file, optionally on top of an open change.
#[derive(Clone, Debug)]
pub struct EditRequest {
    /// Relative to the content root: `site.yaml` or `items/<id>/report.rmd`.
    pub path: String,
    /// The head the editor inspected; the branch or the open change must still be at it.
    pub revision: String,
    pub source: String,
    pub title: String,
    pub actor: Principal,
    /// An open change to add a commit to; none opens a new one.
    pub pull: Option<u64>,
}

fn text<'a>(v: &'a Value, path: &[&str]) -> &'a str {
    path.iter()
        .try_fold(v, |v, k| v.get(*k))
        .and_then(Value::as_str)
        .unwrap_or("")
}
fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// Refuses a deployment whose repository, branch or content root could escape the fixed scope.
fn scope(ctx: &Ctx) -> R<()> {
    let word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
    let repository = ctx.cfg.repository.split_once('/').is_some_and(|(o, n)| {
        !o.is_empty() && !n.is_empty() && o.chars().chain(n.chars()).all(word)
    });
    let branch = !ctx.cfg.branch.is_empty() && ctx.cfg.branch.chars().all(|c| word(c) || c == '/');
    let root = &ctx.cfg.content_root;
    let root = !root.is_empty()
        && !root.contains("..")
        && root
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/'));
    if repository && branch && root {
        Ok(())
    } else {
        Err(error(503, "Repository integration unavailable"))
    }
}

/// The repository path of an allowed source file; nothing but the site file and report sources is reachable.
fn path(ctx: &Ctx, relative: &str) -> R<String> {
    let report = relative
        .strip_prefix("items/")
        .and_then(|r| r.strip_suffix("/report.rmd"))
        .is_some_and(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        });
    if relative == "site.yaml" || report {
        Ok(format!("{}/{relative}", ctx.cfg.content_root))
    } else {
        Err(error(403, "Source path is outside the editorial bridge"))
    }
}

fn is_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// The commit the publication branch points at.
pub fn head(ctx: &Ctx) -> R<String> {
    scope(ctx)?;
    let reference = ctx.forge("GET", &format!("/git/ref/heads/{}", ctx.cfg.branch), None)?;
    Ok(text(&reference, &["object", "sha"]).into())
}

/// Whether the canonical repository is private; browser edits and nonpublic releases require it.
pub fn is_private(ctx: &Ctx) -> R<bool> {
    scope(ctx)?;
    Ok(ctx
        .forge("GET", "", None)?
        .get("private")
        .and_then(Value::as_bool)
        == Some(true))
}

/// Reads one source file at `revision`, or at the branch head.
pub fn source(ctx: &Ctx, relative: &str, revision: Option<&str>) -> R<SourceFile> {
    let target = path(ctx, relative)?;
    let revision = match revision.filter(|r| !r.is_empty()) {
        Some(r) => r.to_string(),
        None => head(ctx)?,
    };
    if !is_sha(&revision) {
        return Err(error(400, "Invalid source revision"));
    }
    let file = ctx.forge("GET", &format!("/contents/{target}?ref={revision}"), None)?;
    let size = file.get("size").and_then(Value::as_u64).unwrap_or(u64::MAX);
    if text(&file, &["type"]) != "file"
        || size > SOURCE_LIMIT
        || text(&file, &["encoding"]) != "base64"
    {
        return Err(error(413, "Source exceeds editor limits"));
    }
    let encoded: String = text(&file, &["content"]).split_whitespace().collect();
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| error(502, "Source is not valid base64"))?;
    let source = String::from_utf8(bytes).map_err(|_| error(422, "Source is not valid UTF-8"))?;
    Ok(SourceFile {
        source,
        revision,
        blob: text(&file, &["sha"]).into(),
        path: relative.into(),
    })
}

/// A pull request that targets only the publication branch of this repository and changes only canonical content.
pub fn inspect_pull(ctx: &Ctx, number: u64) -> R<Value> {
    scope(ctx)?;
    if !(1..1 << 53).contains(&number) {
        return Err(error(400, "Invalid pull request"));
    }
    let pr = ctx.forge("GET", &format!("/pulls/{number}"), None)?;
    let changed = pr
        .get("changed_files")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    if text(&pr, &["base", "ref"]) != ctx.cfg.branch
        || text(&pr, &["base", "repo", "full_name"]) != ctx.cfg.repository
        || text(&pr, &["head", "repo", "full_name"]) != ctx.cfg.repository
        || changed > 100
    {
        return Err(error(
            403,
            "Pull request is outside the configured publication",
        ));
    }
    let files = ctx.forge("GET", &format!("/pulls/{number}/files?per_page=100"), None)?;
    let files = list(&files);
    let prefix = format!("{}/", ctx.cfg.content_root);
    let canonical = files.len() as u64 == changed
        && !files.is_empty()
        && files.iter().all(|f| {
            text(f, &["filename"]).starts_with(&prefix)
                && (text(f, &["previous_filename"]).is_empty()
                    || text(f, &["previous_filename"]).starts_with(&prefix))
        });
    if !canonical {
        return Err(error(
            403,
            "A publication pull request may only change canonical content",
        ));
    }
    Ok(pr)
}

/// Who put commits on a pull request: GitHub logins and commit e-mail addresses, lowercase, opener included.
#[derive(Default)]
pub struct Pushers {
    pub logins: Vec<String>,
    pub emails: Vec<String>,
}
impl Pushers {
    /// Whether an approver is one of them, by mapped GitHub login or by e-mail.
    pub fn include(&self, login: Option<&str>, email: Option<&str>) -> bool {
        login.is_some_and(|l| self.logins.contains(&l.to_lowercase()))
            || email.is_some_and(|e| self.emails.contains(&e.to_lowercase()))
    }
}

/// The people behind every commit of a pull request, or `None` when the history is truncated (it then proves nothing).
/// A commit with no linked login still counts by its e-mail address.
pub fn pushers(ctx: &Ctx, number: u64) -> R<Option<Pushers>> {
    let pr = ctx.forge("GET", &format!("/pulls/{number}"), None)?;
    let commits = ctx.forge(
        "GET",
        &format!("/pulls/{number}/commits?per_page=100"),
        None,
    )?;
    if list(&commits).len() >= 100 {
        return Ok(None);
    }
    let mut found = Pushers {
        logins: vec![text(&pr, &["user", "login"]).to_lowercase()],
        emails: vec![],
    };
    for commit in list(&commits) {
        for role in ["author", "committer"] {
            let login = text(commit, &[role, "login"]);
            if !login.is_empty() {
                found.logins.push(login.to_lowercase());
            }
            let email = text(commit, &["commit", role, "email"]);
            if !email.is_empty() {
                found.emails.push(email.to_lowercase());
            }
        }
    }
    Ok(Some(found))
}

/// The approval behind a merged commit: the latest review of an allowed reviewer approved the pull request's exact
/// head, and the reviewer is not its author. No reviewers, a truncated history or no match authorize nothing.
pub fn approved_commit(ctx: &Ctx, commit: &str, reviewers: &[String]) -> R<Option<Approval>> {
    if reviewers.is_empty() {
        return Ok(None);
    }
    scope(ctx)?;
    let pulls = ctx.forge(
        "GET",
        &format!("/commits/{commit}/pulls?per_page=100"),
        None,
    )?;
    for candidate in list(&pulls) {
        let number = candidate.get("number").and_then(Value::as_u64).unwrap_or(0);
        if text(candidate, &["merged_at"]).is_empty()
            || text(candidate, &["merge_commit_sha"]) != commit
            || text(candidate, &["base", "ref"]) != ctx.cfg.branch
        {
            continue;
        }
        let pr = match inspect_pull(ctx, number) {
            Ok(pr) => pr,
            Err(Stop::Need(needs)) => return Err(Stop::Need(needs)),
            Err(_) => continue,
        };
        let reviews = ctx.forge(
            "GET",
            &format!("/pulls/{number}/reviews?per_page=100"),
            None,
        )?;
        if list(&reviews).len() >= 100 {
            continue;
        }
        // Only each reviewer's latest decision counts.
        let mut latest: Vec<&Value> = vec![];
        for review in list(&reviews) {
            if !["APPROVED", "CHANGES_REQUESTED", "DISMISSED"].contains(&text(review, &["state"])) {
                continue;
            }
            latest.retain(|r| text(r, &["user", "login"]) != text(review, &["user", "login"]));
            latest.push(review);
        }
        // A reviewer who put any commit on the change is one of its authors, never its independent approver. A
        // history that is truncated, or has a commit with no linked GitHub login, cannot show independence.
        let commits = ctx.forge(
            "GET",
            &format!("/pulls/{number}/commits?per_page=100"),
            None,
        )?;
        let mut authors: Vec<String> = vec![text(&pr, &["user", "login"]).to_lowercase()];
        let mut known = list(&commits).len() < 100;
        for commit in list(&commits) {
            for role in ["author", "committer"] {
                match text(commit, &[role, "login"]) {
                    "" => known = false,
                    login => authors.push(login.to_lowercase()),
                }
            }
        }
        if !known {
            continue;
        }
        let head = text(&pr, &["head", "sha"]);
        let approval = latest.iter().find(|r| {
            let login = text(r, &["user", "login"]);
            text(r, &["state"]) == "APPROVED"
                && text(r, &["commit_id"]) == head
                && !authors.contains(&login.to_lowercase())
                && reviewers.iter().any(|x| x == login)
        });
        if let Some(r) = approval {
            return Ok(Some(Approval {
                actor: format!("github:{}", text(r, &["user", "login"])),
                pull: number,
                at: None,
                head: Some(head.into()),
                base: None,
                origin: None,
            }));
        }
    }
    Ok(None)
}

/// Merges an open, ready pull request at exactly `head`; the host's branch protections stay in force.
pub fn merge(ctx: &Ctx, number: u64, head: &str) -> R<Value> {
    let pr = inspect_pull(ctx, number)?;
    if text(&pr, &["head", "sha"]) != head
        || text(&pr, &["state"]) != "open"
        || pr.get("draft").and_then(Value::as_bool) == Some(true)
    {
        return Err(error(409, "Pull request changed or is not ready"));
    }
    let result = ctx.forge(
        "PUT",
        &format!("/pulls/{number}/merge"),
        Some(json!({"sha": head, "merge_method": "merge"})),
    )?;
    if result.get("merged").and_then(Value::as_bool) != Some(true) {
        return Err(error(
            409,
            "The Git host refused the merge; repository protections remain in force",
        ));
    }
    Ok(result)
}

/// Commits `source` to a draft branch and opens, or extends, its pull request. The branch only ever moves
/// forward, and only in a private repository.
pub fn edit(ctx: &Ctx, e: EditRequest) -> R<Edit> {
    let target = path(ctx, &e.path)?;
    let existing = e.pull.map(|n| inspect_pull(ctx, n)).transpose()?;
    if let Some(pr) = &existing
        && (text(pr, &["state"]) != "open" || text(pr, &["head", "sha"]) != e.revision)
    {
        return Err(error(
            409,
            "The draft branch changed. Reload and reconcile your edit.",
        ));
    }
    if existing.is_none() && head(ctx)? != e.revision {
        return Err(error(
            409,
            "The publication branch changed. Reload the source and reconcile your edit.",
        ));
    }
    // A protected preview cannot make a publicly committed draft private.
    if !is_private(ctx)? {
        return Err(error(
            403,
            "Browser editorial changes require a private canonical repository",
        ));
    }
    let base = ctx.forge("GET", &format!("/git/commits/{}", e.revision), None)?;
    let blob = ctx.forge(
        "POST",
        "/git/blobs",
        Some(json!({"content": e.source, "encoding": "utf-8"})),
    )?;
    let tree = ctx.forge(
        "POST",
        "/git/trees",
        Some(json!({
            "base_tree": text(&base, &["tree", "sha"]),
            "tree": [{"path": target, "mode": "100644", "type": "blob", "sha": text(&blob, &["sha"])}],
        })),
    )?;
    let email = e
        .actor
        .email
        .clone()
        .unwrap_or_else(|| "publication-service@users.noreply.github.com".into());
    let commit = ctx.forge(
        "POST",
        "/git/commits",
        Some(json!({
            "message": format!("{}\n\nEditorial actor: {}\nInspected revision: {}", e.title, e.actor.key, e.revision),
            "tree": text(&tree, &["sha"]),
            "parents": [e.revision],
            "author": {"name": e.actor.name, "email": email},
        })),
    )?;
    let next = text(&commit, &["sha"]).to_string();
    let made = |number: u64, url: &str, branch: &str| Edit {
        number,
        url: url.into(),
        head: next.clone(),
        branch: branch.into(),
        actor: e.actor.key.clone(),
        git_author: None,
        imported: false,
        state: EditState::PendingReview,
        path: Some(e.path.clone()),
        contributors: vec![e.actor.key.clone()],
        commit: None,
        release: None,
        author_is_editor: false,
    };
    if let Some(pr) = existing {
        let branch = text(&pr, &["head", "ref"]).to_string();
        ctx.forge(
            "PATCH",
            &format!("/git/refs/heads/{branch}"),
            Some(json!({"sha": next, "force": false})),
        )?;
        let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
        return Ok(made(number, text(&pr, &["html_url"]), &branch));
    }
    let branch = format!("publish-edit/{}", ctx.nonce);
    ctx.forge(
        "POST",
        "/git/refs",
        Some(json!({"ref": format!("refs/heads/{branch}"), "sha": next})),
    )?;
    let created = ctx.forge(
        "POST",
        "/pulls",
        Some(json!({
            "title": e.title,
            "head": branch,
            "base": ctx.cfg.branch,
            "draft": false,
            "body": format!(
                "Editorial change to `{target}`.\n\nActor: `{}`\nInspected revision: `{}`\n\nRequires an independent super editor's approval of the current PR head and a successful Publish build. Initial head: `{next}`. Git remains the source of truth.",
                e.actor.key, e.revision
            ),
        })),
    )?;
    let number = created.get("number").and_then(Value::as_u64).unwrap_or(0);
    Ok(made(number, text(&created, &["html_url"]), &branch))
}
