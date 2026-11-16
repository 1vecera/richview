//! The editorial desk against an in-memory host: a fake Git host answers the forge needs, and every call loops
//! on `Outcome::Need` the way an adapter does.
use richview_publish_server::{access::*, handle, host::*, ledger::*};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::rc::Rc;

const BASE: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const SOURCE: &str = "items/story/report.rmd";

fn cfg() -> Config {
    Config {
        publication_host: "news.example".into(),
        repository: "o/r".into(),
        branch: "publish/news".into(),
        content_root: "publications/news".into(),
        publisher_identity: "service:publisher".into(),
        bootstrap_super_editor: "email:owner@example.com".into(),
        ..Config::default()
    }
}
fn who(key: &str) -> Principal {
    let local = key.split(['@', ':']).nth(1).unwrap_or(key);
    let name = match local {
        "ed" => "Ed One".to_string(),
        other => other.to_string(),
    };
    Principal {
        key: key.into(),
        name,
        email: key.strip_prefix("email:").map(String::from),
        service: false,
    }
}
fn role(name: RoleName, sections: &[&str], audiences: &[&str]) -> Access {
    Access {
        name,
        sections: sections.iter().map(|s| s.to_string()).collect(),
        audiences: audiences.iter().map(|s| s.to_string()).collect(),
        github_login: None,
        byline: None,
    }
}

fn article(state: &str, byline: &str, body: &str) -> String {
    format!(
        "---\ntitle: \"Story\"\ndescription: \"A story.\"\nversion: 1.0.0\npublish:\n  id: \"story\"\n  slug: \"story\"\n  summary: \"s\"\n  byline: \"{byline}\"\n  section: \"news\"\n  tags: []\n  published: \"2026-10-14T09:00:00Z\"\n  updated: \"2026-10-18T09:00:00Z\"\n  state: \"{state}\"\n  audience: \"public\"\n  kind: \"report\"\n---\n\n{body}\n"
    )
}

