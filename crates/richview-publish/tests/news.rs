//! RichView News on the Rust Publish: the real publication is built, staged, activated through the
//! bootstrap path and read back as an anonymous reader against an in-memory host.
use richview_publish::{Kind, Release, build};
use richview_publish_server::{host::*, ledger::*, model::State as ItemState, sha256};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

const PUBLISHER: &str = "service:publisher";
const COMMIT: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcd";
const HOST: &str = "news.example.test";

struct Host {
    cfg: Config,
    state: State,
    blobs: BTreeMap<String, Vec<u8>>,
    forge: BTreeMap<String, (u16, Value)>,
}
impl Host {
    fn send(&mut self, who: Option<&str>, method: &str, path: &str, body: Vec<u8>) -> Response {
        let principal = who.map(|key| Principal {
            key: key.into(),
            name: key.into(),
            email: None,
            service: key.starts_with("service:"),
        });
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let req = Request {
            method: method.into(),
            host: HOST.into(),
            path: path.into(),
            query: query
                .split('&')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
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
                "nonce",
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
                    if let Some(state) = done.state {
                        self.state = state;
                    }
                    return done.response;
                }
            }
        }
        panic!("the call never finished");
    }
    fn get(&mut self, path: &str) -> Response {
        self.send(None, "GET", path, vec![])
    }
    fn text(&mut self, path: &str) -> String {
        String::from_utf8(self.get(path).body).unwrap()
    }
}

