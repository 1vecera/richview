//! The `/_publish/*` API against an in-memory host: storage blobs and heads, and canned Git host replies.
use richview_publish_server::{access::*, host::*, ledger::*, model::Release};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::rc::Rc;

const BRANCH: &str = "publish/news";
const SUPER: &str = "email:super@example.test";
const EDITOR: &str = "email:editor@example.test";
const PUBLISHER: &str = "service:publisher";

fn cfg() -> Config {
    Config {
        publication_host: "news.example".into(),
        repository: "o/r".into(),
        branch: BRANCH.into(),
        content_root: "content".into(),
        publisher_identity: PUBLISHER.into(),
        bootstrap_super_editor: SUPER.into(),
        bootstrap_github_login: Some("reviewer".into()),
        bootstrap_commit: None,
    }
}
fn who(key: &str) -> Principal {
    Principal {
        key: key.into(),
        name: key.into(),
        email: None,
        service: key.starts_with("service:"),
    }
}

/// One release fixture: items are `(id, state, audience, version)`; the version changes the item's objects.
fn make_release(commit: &str, items: &[(&str, &str, &str, &str)]) -> Release {
    let mut objects = serde_json::Map::new();
    let mut rows = vec![];
    for (id, state, audience, version) in items {
        let article = sha256(format!("article-{id}-{version}").as_bytes());
        let content = sha256(format!("content-{id}-{version}").as_bytes());
        for hash in [&article, &content] {
            objects.insert(
                hash.clone(),
                json!({"sha256": hash, "bytes": 10, "content_type": "text/html; charset=utf-8"}),
            );
        }
        rows.push(json!({
            "id": id, "slug": id, "summary": "s", "byline": "b", "section": "news", "tags": [],
            "published": "2026-10-01", "updated": "2026-10-01", "state": state, "audience": audience,
            "kind": "report", "title": id, "source": format!("items/{id}/report.rmd"),
            "route": format!("/stories/{id}"), "article": article, "content": content,
            "card": "", "lead": "", "assets": {},
        }));
    }
    let mut release: Release = serde_json::from_value(json!({
        "schema": 1, "id": "", "commit": commit, "toolchain": "t",
        "site": {
            "title": "News", "description": "d", "base_url": "https://news.example",
            "layout": "broadsheet", "mode": "external",
            "sections": [{"id": "news", "title": "News", "description": "d"}],
            "navigation": [], "lead": [],
        },
        "items": rows, "templates": {}, "runtime": {"a.js": "x"}, "objects": objects,
    }))
    .unwrap();
    release.id = sha256(&serde_json::to_vec(&release).unwrap());
    release
}