/// A fake Git host: just enough of the Git API for the bridge's calls.
struct Git {
    head: String,
    private: bool,
    down: bool,
    /// Parent the next merge commit reports; `None` is the branch head the merge started from.
    merge_parent: Option<String>,
    sources: BTreeMap<(String, String), String>,
    pulls: BTreeMap<u64, Value>,
    commits: BTreeMap<String, (String, String)>,
    blobs: BTreeMap<String, String>,
    counter: u64,
    writes: Vec<String>,
    /// Commits on a pull request as the Git host lists them; absent means one commit by its opener.
    pushed: BTreeMap<u64, Vec<Value>>,
}
impl Git {
    fn new() -> Self {
        let mut git = Git {
            head: BASE.into(),
            private: true,
            down: false,
            merge_parent: None,
            sources: BTreeMap::new(),
            pulls: BTreeMap::new(),
            commits: BTreeMap::new(),
            blobs: BTreeMap::new(),
            counter: 0,
            writes: vec![],
            pushed: BTreeMap::new(),
        };
        git.sources.insert(
            (BASE.into(), SOURCE.into()),
            article("published", "Ed One", "Hello **world**."),
        );
        git
    }
    fn sha(&mut self) -> String {
        self.counter += 1;
        format!("{:040x}", 0xc0ffee00u64 + self.counter)
    }
    fn open_pull(&mut self, number: u64, head: &str, rel: &str, source: &str) {
        self.sources
            .insert((head.into(), rel.into()), source.into());
        self.pulls.insert(
            number,
            json!({
                "number": number, "state": "open", "draft": false, "changed_files": 1,
                "user": {"login": "someone"}, "html_url": format!("https://git/pull/{number}"),
                "base": {"ref": "publish/news", "repo": {"full_name": "o/r"}},
                "head": {"sha": head, "ref": format!("publish-edit/{number}"), "repo": {"full_name": "o/r"}},
                "files": [rel],
            }),
        );
    }
    fn answer(&mut self, call: &ForgeCall) -> ForgeReply {
        let ok = |body: Value| ForgeReply { status: 200, body };
        if self.down {
            return ForgeReply {
                status: 503,
                body: Value::Null,
            };
        }
        let body: Value = call
            .body
            .as_deref()
            .and_then(|b| serde_json::from_str(b).ok())
            .unwrap_or(Value::Null);
        if call.method != "GET" {
            self.writes.push(format!("{} {}", call.method, call.path));
        }
        let path = call.path.as_str();
        match (call.method.as_str(), path) {
            ("GET", "") => ok(json!({"private": self.private})),
            ("GET", p) if p.starts_with("/git/ref/heads/") => {
                ok(json!({"object": {"sha": self.head}}))
            }
            ("GET", p) if p.starts_with("/contents/") => {
                let (file, rev) = p["/contents/publications/news/".len()..]
                    .split_once("?ref=")
                    .unwrap();
                match self
                    .sources
                    .get(&(rev.into(), file.into()))
                    .or_else(|| self.sources.get(&(self.head.clone(), file.into())))
                {
                    Some(text) => ok(json!({
                        "type": "file", "size": text.len(), "encoding": "base64", "sha": "blob1",
                        "content": base64(text.as_bytes()),
                    })),
                    None => ForgeReply {
                        status: 404,
                        body: Value::Null,
                    },
                }
            }
            ("GET", p) if p.starts_with("/git/commits/") => {
                let parent = self.merge_parent.clone().unwrap_or(self.head.clone());
                ok(json!({"tree": {"sha": "tree0"}, "parents": [{"sha": parent}]}))
            }
            ("POST", "/git/blobs") => {
                let sha = self.sha();
                self.blobs
                    .insert(sha.clone(), body["content"].as_str().unwrap_or("").into());
                ok(json!({ "sha": sha }))
            }
            ("POST", "/git/trees") => {
                let entry = &body["tree"][0];
                let rel = entry["path"]
                    .as_str()
                    .unwrap_or("")
                    .trim_start_matches("publications/news/");
                let content = self.blobs[entry["sha"].as_str().unwrap()].clone();
                let sha = self.sha();
                self.commits.insert(sha.clone(), (rel.into(), content));
                ok(json!({ "sha": sha }))
            }
            ("POST", "/git/commits") => {
                let (rel, content) = self.commits[body["tree"].as_str().unwrap()].clone();
                let sha = self.sha();
                self.commits.insert(sha.clone(), (rel, content));
                ok(json!({ "sha": sha }))
            }
            ("POST", "/git/refs") => ok(json!({})),
            ("PATCH", p) if p.starts_with("/git/refs/heads/") => {
                // A forced update could overwrite a concurrent commit; the host must see `force: false`.
                if body["force"] != json!(false) {
                    return ForgeReply {
                        status: 422,
                        body: Value::Null,
                    };
                }
                let sha = body["sha"].as_str().unwrap_or("").to_string();
                let number = p.rsplit('/').next().unwrap().to_string();
                let n: u64 = self
                    .pulls
                    .iter()
                    .find(|(_, v)| {
                        v["head"]["ref"]
                            .as_str()
                            .is_some_and(|r| r.ends_with(&number))
                    })
                    .map(|(n, _)| *n)
                    .unwrap();
                let (rel, content) = self.commits[&sha].clone();
                self.sources.insert((sha.clone(), rel), content);
                self.pulls.get_mut(&n).unwrap()["head"]["sha"] = json!(sha);
                ok(json!({}))
            }
            ("POST", "/pulls") => {
                let number = self.pulls.len() as u64 + 1;
                let head = self
                    .commits
                    .keys()
                    .max()
                    .cloned()
                    .unwrap_or_else(|| BASE.into());
                let (rel, content) = self.commits[&head].clone();
                self.open_pull(number, &head, &rel, &content);
                ok(json!({"number": number, "html_url": format!("https://git/pull/{number}")}))
            }
            ("GET", p) if p.starts_with("/pulls/") && p.contains("/commits") => {
                let n: u64 = p["/pulls/".len()..]
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                ok(json!(self.pushed.get(&n).cloned().unwrap_or_default()))
            }
            ("GET", p) if p.starts_with("/pulls/") && p.contains("/files") => {
                let n: u64 = p["/pulls/".len()..]
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                let rel = self.pulls[&n]["files"][0].as_str().unwrap().to_string();
                ok(json!([{"filename": format!("publications/news/{rel}")}]))
            }
            ("GET", p) if p.starts_with("/pulls/") => {
                let n: u64 = p["/pulls/".len()..].parse().unwrap();
                self.pulls.get(&n).map_or(
                    ForgeReply {
                        status: 404,
                        body: Value::Null,
                    },
                    |v| ok(v.clone()),
                )
            }
            ("PUT", p) if p.ends_with("/merge") => {
                let sha = self.sha();
                ok(json!({"merged": true, "sha": sha}))
            }
            ("PATCH", p) if p.starts_with("/pulls/") => {
                let n: u64 = p["/pulls/".len()..].parse().unwrap();
                self.pulls.get_mut(&n).unwrap()["state"] = body["state"].clone();
                ok(json!({}))
            }
            _ => ForgeReply {
                status: 404,
                body: Value::Null,
            },
        }
    }
}
fn base64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

struct World {
    cfg: Config,
    state: State,
    git: Git,
    blobs: BTreeMap<String, Option<Rc<Vec<u8>>>>,
    audit: Vec<AuditEntry>,
    nonce: u64,
}
struct Reply {
    status: u16,
    body: Value,
}

fn release(commit: &str) -> Value {
    let item = json!({
        "id": "story", "slug": "story", "summary": "s", "byline": "Ed One", "section": "news",
        "tags": [], "published": "2026-10-14T09:00:00Z", "updated": "2026-10-18T09:00:00Z",
        "state": "published", "audience": "public", "kind": "report",
        "title": "Story", "source": SOURCE, "route": "/story", "article": "x", "content": "x",
        "card": "", "lead": "", "assets": {},
    });
    json!({
        "schema": 1, "id": "r1", "commit": commit, "toolchain": "t",
        "site": {"title": "T", "description": "D", "base_url": "https://news.example", "layout": "broadsheet",
                 "mode": "external", "sections": [{"id": "news", "title": "News", "description": "d"}],
                 "navigation": [], "lead": []},
        "items": [item], "templates": {}, "runtime": {}, "objects": {},
    })
}