fn header(r: &Response, name: &str) -> Option<String> {
    r.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tmp/publish-tests")
        .join(format!("news-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Builds `publications/news`, stages the result through the publisher API and activates it.
fn live_news(name: &str, edit: impl FnOnce(&mut Release)) -> (Host, Release) {
    let root = scratch(name);
    let runtime = root.join("runtime");
    fs::create_dir_all(&runtime).unwrap();
    fs::write(runtime.join("richview_runtime.js"), "/* fixture runtime */").unwrap();
    fs::write(runtime.join("richview_runtime_bg.wasm"), b"\0asm\x01\0\0\0").unwrap();
    let site = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../publications/news/site.yaml");
    let out = root.join("out");
    let mut release = build(&site, &out, &runtime, COMMIT).unwrap();
    edit(&mut release);
    // A mutated manifest is a new release: the server recomputes the identity over everything else.
    release.id = String::new();
    release.id = sha256(&serde_json::to_vec(&release).unwrap());
    let mut host = Host {
        cfg: Config {
            publication_host: HOST.into(),
            repository: "o/r".into(),
            branch: "publish/news".into(),
            content_root: "publications/news".into(),
            publisher_identity: PUBLISHER.into(),
            bootstrap_super_editor: "email:super@example.test".into(),
            bootstrap_github_login: None,
            bootstrap_commit: Some(COMMIT.into()),
        },
        state: State::default(),
        blobs: BTreeMap::new(),
        forge: BTreeMap::new(),
    };
    host.forge
        .insert("GET ".into(), (200, json!({"private": true})));
    host.forge.insert(
        "GET /git/ref/heads/publish/news".into(),
        (200, json!({"object": {"sha": COMMIT}})),
    );
    for hash in release.objects.keys() {
        let bytes = fs::read(out.join("objects").join(hash)).unwrap();
        let r = host.send(
            Some(PUBLISHER),
            "PUT",
            &format!("/_publish/objects/{hash}"),
            bytes,
        );
        assert_eq!(r.status, 200, "upload {hash}");
    }
    let staged = host.send(
        Some(PUBLISHER),
        "POST",
        "/_publish/stage",
        serde_json::to_vec(&release).unwrap(),
    );
    assert_eq!(
        staged.status,
        200,
        "{}",
        String::from_utf8_lossy(&staged.body)
    );
    let body = json!({"release": release.id, "generation": host.state.generation});
    let live = host.send(
        Some(PUBLISHER),
        "POST",
        "/_publish/activate",
        serde_json::to_vec(&body).unwrap(),
    );
    assert_eq!(live.status, 200, "{}", String::from_utf8_lossy(&live.body));
    assert_eq!(host.state.active.as_deref(), Some(release.id.as_str()));
    let _ = fs::remove_dir_all(&root);
    (host, release)
}

fn is_public(item: &richview_publish::Item) -> bool {
    item.meta.state == ItemState::Published && item.meta.audience == "public"
}

#[test]
fn news_builds_stages_activates_and_reads_as_an_anonymous_reader() {
    let (mut host, release) = live_news("reader", |_| {});
    let public: Vec<_> = release.items.iter().filter(|i| is_public(i)).collect();
    let hidden: Vec<_> = release.items.iter().filter(|i| !is_public(i)).collect();
    assert!(public.len() >= 5, "News publishes stories");
    assert!(!hidden.is_empty(), "News keeps a draft or members item");

    let home = host.get("/");
    assert_eq!(home.status, 200);
    let home = String::from_utf8(home.body).unwrap();
    assert!(home.contains("RichView News"));
    for lead in &release.site.lead {
        let item = release.items.iter().find(|i| &i.meta.id == lead).unwrap();
        assert!(
            home.contains(&item.title),
            "the lead {lead} is on the home page"
        );
    }

    for item in &public {
        let page = host.get(&item.route);
        assert_eq!(page.status, 200, "{}", item.route);
        assert!(
            String::from_utf8_lossy(&page.body).contains(&item.title),
            "{} carries its title",
            item.route
        );
        let redirect = host.get(&format!("/r/{}", item.meta.id));
        assert_eq!(redirect.status, 308, "/r/{}", item.meta.id);
        assert_eq!(
            header(&redirect, "location").as_deref(),
            Some(item.route.as_str())
        );
        let frame = host.get(&format!("/_content/{}/report.html", item.meta.id));
        assert_eq!(frame.status, 200, "{}", item.meta.id);
        let csp = header(&frame, "content-security-policy").unwrap();
        assert!(csp.starts_with("sandbox allow-scripts"), "{csp}");
        assert!(csp.contains("default-src 'none'"));
        assert_eq!(header(&frame, "x-robots-tag").as_deref(), Some("noindex"));
    }

    let feed = host.text("/feed.xml");
    let search = host.text("/search.json");
    let sitemap = host.text("/sitemap.xml");
    let syndicated: Vec<_> = public
        .iter()
        .filter(|i| i.meta.kind != Kind::Page)
        .collect();
    assert!(!syndicated.is_empty());
    for item in &syndicated {
        assert!(feed.contains(&format!("{}#{}", release.site.base_url, item.meta.id)));
        assert!(search.contains(&format!("\"{}\"", item.meta.id)));
        assert!(sitemap.contains(&format!("{}{}", release.site.base_url, item.route)));
    }
    assert!(
        host.text("/robots.txt")
            .contains("Sitemap: https://news.richview.ai/sitemap.xml")
    );

    let status: Value = serde_json::from_str(&host.text("/_status")).unwrap();
    assert_eq!(status["state"], "live");
    assert_eq!(status["release"], release.id);
    assert_eq!(status["commit"], COMMIT);

    for item in &hidden {
        assert_eq!(host.get(&item.route).status, 404, "{}", item.route);
        assert_eq!(host.get(&format!("/r/{}", item.meta.id)).status, 404);
        assert_eq!(
            host.get(&format!("/_content/{}/report.html", item.meta.id))
                .status,
            404
        );
        for body in [&home, &feed, &search, &sitemap] {
            assert!(!body.contains(&item.title), "{} leaked", item.meta.id);
        }
        assert!(!host.text("/archive").contains(&item.title));
    }
}

#[test]
fn a_draft_added_to_a_live_news_release_stays_off_every_public_surface() {
    let (mut host, release) = live_news("draft", |r| {
        let mut draft = r
            .items
            .iter()
            .find(|i| is_public(i) && i.meta.kind == Kind::Report)
            .unwrap()
            .clone();
        draft.meta.id = "extra-draft".into();
        draft.meta.slug = "extra-draft".into();
        draft.meta.aliases.clear();
        draft.meta.state = ItemState::Draft;
        draft.title = "Unreleased extra draft".into();
        draft.route = "/stories/extra-draft".into();
        r.items.push(draft);
    });
    assert!(release.items.iter().any(|i| i.meta.id == "extra-draft"));
    assert_eq!(host.get("/stories/extra-draft").status, 404);
    assert_eq!(host.get("/r/extra-draft").status, 404);
    assert_eq!(host.get("/_content/extra-draft/report.html").status, 404);
    for path in ["/", "/archive", "/feed.xml", "/sitemap.xml", "/search.json"] {
        let body = host.text(path);
        assert!(!body.contains("Unreleased extra draft"), "{path}");
        assert!(!body.contains("extra-draft"), "{path}");
    }
}