/// The host: durable state, storage and the Git host's canned answers (`"METHOD path"` to status and body).
struct Net {
    cfg: Config,
    state: State,
    blobs: BTreeMap<String, Vec<u8>>,
    forge: BTreeMap<String, (u16, Value)>,
    writes: Vec<Write>,
    calls: Vec<String>,
}
impl Net {
    fn new() -> Self {
        Net {
            cfg: cfg(),
            state: State::default(),
            blobs: BTreeMap::new(),
            forge: BTreeMap::new(),
            writes: vec![],
            calls: vec![],
        }
    }
    fn store(&mut self, release: &Release) {
        self.blobs.insert(
            format!("releases/{}.json", release.id),
            serde_json::to_vec(release).unwrap(),
        );
    }
    /// Makes `release` live at generation 1 with its commit built and activated.
    fn go_live(&mut self, release: &Release) {
        self.store(release);
        self.state.active = Some(release.id.clone());
        self.state.generation = 1;
        self.state
            .activated
            .insert(release.id.clone(), release.commit.clone());
    }
    fn answer(&mut self, method: &str, path: &str, status: u16, body: Value) {
        self.forge
            .insert(format!("{method} {path}"), (status, body));
    }
    /// Canned answers for a merged, independently approved pull request that produced `commit` on `base`.
    fn approved_merge(&mut self, commit: &str, base: &str, author_login: &str) {
        let head = "1".repeat(40);
        self.answer(
            "GET",
            &format!("/git/ref/heads/{BRANCH}"),
            200,
            json!({"object": {"sha": commit}}),
        );
        self.answer(
            "GET",
            &format!("/commits/{commit}/pulls?per_page=100"),
            200,
            json!([{"number": 7, "merged_at": "2026-10-27", "merge_commit_sha": commit, "base": {"ref": BRANCH}}]),
        );
        self.answer(
            "GET",
            "/pulls/7",
            200,
            json!({
                "number": 7, "state": "closed", "html_url": "u", "user": {"login": author_login},
                "base": {"ref": BRANCH, "repo": {"full_name": "o/r"}},
                "head": {"sha": head, "ref": "b", "repo": {"full_name": "o/r"}},
                "changed_files": 1, "merged_at": "2026-10-27", "merge_commit_sha": commit,
            }),
        );
        self.answer(
            "GET",
            "/pulls/7/files?per_page=100",
            200,
            json!([{"filename": "content/items/a/report.rmd"}]),
        );
        self.answer(
            "GET",
            "/pulls/7/reviews?per_page=100",
            200,
            json!([{"state": "APPROVED", "commit_id": head, "user": {"login": "reviewer"}}]),
        );
        self.answer(
            "GET",
            &format!("/git/commits/{commit}"),
            200,
            json!({"parents": [{"sha": base}]}),
        );
        self.pushed_by(&[author_login]);
    }
    /// The logins that put commits on pull request 7.
    fn pushed_by(&mut self, logins: &[&str]) {
        let commits: Vec<Value> = logins
            .iter()
            .map(|l| json!({"author": {"login": l}, "committer": {"login": l}}))
            .collect();
        self.answer("GET", "/pulls/7/commits?per_page=100", 200, json!(commits));
    }
    /// Sends one request and answers every need until the call is done; commits the new state like the host.
    fn call(
        &mut self,
        who_key: Option<&str>,
        method: &str,
        path: &str,
        body: Value,
    ) -> (u16, Value) {
        self.send(who_key, method, path, serde_json::to_vec(&body).unwrap())
    }
    fn send(
        &mut self,
        who_key: Option<&str>,
        method: &str,
        path: &str,
        body: Vec<u8>,
    ) -> (u16, Value) {
        let principal = who_key.map(who);
        let req = Request {
            method: method.into(),
            host: "news.example".into(),
            path: path.into(),
            body,
            ..Request::default()
        };
        let mut io = Io::default();
        for _ in 0..200 {
            match richview_publish_server::handle(
                &self.cfg,
                principal.as_ref(),
                &self.state,
                &io,
                "2026-10-27T10:00:00Z",
                "nonce1",
                &req,
            ) {
                Outcome::Need(needs) => {
                    for need in needs {
                        match need {
                            Need::Blob { key } => {
                                io.blobs.insert(
                                    key.clone(),
                                    self.blobs.get(&key).cloned().map(Rc::new),
                                );
                            }
                            Need::Head { key } => {
                                let head = self.blobs.get(&key).map(|b| BlobHead {
                                    size: b.len(),
                                    sha256: sha256(b),
                                });
                                io.heads.insert(key, head);
                            }
                            Need::Forge(call) => {
                                let key = format!("{} {}", call.method, call.path);
                                self.calls.push(key.clone());
                                let (status, body) =
                                    self.forge.get(&key).cloned().unwrap_or((404, json!({})));
                                io.forge.insert(call.key(), ForgeReply { status, body });
                            }
                            Need::Audit { .. } => io.audit = Some(vec![]),
                        }
                    }
                }
                Outcome::Done(done) => {
                    for Write::Blob {
                        key,
                        bytes,
                        if_absent,
                        ..
                    } in &done.writes
                    {
                        if !*if_absent || !self.blobs.contains_key(key) {
                            self.blobs.insert(key.clone(), bytes.clone());
                        }
                    }
                    self.writes.extend(done.writes.clone());
                    if let Some(state) = done.state {
                        self.state = state;
                    }
                    let json = serde_json::from_slice(&done.response.body).unwrap_or(Value::Null);
                    return (done.response.status, json);
                }
            }
        }
        panic!("the call never finished");
    }
    fn editor(&mut self, key: &str, name: RoleName, login: Option<&str>) {
        self.state.roles.insert(
            key.into(),
            Access {
                name,
                sections: vec!["*".into()],
                audiences: vec!["*".into()],
                github_login: login.map(String::from),
                byline: None,
            },
        );
    }
}