impl World {
    fn new() -> World {
        let mut state = State {
            active: Some("r1".into()),
            ..State::default()
        };
        for (key, access) in [
            ("email:ed@x", role(RoleName::Editor, &["news"], &["public"])),
            (
                "email:ed2@x",
                role(RoleName::Editor, &["news"], &["public"]),
            ),
            ("email:boss@x", role(RoleName::SuperEditor, &["*"], &["*"])),
            ("email:boss2@x", role(RoleName::SuperEditor, &["*"], &["*"])),
            ("email:r@x", role(RoleName::Reader, &[], &["members"])),
        ] {
            state.roles.insert(key.into(), access);
        }
        // Changes made in Git are approved only by reviewers with a mapped GitHub login.
        for (key, login) in [("email:ed2@x", "ed2-gh"), ("email:boss@x", "boss-gh")] {
            state.roles.get_mut(key).unwrap().github_login = Some(login.into());
        }
        let mut blobs = BTreeMap::new();
        blobs.insert(
            "releases/r1.json".to_string(),
            Some(Rc::new(release(BASE).to_string().into_bytes())),
        );
        World {
            cfg: cfg(),
            state,
            git: Git::new(),
            blobs,
            audit: vec![],
            nonce: 0,
        }
    }
    fn get(&mut self, who_key: &str, path: &str) -> Reply {
        self.call(who_key, "GET", path, Value::Null)
    }
    fn post(&mut self, who_key: &str, path: &str, body: Value) -> Reply {
        self.call(who_key, "POST", path, body)
    }
    fn call(&mut self, who_key: &str, method: &str, path: &str, body: Value) -> Reply {
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let req = Request {
            method: method.into(),
            host: "news.example".into(),
            path: path.into(),
            query: query
                .split('&')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: if body.is_null() {
                vec![]
            } else {
                body.to_string().into_bytes()
            },
            ..Request::default()
        };
        let principal = who(who_key);
        self.nonce += 1;
        let nonce = format!("nonce{}", self.nonce);
        let mut io = Io {
            blobs: self.blobs.clone(),
            ..Io::default()
        };
        for _ in 0..200 {
            match handle(
                &self.cfg,
                Some(&principal),
                &self.state,
                &io,
                "2026-10-27T10:00:00Z",
                &nonce,
                &req,
            ) {
                Outcome::Done(done) => {
                    if let Some(state) = done.state {
                        self.state = state;
                    }
                    self.audit.extend(done.audit);
                    return Reply {
                        status: done.response.status,
                        body: serde_json::from_slice(&done.response.body).unwrap_or(Value::Null),
                    };
                }
                Outcome::Need(needs) => {
                    for need in needs {
                        match need {
                            Need::Forge(call) => {
                                let reply = self.git.answer(&call);
                                io.forge.insert(call.key(), reply);
                            }
                            Need::Blob { key } => {
                                io.blobs
                                    .insert(key.clone(), self.blobs.get(&key).cloned().flatten());
                            }
                            Need::Audit { .. } => io.audit = Some(self.audit.clone()),
                            Need::Head { key } => {
                                io.heads.insert(key, None);
                            }
                        }
                    }
                }
            }
        }
        panic!("the call never settled");
    }
    fn actions(&self) -> Vec<&str> {
        self.audit.iter().map(|a| a.action.as_str()).collect()
    }
    /// A pending change from `actor` at `head`, with a finished preview build.
    fn change(&mut self, number: u64, actor: &str, head: &str, editor: bool, built: bool) {
        let source = article("published", "Ed One", "Changed body.");
        self.git.open_pull(number, head, SOURCE, &source);
        self.state.edits.insert(
            number,
            Edit {
                number,
                url: format!("https://git/pull/{number}"),
                head: head.into(),
                branch: format!("publish-edit/{number}"),
                actor: actor.into(),
                git_author: None,
                imported: false,
                state: EditState::PendingReview,
                path: Some(SOURCE.into()),
                contributors: vec![actor.into()],
                commit: None,
                release: None,
                author_is_editor: editor,
            },
        );
        if built {
            self.state.builds.insert(
                head.into(),
                Build {
                    state: BuildState::Built,
                    release: None,
                    reason: None,
                },
            );
        }
    }
}

const H1: &str = "1111111111111111111111111111111111111111";

#[test]
fn readers_cannot_edit_and_editors_cannot_grant_roles_or_restore() {
    let mut w = World::new();
    assert_eq!(w.get("email:r@x", "/_editor/api/session").status, 403);
    let source = article("published", "Ed One", "x");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": source});
    assert_eq!(w.post("email:r@x", "/_editor/api/edit", edit).status, 403);
    let grant = json!({"identity": "email:new@x", "role": "editor", "sections": ["news"], "audiences": ["public"]});
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/roles", grant).status,
        403
    );
    w.state.hidden.insert(
        "story".into(),
        Hide {
            revision: "h".into(),
            actor: "email:boss@x".into(),
            reason: "r".into(),
            at: "t".into(),
            reconciliation: "pending".into(),
            pull: None,
        },
    );
    let restore = json!({"id": "story", "reason": "fixed"});
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/restore", restore).status,
        403
    );
    assert!(w.get("email:ed@x", "/_editor/api/audit").status == 403);
}

#[test]
fn session_and_items_show_an_editor_their_scope() {
    let mut w = World::new();
    let session = w.get("email:ed@x", "/_editor/api/session");
    assert_eq!(session.body["revision"], BASE);
    assert_eq!(session.body["release"], "r1");
    w.change(7, "email:ed@x", H1, true, true);
    let items = w.get("email:ed@x", "/_editor/api/items");
    assert_eq!(items.body["items"][0]["id"], "story");
    assert_eq!(items.body["items"][0]["state"], "live");
    assert_eq!(items.body["changes"][0]["validation"]["state"], "built");
    // A git outage leaves the desk reachable.
    w.git.down = true;
    let session = w.get("email:ed@x", "/_editor/api/session");
    assert_eq!(session.status, 200);
    assert_eq!(session.body["revision"], Value::Null);
}

#[test]
fn source_reads_at_an_explicit_commit_inside_scope() {
    let mut w = World::new();
    let source = w.get(
        "email:ed@x",
        &format!("/_editor/api/source?id=story&revision={BASE}"),
    );
    assert_eq!(source.status, 200);
    assert!(
        source.body["source"]
            .as_str()
            .unwrap()
            .contains("Hello **world**.")
    );
    assert_eq!(source.body["revision"], BASE);
    assert_eq!(
        w.get("email:ed@x", "/_editor/api/source?id=missing").status,
        404
    );
    assert_eq!(
        w.get("email:ed@x", "/_editor/api/source?id=story&revision=main")
            .status,
        400
    );
    w.state.roles.insert(
        "email:other@x".into(),
        role(RoleName::Editor, &["sport"], &["public"]),
    );
    assert_eq!(
        w.get("email:other@x", "/_editor/api/source?id=story")
            .status,
        404
    );
}

#[test]
fn a_concurrent_pull_request_edit_is_rejected_and_the_ref_update_is_never_forced() {
    let mut w = World::new();
    let first =
        json!({"path": SOURCE, "revision": BASE, "source": article("published", "Ed One", "One.")});
    let created = w.post("email:ed@x", "/_editor/api/edit", first);
    assert_eq!(created.status, 201, "{:?}", created.body);
    let n = created.body["number"].as_u64().unwrap();
    let head = created.body["head"].as_str().unwrap().to_string();
    // An edit opened at an older head of the same pull request is refused before any write.
    let writes = w.git.writes.len();
    let stale = json!({"path": SOURCE, "revision": BASE, "pull": n, "source": article("published", "Ed One", "Stale.")});
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", stale).status, 409);
    assert_eq!(w.git.writes.len(), writes);
    // At the current head the update goes through, and the fake refuses it if it were forced.
    let current = json!({"path": SOURCE, "revision": head, "pull": n, "source": article("published", "Ed One", "Two.")});
    let updated = w.post("email:ed@x", "/_editor/api/edit", current);
    assert_eq!(updated.status, 201, "{:?}", updated.body);
    assert_ne!(updated.body["head"].as_str().unwrap(), head);
}

#[test]
fn a_byline_mapped_to_a_person_makes_them_the_author_of_those_stories() {
    // Production principals are named by their e-mail address, so a real byline never equals the name.
    let mut w = World::new();
    let edit = json!({"path": SOURCE, "revision": BASE, "source": article("published", "Ed One", "Mapped.")});
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/edit", edit.clone())
            .status,
        403
    );
    let mut ed2 = w.state.roles["email:ed2@x"].clone();
    ed2.byline = Some("Ed One".into());
    w.state.roles.insert("email:ed2@x".into(), ed2);
    assert_eq!(w.post("email:ed2@x", "/_editor/api/edit", edit).status, 201);
}

#[test]
fn an_authors_valid_edit_becomes_an_attributed_pull_request() {
    let mut w = World::new();
    let source = article("published", "Ed One", "A **better** body.");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": source});
    let reply = w.post("email:ed@x", "/_editor/api/edit", edit);
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    assert_eq!(reply.body["actor"], "email:ed@x");
    assert_eq!(reply.body["state"], "pending review");
    let n = reply.body["number"].as_u64().unwrap();
    let change = &w.state.edits[&n];
    assert!(change.author_is_editor);
    assert_eq!(change.contributors, vec!["email:ed@x".to_string()]);
    assert!(w.actions().contains(&"editor.change"));
    // The body is saved exactly as written.
    assert!(w.git.commits.values().any(|(_, c)| *c == source));
}

#[test]
fn an_edit_by_someone_who_is_not_the_author_is_refused() {
    let mut w = World::new();
    let source = article("published", "Ed One", "Mine now.");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": source});
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/edit", edit.clone())
            .status,
        403
    );
    assert!(w.git.writes.is_empty());
    // Not even a super editor edits another person's item in the browser.
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/edit", edit).status,
        403
    );
}