fn live_pair() -> (Net, Release, Release) {
    live_pair_with("published")
}
/// Like `live_pair`, with story `a` in `a_before` in the live release.
fn live_pair_with(a_before: &str) -> (Net, Release, Release) {
    let old = make_release(
        &"a".repeat(40),
        &[
            ("a", a_before, "public", "1"),
            ("b", "published", "members", "1"),
        ],
    );
    let new = make_release(
        &"d".repeat(40),
        &[
            ("a", "published", "public", "2"),
            ("b", "published", "members", "1"),
        ],
    );
    let mut net = Net::new();
    net.go_live(&old);
    net.store(&new);
    (net, old, new)
}

fn activate_body(release: &Release, generation: u64) -> Value {
    json!({"release": release.id, "generation": generation})
}

#[test]
fn only_the_publisher_uploads_and_a_wrong_hash_is_refused() {
    let mut net = Net::new();
    let bytes = b"hello".to_vec();
    let hash = sha256(&bytes);
    let path = format!("/_publish/objects/{hash}");
    assert_eq!(net.send(Some(SUPER), "PUT", &path, bytes.clone()).0, 403);
    assert_eq!(net.send(Some(EDITOR), "PUT", &path, bytes.clone()).0, 403);
    assert_eq!(
        net.send(Some(PUBLISHER), "PUT", &path, b"other".to_vec()).0,
        400
    );
    assert_eq!(
        net.send(
            Some(PUBLISHER),
            "PUT",
            "/_publish/objects/nothex",
            bytes.clone()
        )
        .0,
        400
    );
    let big = vec![0u8; 20 * 1024 * 1024 + 1];
    assert_eq!(
        net.send(
            Some(PUBLISHER),
            "PUT",
            &format!("/_publish/objects/{}", sha256(&big)),
            big
        )
        .0,
        400
    );
    assert_eq!(
        net.send(Some(PUBLISHER), "PUT", &path, bytes.clone()).0,
        200
    );
    assert_eq!(net.blobs[&format!("objects/{hash}")], bytes);
}

#[test]
fn a_publisher_cannot_be_elevated_and_an_editor_cannot_forge_delivery() {
    let mut net = Net::new();
    for action in ["stage", "activate", "building", "failed"] {
        assert_eq!(
            net.call(
                Some(SUPER),
                "POST",
                &format!("/_publish/{action}"),
                json!({})
            )
            .0,
            403
        );
    }
    assert_eq!(
        net.call(Some(EDITOR), "GET", "/_publish/state", json!({}))
            .0,
        403
    );
    assert_eq!(net.call(None, "GET", "/_publish/state", json!({})).0, 403);
    // A role granted to the publisher's key is ignored: the key stays the publisher.
    net.editor(PUBLISHER, RoleName::SuperEditor, None);
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/rollback",
            json!({"release": "x", "reason": "r"})
        )
        .0,
        403
    );
}

#[test]
fn state_and_queue_describe_what_is_live_and_what_waits() {
    let (mut net, old, _) = live_pair();
    net.state.edits.insert(
        7,
        Edit {
            number: 7,
            url: "u".into(),
            head: "1".repeat(40),
            branch: "b".into(),
            actor: EDITOR.into(),
            git_author: None,
            imported: false,
            state: EditState::PendingReview,
            path: None,
            contributors: vec![],
            commit: None,
            release: None,
            author_is_editor: false,
        },
    );
    let (status, state) = net.call(Some(PUBLISHER), "GET", "/_publish/state", Value::Null);
    assert_eq!(
        (status, state["active"].as_str()),
        (200, Some(old.id.as_str()))
    );
    let (_, queue) = net.call(Some(SUPER), "GET", "/_publish/queue", Value::Null);
    assert_eq!(queue["previews"][0]["number"], 7);
    assert_eq!(queue["branch"], BRANCH);
    net.state.builds.insert(
        "1".repeat(40),
        Build {
            state: BuildState::Built,
            release: None,
            reason: None,
        },
    );
    let (_, queue) = net.call(Some(SUPER), "GET", "/_publish/queue", Value::Null);
    assert_eq!(queue["previews"].as_array().unwrap().len(), 0);
}

#[test]
fn staging_verifies_identity_and_objects_then_stores_the_release_once() {
    let mut net = Net::new();
    net.answer("GET", "", 200, json!({"private": true}));
    let release = make_release(&"d".repeat(40), &[("a", "draft", "members", "1")]);
    let body = serde_json::to_value(&release).unwrap();
    // Objects are not uploaded yet.
    assert_eq!(
        net.call(Some(PUBLISHER), "POST", "/_publish/stage", body.clone())
            .0,
        400
    );
    for hash in release.objects.keys() {
        net.blobs.insert(format!("objects/{hash}"), vec![0; 10]);
    }
    // The stored bytes must hash to the registered object.
    assert_eq!(
        net.call(Some(PUBLISHER), "POST", "/_publish/stage", body.clone())
            .0,
        400
    );
    for hash in release.objects.keys() {
        net.blobs
            .insert(format!("objects/{hash}"), b"0123456789".to_vec());
    }
    let mut tampered = body.clone();
    tampered["toolchain"] = "other".into();
    assert_eq!(
        net.call(Some(PUBLISHER), "POST", "/_publish/stage", tampered)
            .0,
        400
    );
}

#[test]
fn a_verified_release_is_stored_immutably_and_its_runtime_recorded() {
    let mut net = Net::new();
    net.answer("GET", "", 200, json!({"private": true}));
    let release = make_release(&"d".repeat(40), &[("a", "published", "public", "1")]);
    // Register objects whose bytes hash to their names.
    let mut fixed = release.clone();
    let mut objects = BTreeMap::new();
    for (hash, object) in &release.objects {
        let bytes = format!("object {hash}").into_bytes();
        let real = sha256(&bytes);
        net.blobs.insert(format!("objects/{real}"), bytes.clone());
        let mut o = object.clone();
        o.sha256 = real.clone();
        o.bytes = bytes.len();
        objects.insert(real, o);
        for item in &mut fixed.items {
            if item.article == *hash {
                item.article = sha256(format!("object {hash}").as_bytes());
            }
            if item.content == *hash {
                item.content = sha256(format!("object {hash}").as_bytes());
            }
        }
    }
    fixed.objects = objects;
    fixed.id = String::new();
    fixed.id = sha256(&serde_json::to_vec(&fixed).unwrap());
    let body = serde_json::to_value(&fixed).unwrap();
    let (status, answer) = net.call(Some(PUBLISHER), "POST", "/_publish/stage", body);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["release"], fixed.id);
    assert!(
        net.blobs
            .contains_key(&format!("releases/{}.json", fixed.id))
    );
    assert_eq!(net.state.builds[&fixed.commit].state, BuildState::Built);
    assert_eq!(net.state.runtimes.len(), 1);
    assert!(net.state.active.is_none());
}

#[test]
fn nonpublic_sources_need_a_private_repository() {
    let mut net = Net::new();
    net.answer("GET", "", 200, json!({"private": false}));
    let release = make_release(&"d".repeat(40), &[("a", "published", "members", "1")]);
    let body = serde_json::to_value(&release).unwrap();
    assert_eq!(
        net.call(Some(PUBLISHER), "POST", "/_publish/stage", body).0,
        403
    );
}

#[test]
fn a_failed_build_leaves_the_last_good_release_live() {
    let (mut net, old, _) = live_pair();
    let commit = "d".repeat(40);
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/building",
            json!({"commit": commit})
        )
        .0,
        200
    );
    assert_eq!(net.state.builds[&commit].state, BuildState::Building);
    let (status, answer) = net.call(
        Some(PUBLISHER),
        "POST",
        "/_publish/failed",
        json!({"commit": commit, "reason": "Validation failed"}),
    );
    assert_eq!((status, answer["state"].as_str()), (200, Some("failed")));
    assert_eq!(net.state.builds[&commit].state, BuildState::Failed);
    assert_eq!(net.state.active, Some(old.id));
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/failed",
            json!({"commit": "short"})
        )
        .0,
        400
    );
}

#[test]
fn a_stale_activation_loses() {
    let (mut net, old, new) = live_pair();
    net.approved_merge(&new.commit, &old.commit, "writer");
    let (status, _) = net.call(
        Some(PUBLISHER),
        "POST",
        "/_publish/activate",
        activate_body(&new, 0),
    );
    assert_eq!(status, 409);
    assert_eq!(net.state.active, Some(old.id.clone()));
    assert_eq!(net.state.generation, 1);
}