#[test]
fn an_invalid_source_is_refused_with_the_core_diagnostics_before_git() {
    let mut w = World::new();
    // A duplicate key is a YAML error with a line number.
    let duplicate = article("published", "Ed One", "x")
        .replace("version: 1.0.0\n", "version: 1.0.0\nversion: 2.0.0\n");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": duplicate});
    let reply = w.post("email:ed@x", "/_editor/api/edit", edit);
    assert_eq!(reply.status, 422);
    let diagnostics = reply.body["diagnostics"].as_array().unwrap();
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == "yaml.duplicate-key" && d["line"].is_number()),
        "{diagnostics:?}"
    );
    // A rich block that does not validate is refused too.
    let broken = article("published", "Ed One", "```chart\ntype: nonsense\n```");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": broken});
    let reply = w.post("email:ed@x", "/_editor/api/edit", edit);
    assert_eq!(reply.status, 422, "{:?}", reply.body);
    assert!(w.git.writes.is_empty());
    assert!(w.state.edits.is_empty());
}

#[test]
fn edits_keep_the_stable_identity_and_patterns() {
    let mut w = World::new();
    let renamed = article("published", "Ed One", "x").replace("id: \"story\"", "id: \"other\"");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": renamed});
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", edit).status, 403);
    let bad =
        article("published", "Ed One", "x").replace("slug: \"story\"", "slug: \"Not A Slug\"");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": bad});
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", edit).status, 400);
    let moved =
        article("published", "Ed One", "x").replace("section: \"news\"", "section: \"sport\"");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": moved});
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", edit).status, 403);
    let huge = article("published", "Ed One", &"word ".repeat(55_000));
    let edit = json!({"path": SOURCE, "revision": BASE, "source": huge});
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", edit).status, 400);
    let edit =
        json!({"path": SOURCE, "revision": "main", "source": article("published", "Ed One", "x")});
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", edit).status, 400);
    assert!(w.git.writes.is_empty());
}

#[test]
fn an_edit_refuses_a_public_repository_and_a_moved_branch() {
    let mut w = World::new();
    let source = article("published", "Ed One", "Public?");
    let edit = json!({"path": SOURCE, "revision": BASE, "source": source});
    w.git.private = false;
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/edit", edit.clone())
            .status,
        403
    );
    w.git.private = true;
    w.git.head = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2".into();
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", edit).status, 409);
    assert!(w.git.writes.is_empty());
}

#[test]
fn only_a_super_editor_may_propose_restoring_hidden_content() {
    let mut w = World::new();
    w.git.sources.insert(
        (BASE.into(), SOURCE.into()),
        article("hidden", "Ed One", "Gone."),
    );
    let edit = json!({"path": SOURCE, "revision": BASE, "source": article("published", "Ed One", "Back.")});
    assert_eq!(w.post("email:ed@x", "/_editor/api/edit", edit).status, 403);
}

#[test]
fn an_independent_approval_of_a_non_editors_change_merges_and_goes_public() {
    let mut w = World::new();
    w.change(7, "github:contributor", H1, false, true);
    let approve = json!({"number": 7, "head": H1});
    let reply = w.post("email:ed2@x", "/_editor/api/approve", approve);
    assert_eq!(reply.status, 200, "{:?}", reply.body);
    assert_eq!(reply.body["state"], "building");
    let commit = reply.body["commit"].as_str().unwrap();
    let authorized = &w.state.authorized[commit];
    assert_eq!(authorized.actor, "email:ed2@x");
    assert_eq!(authorized.origin, None);
    assert_eq!(authorized.head.as_deref(), Some(H1));
    assert_eq!(authorized.base.as_deref(), Some(BASE));
    assert_eq!(w.state.edits[&7].state, EditState::Building);
    assert!(w.actions().contains(&"change.approved"));
    assert!(w.actions().contains(&"change.merged"));
}

#[test]
fn authors_contributors_and_aliases_cannot_approve() {
    let mut w = World::new();
    w.change(7, "email:ed@x", H1, false, true);
    let approve = json!({"number": 7, "head": H1});
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/approve", approve.clone())
            .status,
        403
    );
    // A second contributor on the same change is barred as well.
    w.state
        .edits
        .get_mut(&7)
        .unwrap()
        .contributors
        .push("email:ed2@x".into());
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/approve", approve.clone())
            .status,
        403
    );
    // The same person under a mapped GitHub login is one actor.
    w.state.roles.get_mut("email:boss@x").unwrap().github_login = Some("Ed-Gh".into());
    w.state.edits.get_mut(&7).unwrap().actor = "github:ed-gh".into();
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/approve", approve.clone())
            .status,
        403
    );
    assert!(w.state.authorized.is_empty());
    assert!(w.state.approvals.is_empty());
    // A reader never reaches the desk.
    assert_eq!(
        w.post("email:r@x", "/_editor/api/approve", approve).status,
        403
    );
}