#[test]
fn an_approved_merge_with_an_unreviewed_base_cannot_be_published() {
    let (mut net, old, new) = live_pair();
    net.approved_merge(&new.commit, &"f".repeat(40), "writer");
    let (status, _) = net.call(
        Some(PUBLISHER),
        "POST",
        "/_publish/activate",
        activate_body(&new, 1),
    );
    assert_eq!(status, 409);
    assert_eq!(net.state.active, Some(old.id.clone()));
    net.approved_merge(&new.commit, &old.commit, "writer");
    let (status, answer) = net.call(
        Some(PUBLISHER),
        "POST",
        "/_publish/activate",
        activate_body(&new, 1),
    );
    assert_eq!((status, answer["generation"].as_u64()), (200, Some(2)));
    assert_eq!(net.state.active, Some(new.id.clone()));
    assert_eq!(
        net.state.authorized[&new.commit].base.as_deref(),
        Some(old.commit.as_str())
    );
    assert_eq!(net.state.builds[&new.commit].state, BuildState::Live);
    assert_eq!(net.state.activated[&new.id], new.commit);
    assert!(net.state.access_revision > 0);
}

#[test]
fn a_reviewer_who_pushed_a_commit_to_the_change_is_not_its_independent_approver() {
    let (mut net, old, new) = live_pair();
    // The opener is `writer`; the mapped super editor `reviewer` added a commit, then approved the head.
    net.approved_merge(&new.commit, &old.commit, "writer");
    net.pushed_by(&["writer", "reviewer"]);
    let activate = |net: &mut Net| {
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 1),
        )
        .0
    };
    assert_eq!(activate(&mut net), 403);
    assert_eq!(net.state.active, Some(old.id.clone()));
    // A commit with no linked GitHub login cannot show independence either.
    net.answer(
        "GET",
        "/pulls/7/commits?per_page=100",
        200,
        json!([{"author": null, "committer": {"login": "writer"}}]),
    );
    assert_eq!(activate(&mut net), 403);
    // With only the opener's commits the same approval authorizes the release.
    net.pushed_by(&["writer"]);
    assert_eq!(activate(&mut net), 200);
}

#[test]
fn activation_needs_the_current_branch_head_and_an_independent_approval() {
    let (mut net, old, new) = live_pair();
    net.approved_merge(&new.commit, &old.commit, "writer");
    // The branch moved on after this build.
    net.answer(
        "GET",
        &format!("/git/ref/heads/{BRANCH}"),
        200,
        json!({"object": {"sha": "9".repeat(40)}}),
    );
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 1)
        )
        .0,
        409
    );
    // The only approval is the author's own: nothing authorizes the commit.
    net.approved_merge(&new.commit, &old.commit, "reviewer");
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 1)
        )
        .0,
        403
    );
    assert_eq!(net.state.active, Some(old.id));
}

#[test]
fn activation_rebuilds_floors_and_keeps_deleted_items_deleted() {
    let (mut net, old, _) = live_pair();
    let next = make_release(&"c".repeat(40), &[("a", "published", "public", "3")]);
    net.store(&next);
    net.state.policies.insert(
        "b".into(),
        Policy {
            id: "b".into(),
            section: "news".into(),
            state: richview_publish_server::model::State::Published,
            audience: "members".into(),
            deleted: false,
        },
    );
    net.approved_merge(&next.commit, &old.commit, "writer");
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&next, 1)
        )
        .0,
        200
    );
    assert!(!net.state.policies["a"].deleted);
    assert!(net.state.policies["b"].deleted);
}