#[test]
fn a_reviewer_who_pushed_a_commit_to_the_change_cannot_approve_it() {
    let mut w = World::new();
    w.change(7, "github:contributor", H1, false, true);
    let approve = json!({"number": 7, "head": H1});
    // ed2 pushed a commit straight to the branch in Git; the desk never recorded it.
    w.git.pushed.insert(
        7,
        vec![json!({
            "author": {"login": "someone"}, "committer": {"login": "someone"},
            "commit": {"author": {"email": "someone@x"}, "committer": {"email": "ed2@x"}},
        })],
    );
    let mut ed2 = w.state.roles["email:ed2@x"].clone();
    ed2.github_login = Some("ed2-gh".into());
    w.state.roles.insert("email:ed2@x".into(), ed2);
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/approve", approve.clone())
            .status,
        403
    );
    // By mapped login as well.
    w.git.pushed.insert(
        7,
        vec![json!({"author": {"login": "Ed2-GH"}, "committer": {"login": "web-flow"}})],
    );
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/approve", approve.clone())
            .status,
        403
    );
    // A truncated history proves nothing.
    w.git
        .pushed
        .insert(7, vec![json!({"author": {"login": "x"}}); 100]);
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/approve", approve.clone())
            .status,
        403
    );
    assert!(w.state.approvals.is_empty());
    // Someone who pushed nothing approves and the change merges.
    w.git.pushed.remove(&7);
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/approve", approve)
            .status,
        200
    );
}

#[test]
fn an_approval_is_bound_to_the_exact_head_and_a_successful_preview() {
    let mut w = World::new();
    w.change(7, "github:contributor", H1, false, false);
    let stale = "2222222222222222222222222222222222222222";
    assert_eq!(
        w.post(
            "email:ed2@x",
            "/_editor/api/approve",
            json!({"number": 7, "head": stale})
        )
        .status,
        409
    );
    // No finished preview build yet.
    assert_eq!(
        w.post(
            "email:ed2@x",
            "/_editor/api/approve",
            json!({"number": 7, "head": H1})
        )
        .status,
        409
    );
    w.state.builds.insert(
        H1.into(),
        Build {
            state: BuildState::Failed,
            release: None,
            reason: Some("bad".into()),
        },
    );
    assert_eq!(
        w.post(
            "email:ed2@x",
            "/_editor/api/approve",
            json!({"number": 7, "head": H1})
        )
        .status,
        409
    );
    // A closed pull request cannot be approved.
    w.state.builds.insert(
        H1.into(),
        Build {
            state: BuildState::Built,
            release: None,
            reason: None,
        },
    );
    w.git.pulls.get_mut(&7).unwrap()["state"] = json!("closed");
    assert_eq!(
        w.post(
            "email:ed2@x",
            "/_editor/api/approve",
            json!({"number": 7, "head": H1})
        )
        .status,
        409
    );
    assert!(w.state.approvals.is_empty());
    assert!(w.git.writes.is_empty());
}

#[test]
fn a_changed_role_voids_an_approval_and_publishing_needs_an_independent_head() {
    let mut w = World::new();
    w.change(7, "github:contributor", H1, false, true);
    // An approval whose merge could not run stays recorded until the role changes.
    w.git.head = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2".into();
    let reply = w.post(
        "email:ed2@x",
        "/_editor/api/approve",
        json!({"number": 7, "head": H1}),
    );
    assert_eq!(reply.status, 409);
    assert_eq!(reply.body["state"], "approved");
    assert!(w.state.approvals.contains_key(H1));
    assert_eq!(w.state.edits[&7].state, EditState::Approved);
    let demote =
        json!({"identity": "email:ed2@x", "role": "reader", "sections": [], "audiences": []});
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/roles", demote).status,
        200
    );
    assert!(w.state.approvals.is_empty());
    assert_eq!(w.state.edits[&7].state, EditState::PendingReview);
    assert!(w.actions().contains(&"role.updated"));
    // Without an approval nobody publishes.
    let publish = json!({"number": 7, "head": H1});
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/publish", publish)
            .status,
        403
    );
}

#[test]
fn an_approved_head_publishes_once_the_branch_is_live_again() {
    let mut w = World::new();
    w.change(7, "github:contributor", H1, false, true);
    let moved = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";
    w.git.head = moved.into();
    assert_eq!(
        w.post(
            "email:ed2@x",
            "/_editor/api/approve",
            json!({"number": 7, "head": H1})
        )
        .status,
        409
    );
    // The contributor cannot publish their own approval; another editor can once the release catches up.
    w.blobs.insert(
        "releases/r1.json".into(),
        Some(Rc::new(release(moved).to_string().into_bytes())),
    );
    let publish = json!({"number": 7, "head": H1});
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/publish", publish.clone())
            .status,
        200
    );
    assert_eq!(w.state.authorized.len(), 1);
}

#[test]
fn branch_drift_during_a_merge_holds_the_commit() {
    let mut w = World::new();
    w.change(7, "github:contributor", H1, false, true);
    w.git.merge_parent = Some("c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3".into());
    let reply = w.post(
        "email:ed2@x",
        "/_editor/api/approve",
        json!({"number": 7, "head": H1}),
    );
    assert_eq!(reply.status, 409);
    assert!(w.state.authorized.is_empty());
    assert_eq!(w.state.edits[&7].state, EditState::Failed);
    assert!(w.state.edits[&7].commit.is_some());
    assert!(w.actions().contains(&"change.merge.held"));
}