#[test]
fn rollback_pauses_publication_and_keeps_hides_and_floors() {
    let (mut net, old, new) = live_pair();
    net.approved_merge(&new.commit, &old.commit, "writer");
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 1)
        )
        .0,
        200
    );
    net.state.hidden.insert(
        "a".into(),
        Hide {
            revision: "r".into(),
            actor: SUPER.into(),
            reason: "hold".into(),
            at: "t".into(),
            reconciliation: "pending".into(),
            pull: None,
        },
    );
    net.state.policies.get_mut("b").unwrap().deleted = true;
    let rollback = |release: &Release, generation: u64, reason: &str| json!({"release": release.id, "generation": generation, "reason": reason});
    // A reason, a super editor and a release that was once live are all required.
    assert_eq!(
        net.call(
            Some(SUPER),
            "POST",
            "/_publish/rollback",
            rollback(&old, 2, "")
        )
        .0,
        403
    );
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/rollback",
            rollback(&old, 2, "drill")
        )
        .0,
        403
    );
    net.editor(EDITOR, RoleName::Editor, None);
    assert_eq!(
        net.call(
            Some(EDITOR),
            "POST",
            "/_publish/rollback",
            rollback(&old, 2, "drill")
        )
        .0,
        403
    );
    let never = make_release(&"e".repeat(40), &[("a", "published", "public", "9")]);
    net.store(&never);
    assert_eq!(
        net.call(
            Some(SUPER),
            "POST",
            "/_publish/rollback",
            rollback(&never, 2, "drill")
        )
        .0,
        403
    );
    let (status, _) = net.call(
        Some(SUPER),
        "POST",
        "/_publish/rollback",
        rollback(&old, 2, "Recovery drill"),
    );
    assert_eq!(status, 200);
    assert_eq!(net.state.active, Some(old.id.clone()));
    assert!(net.state.paused);
    assert_eq!(net.state.generation, 3);
    assert!(net.state.hidden.contains_key("a"));
    assert!(net.state.policies["b"].deleted);
    // A late rollback that started from the old generation loses.
    assert_eq!(
        net.call(
            Some(SUPER),
            "POST",
            "/_publish/rollback",
            rollback(&new, 2, "Late")
        )
        .0,
        409
    );
    assert_eq!(net.state.active, Some(old.id.clone()));
    // Automatic activation waits for an explicit resume.
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 3)
        )
        .0,
        409
    );
}

fn editor_authorization(net: &mut Net, commit: &str, origin: Option<&str>) {
    net.state.authorized.insert(
        commit.into(),
        Approval {
            actor: EDITOR.into(),
            pull: 7,
            at: None,
            head: Some("1".repeat(40)),
            base: None,
            origin: origin.map(String::from),
        },
    );
}

#[test]
fn an_editors_own_publish_lands_private_and_a_later_edit_neither_hides_nor_publishes_it() {
    let (mut net, _, new) = live_pair_with("draft");
    net.answer(
        "GET",
        &format!("/git/ref/heads/{BRANCH}"),
        200,
        json!({"object": {"sha": new.commit}}),
    );
    editor_authorization(&mut net, &new.commit, Some("editor"));
    // `b` was private before and is unchanged; `a` changed.
    net.state.private.insert(
        "b".into(),
        Private {
            actor: EDITOR.into(),
            since: "t".into(),
        },
    );
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 1)
        )
        .0,
        200
    );
    assert_eq!(net.state.private["a"].actor, EDITOR);
    assert!(net.state.private.contains_key("b"));

    // A later release approved by someone else only edits `a` (a new hash) and `b` stays as it was: neither is
    // published or withdrawn, so `a` stays private until an editor makes it public in the desk.
    let third = make_release(
        &"c".repeat(40),
        &[
            ("a", "published", "public", "4"),
            ("b", "published", "members", "1"),
        ],
    );
    net.store(&third);
    net.answer(
        "GET",
        &format!("/git/ref/heads/{BRANCH}"),
        200,
        json!({"object": {"sha": third.commit}}),
    );
    net.state.authorized.insert(
        third.commit.clone(),
        Approval {
            actor: "github:reviewer".into(),
            pull: 8,
            at: None,
            head: Some("2".repeat(40)),
            base: None,
            origin: None,
        },
    );
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&third, 2)
        )
        .0,
        200
    );
    assert!(net.state.private.contains_key("a"));
    assert!(net.state.private.contains_key("b"));
}

#[test]
fn a_correction_to_a_live_story_by_an_editor_does_not_make_it_private() {
    // `a` is already public; the editor's own release only changes its content.
    let (mut net, _, new) = live_pair();
    net.answer(
        "GET",
        &format!("/git/ref/heads/{BRANCH}"),
        200,
        json!({"object": {"sha": new.commit}}),
    );
    editor_authorization(&mut net, &new.commit, Some("editor"));
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 1)
        )
        .0,
        200
    );
    assert!(net.state.private.is_empty());
}