#[test]
fn an_editors_own_change_merges_at_once_without_a_second_approver() {
    let mut w = World::new();
    w.change(7, "email:ed@x", H1, true, true);
    let publish = json!({"number": 7, "head": H1});
    // Others cannot publish it, and approving it is not the path.
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/publish", publish.clone())
            .status,
        403
    );
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/approve", publish.clone())
            .status,
        409
    );
    let reply = w.post("email:ed@x", "/_editor/api/publish", publish);
    assert_eq!(reply.status, 200, "{:?}", reply.body);
    let commit = reply.body["commit"].as_str().unwrap();
    let authorized = &w.state.authorized[commit];
    assert_eq!(authorized.origin.as_deref(), Some("editor"));
    assert_eq!(authorized.actor, "email:ed@x");
    assert!(w.state.approvals.is_empty());
}

#[test]
fn an_editor_makes_their_own_private_item_public_and_so_may_another_in_scope() {
    let mut w = World::new();
    w.state.private.insert(
        "story".into(),
        Private {
            actor: "email:ed@x".into(),
            since: "t".into(),
        },
    );
    let approve = json!({"id": "story"});
    // An editor outside the section cannot make it public.
    w.state.roles.insert(
        "email:other@x".into(),
        role(RoleName::Editor, &["sport"], &["public"]),
    );
    assert_eq!(
        w.post("email:other@x", "/_editor/api/approve", approve.clone())
            .status,
        404
    );
    let before = w.state.access_revision;
    // The author makes their own item public (Daniel's OPN-54: "the editor makes it public").
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/approve", approve.clone())
            .status,
        200
    );
    assert!(w.state.private.is_empty());
    assert!(w.actions().contains(&"item.published"));
    assert!(w.state.access_revision > before);
    // Nothing left to approve.
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/approve", approve)
            .status,
        409
    );
}

#[test]
fn rejecting_closes_the_change_on_its_exact_head() {
    let mut w = World::new();
    w.change(7, "github:contributor", H1, false, true);
    let stale = json!({"number": 7, "head": "2222222222222222222222222222222222222222"});
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/reject", stale).status,
        409
    );
    let reject = json!({"number": 7, "head": H1, "reason": "no"});
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/reject", reject).status,
        200
    );
    assert_eq!(w.state.edits[&7].state, EditState::Closed);
    assert_eq!(w.git.pulls[&7]["state"], "closed");
    assert!(w.actions().contains(&"change.rejected"));
}

#[test]
fn an_urgent_hide_survives_a_git_outage() {
    let mut w = World::new();
    w.git.down = true;
    let hide = json!({"id": "story", "reason": "legal"});
    let first = w.post("email:ed@x", "/_editor/api/hide", hide.clone());
    assert_eq!(first.status, 202);
    assert!(w.state.hidden.contains_key("story"));
    assert_eq!(w.state.hidden["story"].reconciliation, "pending");
    assert!(w.actions().contains(&"item.hidden"));
    // Git is still down: the hide stays in force and reconciliation stays pending.
    let again = w.post("email:ed@x", "/_editor/api/hide", hide.clone());
    assert_eq!(again.status, 202);
    assert_eq!(w.state.hidden["story"].reconciliation, "pending");
    // Git returns: the retry opens the reconciling pull request.
    w.git.down = false;
    let done = w.post("email:ed@x", "/_editor/api/hide", hide);
    assert_eq!(done.status, 200, "{:?}", done.body);
    let hidden = &w.state.hidden["story"];
    assert!(hidden.pull.is_some() && hidden.reconciliation.starts_with("https://git/pull/"));
    assert!(w.actions().contains(&"hide.reconciled"));
    let (_, content) = w.git.commits.values().last().unwrap();
    assert!(
        content.contains("state: \"hidden\"")
            && content.contains("updated: \"2026-10-27T10:00:00Z\"")
    );
    assert!(content.contains("Hello **world**."));
}

#[test]
fn hiding_needs_scope_and_a_reason() {
    let mut w = World::new();
    assert_eq!(
        w.post(
            "email:ed@x",
            "/_editor/api/hide",
            json!({"id": "story", "reason": " "})
        )
        .status,
        400
    );
    w.state.roles.insert(
        "email:other@x".into(),
        role(RoleName::Editor, &["sport"], &["public"]),
    );
    assert_eq!(
        w.post(
            "email:other@x",
            "/_editor/api/hide",
            json!({"id": "story", "reason": "x"})
        )
        .status,
        403
    );
    assert!(w.state.hidden.is_empty());
}

#[test]
fn a_different_super_editor_restores_once_git_says_published() {
    let mut w = World::new();
    w.git.sources.insert(
        (BASE.into(), SOURCE.into()),
        article("hidden", "Ed One", "Gone."),
    );
    w.state.hidden.insert(
        "story".into(),
        Hide {
            revision: "h".into(),
            actor: "email:boss@x".into(),
            reason: "r".into(),
            at: "t".into(),
            reconciliation: "x".into(),
            pull: Some(9),
        },
    );
    w.change(9, "email:boss@x", H1, true, true);
    let restore = json!({"id": "story", "reason": "cleared"});
    // The same super editor, and the hide's own alias, cannot restore.
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/restore", restore.clone())
            .status,
        403
    );
    // Git still says hidden: the runtime hide stays.
    assert_eq!(
        w.post("email:boss2@x", "/_editor/api/restore", restore.clone())
            .status,
        409
    );
    assert!(w.state.hidden.contains_key("story"));
    assert_eq!(
        w.post(
            "email:boss2@x",
            "/_editor/api/restore",
            json!({"id": "story"})
        )
        .status,
        400
    );
    w.git.sources.insert(
        (BASE.into(), SOURCE.into()),
        article("published", "Ed One", "Back."),
    );
    let reply = w.post("email:boss2@x", "/_editor/api/restore", restore);
    assert_eq!(reply.status, 200, "{:?}", reply.body);
    assert!(w.state.hidden.is_empty());
    assert_eq!(w.state.edits[&9].state, EditState::Closed);
    assert_eq!(w.git.pulls[&9]["state"], "closed");
    assert!(w.actions().contains(&"item.restored"));
}

#[test]
fn roles_guard_the_publisher_the_identity_and_the_scope() {
    let mut w = World::new();
    let grant = |identity: &str, role: &str| json!({"identity": identity, "role": role, "sections": ["news"], "audiences": ["public"]});
    assert_eq!(
        w.post(
            "email:boss@x",
            "/_editor/api/roles",
            grant("service:publisher", "editor")
        )
        .status,
        403
    );
    assert_eq!(
        w.post(
            "email:boss@x",
            "/_editor/api/roles",
            grant("alice", "editor")
        )
        .status,
        400
    );
    assert_eq!(
        w.post(
            "email:boss@x",
            "/_editor/api/roles",
            grant("email:a b@x", "editor")
        )
        .status,
        400
    );
    assert_eq!(
        w.post(
            "email:boss@x",
            "/_editor/api/roles",
            grant("email:new@x", "admin")
        )
        .status,
        400
    );
    let wild = json!({"identity": "email:new@x", "role": "editor", "sections": ["Bad Id"], "audiences": []});
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/roles", wild).status,
        400
    );
    let mut ok = grant("email:new@x", "editor");
    ok["github_login"] = json!("new-gh");
    assert_eq!(w.post("email:boss@x", "/_editor/api/roles", ok).status, 200);
    assert_eq!(
        w.state.roles["email:new@x"].github_login.as_deref(),
        Some("new-gh")
    );
    assert!(!w.state.roles.contains_key("service:publisher"));
}

#[test]
fn self_demotion_only_disables_your_own_access_with_another_owner_assigned() {
    let mut w = World::new();
    let off =
        json!({"identity": "email:boss@x", "role": "reader", "sections": [], "audiences": []});
    let keep = json!({"identity": "email:boss@x", "role": "editor", "sections": ["news"], "audiences": ["public"]});
    // Keeping any scope for yourself is refused.
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/roles", keep).status,
        400
    );
    // The last full-scope super editor cannot retire themselves.
    w.state.roles.remove("email:boss2@x");
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/roles", off.clone())
            .status,
        400
    );
    // A narrower super editor does not count as another owner.
    w.state.roles.insert(
        "email:boss2@x".into(),
        role(RoleName::SuperEditor, &["news"], &["*"]),
    );
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/roles", off.clone())
            .status,
        400
    );
    w.state.roles.insert(
        "email:boss2@x".into(),
        role(RoleName::SuperEditor, &["*"], &["*"]),
    );
    assert_eq!(
        w.post("email:boss@x", "/_editor/api/roles", off).status,
        200
    );
    assert_eq!(w.state.roles["email:boss@x"].name, RoleName::Reader);
}

#[test]
fn resume_retry_and_audit() {
    let mut w = World::new();
    w.state.paused = true;
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/resume", json!({"reason": "ok"}))
            .status,
        403
    );
    assert_eq!(
        w.post(
            "email:boss@x",
            "/_editor/api/resume",
            json!({"reason": " "})
        )
        .status,
        400
    );
    assert_eq!(
        w.post(
            "email:boss@x",
            "/_editor/api/resume",
            json!({"reason": "checked"})
        )
        .status,
        200
    );
    assert!(!w.state.paused);

    w.change(7, "email:ed@x", H1, true, false);
    w.state.builds.insert(
        H1.into(),
        Build {
            state: BuildState::Failed,
            release: None,
            reason: Some("x".into()),
        },
    );
    // Another editor's failed preview is not theirs to retry.
    assert_eq!(
        w.post("email:ed2@x", "/_editor/api/retry", json!({"number": 7}))
            .status,
        403
    );
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/retry", json!({"number": 7}))
            .status,
        200
    );
    assert!(!w.state.builds.contains_key(H1));
    // Only a failed current preview can be retried.
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/retry", json!({"number": 7}))
            .status,
        409
    );

    let audit = w.get("email:boss@x", "/_editor/api/audit");
    assert_eq!(audit.status, 200);
    let actions: Vec<&str> = audit
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec!["publication.resumed", "build.retry.requested"]
    );
    assert_eq!(w.get("email:ed@x", "/_editor/api/audit").status, 403);
}

#[test]
fn the_embedded_editor_page_belongs_to_the_adapter() {
    let mut w = World::new();
    assert_eq!(w.get("email:ed@x", "/_editor/").status, 404);
    assert_eq!(w.get("email:ed@x", "/_editor/app.js").status, 404);
    assert_eq!(w.get("email:ed@x", "/_editor/api/nothing").status, 404);
    assert_eq!(
        w.post("email:ed@x", "/_editor/api/create", json!({}))
            .status,
        404
    );
}