#[test]
fn an_editors_unchanged_items_are_not_made_private() {
    let (mut net, _, new) = live_pair_with("draft");
    net.answer(
        "GET",
        &format!("/git/ref/heads/{BRANCH}"),
        200,
        json!({"object": {"sha": new.commit}}),
    );
    editor_authorization(&mut net, &new.commit, Some("editor"));
    assert_eq!(
        net.call(
            Some(PUBLISHER),
            "POST",
            "/_publish/activate",
            activate_body(&new, 1)
        )
        .0,
        200
    );
    assert_eq!(net.state.private.keys().collect::<Vec<_>>(), vec!["a"]);
}

#[test]
fn sync_observes_pull_requests_reconciles_closed_ones_and_reports_readiness() {
    let (mut net, old, new) = live_pair();
    let head = "1".repeat(40);
    let pr = |number: u64, state: &str, merged: Option<&str>| {
        json!({
            "number": number, "state": state, "html_url": format!("u{number}"), "user": {"login": "writer"},
            "base": {"ref": BRANCH, "repo": {"full_name": "o/r"}},
            "head": {"sha": head, "ref": format!("b{number}"), "repo": {"full_name": "o/r"}},
            "changed_files": 1, "merged_at": merged, "merge_commit_sha": merged.map(|_| new.commit.clone()),
        })
    };
    net.answer(
        "GET",
        "/pulls?state=open&base=publish%2Fnews&per_page=100",
        200,
        json!([{"number": 5}, {"number": 6}]),
    );
    net.answer("GET", "/pulls/5", 200, pr(5, "open", None));
    net.answer(
        "GET",
        "/pulls/5/files?per_page=100",
        200,
        json!([{"filename": "content/site.yaml"}]),
    );
    // Pull request 6 touches a file outside the content root and is never built.
    net.answer("GET", "/pulls/6", 200, pr(6, "open", None));
    net.answer(
        "GET",
        "/pulls/6/files?per_page=100",
        200,
        json!([{"filename": ".github/workflows/x.yml"}]),
    );
    // Pull request 4 was observed before and has since been merged.
    net.state.edits.insert(
        4,
        Edit {
            number: 4,
            url: "u4".into(),
            head: "9".repeat(40),
            branch: "b4".into(),
            actor: "github:writer".into(),
            git_author: None,
            imported: true,
            state: EditState::Approved,
            path: None,
            contributors: vec![],
            commit: None,
            release: None,
            author_is_editor: false,
        },
    );
    net.state.approvals.insert(
        "9".repeat(40),
        Approval {
            actor: "github:reviewer".into(),
            pull: 4,
            at: None,
            head: Some("9".repeat(40)),
            base: None,
            origin: None,
        },
    );
    net.answer("GET", "/pulls/4", 200, pr(4, "closed", Some("2026-10-27")));
    net.answer(
        "GET",
        "/pulls/4/files?per_page=100",
        200,
        json!([{"filename": "content/site.yaml"}]),
    );
    net.approved_merge(&new.commit, &old.commit, "writer");
    // `approved_merge` replaced pull 7 only; the branch head is the merged commit and is approved.
    let (status, answer) = net.call(Some(SUPER), "POST", "/_publish/sync", json!({}));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["ready"], new.commit);
    assert_eq!(net.state.edits[&5].state, EditState::PendingReview);
    assert!(
        net.state.edits[&5]
            .contributors
            .contains(&"github:writer".to_string())
    );
    assert!(!net.state.edits.contains_key(&6));
    assert_eq!(net.state.edits[&4].state, EditState::Building);
    assert_eq!(
        net.state.edits[&4].commit.as_deref(),
        Some(new.commit.as_str())
    );
    assert!(
        !net.state.approvals.contains_key(&"9".repeat(40))
            || net.state.edits[&4].state == EditState::Building
    );
    assert_eq!(answer["previews"][0]["number"], 5);

    // Paused publication reports nothing ready, and a second sync is quiet.
    net.state.paused = true;
    let (_, answer) = net.call(Some(SUPER), "POST", "/_publish/sync", json!({}));
    assert_eq!(answer["ready"], Value::Null);
}

#[test]
fn unknown_actions_and_methods_fail() {
    let mut net = Net::new();
    assert_eq!(
        net.call(Some(PUBLISHER), "POST", "/_publish/nothing", json!({}))
            .0,
        404
    );
    assert_eq!(
        net.call(Some(PUBLISHER), "DELETE", "/_publish/state", json!({}))
            .0,
        405
    );
}
