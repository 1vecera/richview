//! The reader routes against an in-memory host: a release in `Io.blobs`, a ledger `State`, and requests through `handle`.
use richview_publish_server::{access::*, host::*, ledger::*, *};
use serde_json::{Value, json};
use std::{collections::BTreeMap, rc::Rc};

const RELEASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OLD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn item(id: &str, audience: &str, state: &str) -> Value {
    json!({
        "id": id, "slug": id, "summary": "Summary", "byline": "Writer", "section": "research",
        "tags": [], "published": "2026-10-18T09:00:00Z", "updated": "2026-10-18T09:00:00Z",
        "state": state, "audience": audience, "kind": "report", "aliases": [],
        "title": format!("Title of {id}"), "source": format!("items/{id}/report.rmd"),
        "route": format!("/stories/{id}"), "article": "article", "content": "content",
        "card": format!("<article>Card of {id}</article>"), "lead": format!("<article>Lead of {id}</article>"),
        "assets": {"cover": "cover", "og.png": "og"},
    })
}
fn object(kind: &str) -> Value {
    json!({"sha256": "x", "bytes": 1, "content_type": kind})
}

struct World {
    cfg: Config,
    state: State,
    blobs: BTreeMap<String, String>,
    manifest: Value,
}
impl World {
    fn new() -> World {
        let mut report = item("report", "public", "published");
        report["aliases"] = json!(["old-slug"]);
        report["title"] = json!("Public finding");
        let mut member = item("member", "members", "published");
        member["title"] = json!("Secret member title");
        member["article"] = json!("private");
        member["content"] = json!("private");
        let mut draft = item("draft", "members", "draft");
        draft["title"] = json!("Unpublished title");
        let manifest = json!({
            "schema": 1, "id": RELEASE, "commit": "c".repeat(40), "toolchain": "t",
            "site": {
                "title": "News", "description": "Example", "base_url": "https://news.example.test",
                "layout": "broadsheet", "mode": "external", "sections": [],
                "navigation": [], "lead": ["report"],
            },
            "items": [report, member, draft],
            "templates": {"home": "home", "search": "home", "archive": "home", "section:research": "home"},
            "runtime": {"richview_runtime.js": "rtjs", "richview_runtime_bg.wasm": "rtwasm"},
            "objects": {
                "content": object("text/html; charset=utf-8"), "article": object("text/html; charset=utf-8"),
                "private": object("text/html; charset=utf-8"), "cover": object("image/svg+xml"),
                "og": object("image/png"),
                "data": object("application/octet-stream"),
            },
        });
        let mut state = State {
            generation: 1,
            active: Some(RELEASE.into()),
            ..State::default()
        };
        for i in manifest["items"].as_array().unwrap() {
            state.policies.insert(
                i["id"].as_str().unwrap().into(),
                serde_json::from_value(json!({
                    "id": i["id"], "section": "research", "state": i["state"], "audience": i["audience"],
                }))
                .unwrap(),
            );
        }
        state.activated.insert(RELEASE.into(), "c".repeat(40));
        state.activated.insert(OLD.into(), "c".repeat(40));
        state.roles.insert(
            "email:editor@example.test".into(),
            Access {
                name: RoleName::Editor,
                sections: vec!["research".into()],
                audiences: vec!["members".into()],
                github_login: None,
                byline: None,
            },
        );
        state.roles.insert(
            "email:outsider@example.test".into(),
            Access {
                name: RoleName::Reader,
                sections: vec![],
                audiences: vec![],
                github_login: None,
                byline: None,
            },
        );
        let mut w = World {
            cfg: Config {
                publication_host: "news.example.test".into(),
                publisher_identity: "email:publisher@example.test".into(),
                bootstrap_super_editor: "email:super@example.test".into(),
                ..Config::default()
            },
            state,
            blobs: BTreeMap::new(),
            manifest,
        };
        for (key, text) in [
            ("content", "<h1>Public report body</h1>"),
            ("article", "<h1>Public finding</h1>"),
            ("private", "<h1>Secret member title</h1>"),
            ("cover", "<svg/>"),
            ("og", "PNG"),
            ("rtjs", "export default 1"),
            ("rtwasm", "wasm"),
            (
                "home",
                "<main class=\"page\"><!--PUBLISH:LEAD--><!--PUBLISH:ITEMS--><!--PUBLISH:QUERY--></main>",
            ),
        ] {
            w.blobs.insert(format!("objects/{key}"), text.into());
        }
        w.republish();
        w
    }
    /// Stores the manifest as the live release and as an older release under the same content.
    fn republish(&mut self) {
        let text = self.manifest.to_string();
        self.blobs.insert(format!("releases/{RELEASE}.json"), text);
        let mut old = self.manifest.clone();
        old["id"] = json!(OLD);
        self.blobs
            .insert(format!("releases/{OLD}.json"), old.to_string());
    }
    fn set_object(&mut self, key: &str, text: &str) {
        self.blobs.insert(format!("objects/{key}"), text.into());
    }
    fn get(&self, path: &str, who: Option<&str>) -> Response {
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let req = Request {
            method: "GET".into(),
            host: "news.example.test".into(),
            path: path.into(),
            query: query
                .split('&')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.into(), v.replace('+', " ")))
                .collect(),
            ..Request::default()
        };
        let principal = who.map(|w| Principal {
            key: format!("email:{w}"),
            name: w.into(),
            email: Some(w.into()),
            service: false,
        });
        let mut io = Io::default();
        for _ in 0..20 {
            match handle(
                &self.cfg,
                principal.as_ref(),
                &self.state,
                &io,
                "2026-10-27T00:00:00Z",
                "nonce",
                &req,
            ) {
                Outcome::Done(done) => return done.response,
                Outcome::Need(needs) => {
                    for need in needs {
                        let Need::Blob { key } = need else {
                            panic!("unexpected need {need:?}")
                        };
                        let bytes = self
                            .blobs
                            .get(&key)
                            .map(|t| Rc::new(t.clone().into_bytes()));
                        io.blobs.insert(key, bytes);
                    }
                }
            }
        }
        panic!("the call never finished");
    }
    fn status(&self, path: &str, who: Option<&str>) -> u16 {
        self.get(path, who).status
    }
    fn text(&self, path: &str, who: Option<&str>) -> String {
        String::from_utf8(self.get(path, who).body).unwrap()
    }
}
fn header(r: &Response, name: &str) -> Option<String> {
    r.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}
fn directive(policy: &str, name: &str) -> Option<String> {
    policy
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with(&format!("{name} ")))
        .map(String::from)
}

#[test]
fn public_navigation_search_and_feeds_never_disclose_draft_or_member_metadata() {
    let w = World::new();
    for path in [
        "/",
        "/archive",
        "/search?q=Secret",
        "/sections/research",
        "/feed.xml",
        "/sitemap.xml",
        "/search.json",
    ] {
        let r = w.get(path, None);
        assert_eq!(r.status, 200, "{path}");
        let body = String::from_utf8(r.body.clone()).unwrap();
        assert!(!body.contains("Secret member title") && !body.contains("Card of member"));
        assert!(!body.contains("Unpublished title") && !body.contains("Card of draft"));
        assert!(header(&r, "Cache-Control").unwrap().contains("no-store"));
    }
}

#[test]
fn syndication_is_public_even_for_an_editor_and_dates_items_for_rss() {
    let w = World::new();
    for who in [
        None,
        Some("reader@example.test"),
        Some("super@example.test"),
    ] {
        let feed = w.text("/feed.xml", who);
        assert!(feed.contains("<title>Public finding</title>"));
        assert!(feed.contains("<pubDate>Sun, 18 Oct 2026 09:00:00 GMT</pubDate>"));
        assert!(feed.contains("<link>https://news.example.test/stories/report</link>"));
        assert!(!feed.contains("Secret member title"));
        let search: Value = serde_json::from_str(&w.text("/search.json", who)).unwrap();
        assert_eq!(search.as_array().unwrap().len(), 1);
        assert_eq!(search[0]["id"], "report");
        assert!(w.text("/sitemap.xml", who).contains("/stories/report"));
    }
}

#[test]
fn pages_are_never_syndicated_and_the_member_edition_has_no_feed() {
    let mut w = World::new();
    w.manifest["items"][0]["kind"] = json!("page");
    w.manifest["items"][0]["route"] = json!("/about");
    w.republish();
    assert!(!w.text("/feed.xml", None).contains("Public finding"));
    assert!(!w.text("/search.json", None).contains("report"));
    w.manifest["items"][0]["kind"] = json!("report");
    w.manifest["items"][0]["route"] = json!("/stories/report");
    w.republish();
    assert!(w.text("/search.json", None).contains("report"));
    assert!(
        !w.text("/internal/feed.xml", Some("reader@example.test"))
            .contains("Public finding")
    );
}

#[test]
fn member_entitlement_protects_content_bytes_assets_and_old_releases() {
    let w = World::new();
    for path in [
        "/stories/member".to_string(),
        "/_content/member/report.html".into(),
        format!("/_releases/{OLD}/stories/member"),
    ] {
        assert_eq!(w.status(&path, None), 404, "{path}");
    }
    assert_eq!(
        w.status("/stories/member", Some("reader@example.test")),
        200
    );
    assert_eq!(
        w.status("/_content/member/report.html", Some("reader@example.test")),
        200
    );
    assert_eq!(w.status("/stories/draft", Some("reader@example.test")), 404);
}

#[test]
fn only_scoped_editors_may_view_previews_including_drafts() {
    let mut w = World::new();
    w.set_object("article", "<main class=\"story\">Draft</main>");
    let path = format!("/_preview/{RELEASE}/stories/draft");
    assert_eq!(w.status(&path, None), 404);
    assert_eq!(w.status(&path, Some("reader@example.test")), 404);
    let page = w.get(&path, Some("editor@example.test"));
    assert_eq!(page.status, 200);
    assert!(
        String::from_utf8(page.body)
            .unwrap()
            .contains("Protected preview")
    );
    // An editor outside the item's section sees nothing.
    let mut w = World::new();
    w.state
        .roles
        .get_mut("email:editor@example.test")
        .unwrap()
        .sections = vec!["other".into()];
    assert_eq!(w.status(&path, Some("editor@example.test")), 404);
}

#[test]
fn the_internal_edition_requires_membership_at_the_first_route() {
    let w = World::new();
    assert_eq!(w.status("/internal", None), 401);
    assert_eq!(w.status("/internal", Some("outsider@example.test")), 401);
    assert_eq!(w.status("/internal", Some("reader@example.test")), 200);
    let mut w = World::new();
    w.set_object("home", "<main ><!--PUBLISH:ITEMS--></main><a href=\"/stories/report\">x</a><a href=\"/internal\">y</a>");
    let page = w.text("/internal", Some("reader@example.test"));
    assert!(page.contains("Member edition"));
    assert!(page.contains("href=\"/internal/stories/report\""));
    assert!(page.contains("href=\"/internal\">y"));
    assert!(page.contains("Card of member"));
    assert_eq!(
        w.status("/internal/stories/member", Some("reader@example.test")),
        200
    );
}

#[test]
fn a_wholly_internal_publication_protects_its_root_and_public_labelled_bytes() {
    let mut w = World::new();
    w.manifest["site"]["mode"] = json!("internal");
    w.republish();
    for path in ["/", "/stories/report", "/_content/report/report.html"] {
        assert_eq!(w.status(path, None), 401, "{path}");
        assert_eq!(w.status(path, Some("outsider@example.test")), 401, "{path}");
        assert_eq!(w.status(path, Some("reader@example.test")), 200, "{path}");
    }
    assert!(
        w.text("/robots.txt", Some("reader@example.test"))
            .contains("Disallow: /\n")
    );
    assert!(
        !w.text("/feed.xml", Some("reader@example.test"))
            .contains("Public finding")
    );
    let page = w.get("/", Some("reader@example.test"));
    assert_eq!(header(&page, "X-Robots-Tag").as_deref(), Some("noindex"));
}

#[test]
fn a_dashboard_in_a_stored_release_is_gone_and_does_not_break_the_release() {
    // The live News release still lists a dashboard item with a snapshot (FEA-BR-30 retired them).
    let mut w = World::new();
    let mut legacy = item("trading-desk", "public", "published");
    legacy["kind"] = json!("dashboard");
    legacy["snapshot"] = json!("snapshot.json");
    legacy["route"] = json!("/dashboards/trading-desk");
    legacy["aliases"] = json!(["old-desk"]);
    legacy["card"] = json!("<article>Trading desk card</article>");
    w.manifest["items"].as_array_mut().unwrap().push(legacy);
    w.state.policies.insert(
        "trading-desk".into(),
        richview_publish_server::ledger::Policy {
            id: "trading-desk".into(),
            section: "research".into(),
            state: richview_publish_server::model::State::Published,
            audience: "public".into(),
            deleted: false,
        },
    );
    w.republish();
    // The release still loads: status, the front page and the other stories answer.
    assert_eq!(w.status("/_status", None), 200);
    assert_eq!(w.status("/stories/report", None), 200);
    for who in [
        None,
        Some("editor@example.test"),
        Some("super@example.test"),
    ] {
        for path in [
            "/dashboards/trading-desk",
            "/dashboards/old-desk",
            "/r/trading-desk",
        ] {
            assert_eq!(w.status(path, who), 410, "{path}");
        }
        assert_eq!(w.status("/_content/trading-desk/cover", who), 404);
    }
    for path in ["/", "/archive", "/feed.xml", "/search.json", "/sitemap.xml"] {
        assert!(!w.text(path, None).contains("Trading desk"), "{path}");
    }
}

#[test]
fn an_urgent_hide_denies_every_route_at_once() {
    let mut w = World::new();
    w.state.hidden.insert(
        "report".into(),
        Hide {
            revision: "r".into(),
            actor: "email:editor@example.test".into(),
            reason: "Correction drill".into(),
            at: "2026-10-27T00:00:00Z".into(),
            reconciliation: "pending".into(),
            pull: None,
        },
    );
    for path in [
        "/stories/report".to_string(),
        "/stories/old-slug".into(),
        "/r/report".into(),
        "/_content/report/report.html".into(),
        "/_content/report/cover".into(),
        format!("/_releases/{OLD}/stories/report"),
        format!("/_releases/{OLD}/_content/report/cover"),
        format!("/_preview/{RELEASE}/stories/report"),
    ] {
        assert_eq!(w.status(&path, Some("editor@example.test")), 404, "{path}");
        assert_eq!(w.status(&path, Some("super@example.test")), 404, "{path}");
    }
    w.manifest["items"][0]["card"] = json!("<article>Public finding</article>");
    w.republish();
    for path in ["/", "/feed.xml", "/search.json", "/sitemap.xml"] {
        assert!(!w.text(path, None).contains("Public finding"), "{path}");
    }
}

#[test]
fn an_old_release_cannot_weaken_a_current_audience_or_resurrect_a_deletion() {
    let mut w = World::new();
    let old = format!("/_releases/{OLD}/stories/report");
    w.state.policies.get_mut("report").unwrap().audience = "members".into();
    assert_eq!(w.status(&old, None), 404);
    assert_eq!(w.status(&old, Some("reader@example.test")), 200);
    w.state.policies.get_mut("report").unwrap().deleted = true;
    assert_eq!(w.status(&old, Some("super@example.test")), 404);
    // A release that never went live is not history.
    w.state.activated.remove(OLD);
    w.state.policies.get_mut("report").unwrap().deleted = false;
    assert_eq!(w.status(&old, Some("super@example.test")), 404);
}

#[test]
fn rollback_keeps_hides_and_access_floors() {
    let mut w = World::new();
    w.state.active = Some(OLD.into());
    w.state.generation = 2;
    w.state.policies.get_mut("member").unwrap().deleted = true;
    w.state.hidden.insert(
        "report".into(),
        Hide {
            revision: "r".into(),
            actor: "x".into(),
            reason: "hold".into(),
            at: "t".into(),
            reconciliation: "pending".into(),
            pull: None,
        },
    );
    assert_eq!(w.status("/stories/report", None), 404);
    assert_eq!(w.status("/stories/member", Some("super@example.test")), 404);
    let status: Value = serde_json::from_str(&w.text("/_status", None)).unwrap();
    assert_eq!(status["release"], OLD);
    assert_eq!(status["generation"], 2);
}

#[test]
fn status_names_the_live_release_and_nothing_about_items() {
    let mut w = World::new();
    let status: Value = serde_json::from_str(&w.text("/_status", None)).unwrap();
    assert_eq!(
        status,
        json!({"state": "live", "release": RELEASE, "generation": 1, "paused": false, "commit": "c".repeat(40)})
    );
    w.state.active = None;
    let status: Value = serde_json::from_str(&w.text("/_status", None)).unwrap();
    assert_eq!(status["state"], "unpublished");
    assert_eq!(status["commit"], Value::Null);
    let r = w.get("/", None);
    assert_eq!(r.status, 503);
    assert!(
        String::from_utf8(r.body)
            .unwrap()
            .contains("The first publication is being prepared.")
    );
}

#[test]
fn the_front_page_shows_one_lead_a_dateline_and_only_figures_the_reader_may_see() {
    let mut w = World::new();
    w.set_object(
        "home",
        "<main><!--PUBLISH:DATELINE--><!--PUBLISH:FIGURES--><!--PUBLISH:LEAD--><!--PUBLISH:ITEMS--></main>",
    );
    w.manifest["items"][0]["stat"] = json!("<a class=\"figure\">42%</a>");
    w.manifest["items"][1]["stat"] = json!("<a class=\"figure\">Secret member figure</a>");
    w.manifest["site"]["lead"] = json!(["report", "member"]);
    w.republish();
    let anonymous = w.text("/", None);
    assert!(
        anonymous.contains(
            "Updated <time datetime=\"2026-10-18T09:00:00Z\">18 Oct 2026</time> · 1 story"
        )
    );
    assert!(anonymous.contains("class=\"figures\"") && anonymous.contains("42%"));
    assert!(!anonymous.contains("Secret member figure"));
    let reader = w.text("/", Some("reader@example.test"));
    assert!(
        reader.contains("Lead of report") && !reader.contains("Lead of member"),
        "one lead only"
    );
    assert!(reader.contains("Secret member figure"));
    assert!(reader.contains("2 stories"));
}

#[test]
fn related_stories_and_archive_months_are_assembled_per_reader() {
    let mut w = World::new();
    w.set_object("article", "<h1>Public finding</h1><!--PUBLISH:MORE-->");
    w.manifest["site"]["sections"] =
        json!([{"id": "research", "title": "Research", "description": ""}]);
    let mut older = item("older", "public", "published");
    older["published"] = json!("2026-09-02T09:00:00Z");
    older["card"] = json!("<article>Older public finding</article>");
    w.manifest["items"].as_array_mut().unwrap().push(older);
    w.republish();
    let story = w.text("/stories/report", None);
    assert!(story.contains("More in Research") && story.contains("Older public finding"));
    assert!(!story.contains("Card of member"));
    assert!(
        !story.contains("Card of report"),
        "a story is not related to itself"
    );
    assert!(
        w.text("/stories/report", Some("reader@example.test"))
            .contains("Card of member")
    );
    let archive = w.text("/archive", None);
    let october = archive
        .find("<h2 class=\"month\">October 2026</h2>")
        .unwrap();
    let september = archive
        .find("<h2 class=\"month\">September 2026</h2>")
        .unwrap();
    assert!(
        october < archive.find("Card of report").unwrap()
            && archive.find("Card of report").unwrap() < september
    );
    assert!(archive.find("Older public finding").unwrap() > september);
    assert!(
        w.text("/search?q=nothing-here", None)
            .contains("Nothing matches “nothing-here”")
    );
    assert!(w.text("/search?q=finding", None).contains("Card of report"));
    assert!(!w.text("/search?q=finding", None).contains("Card of member"));
}

#[test]
fn sections_page_and_search_use_their_templates() {
    let mut w = World::new();
    w.set_object(
        "home",
        "<main><!--PUBLISH:ITEMS--><!--PUBLISH:PAGINATION--></main>",
    );
    assert!(
        w.text("/sections/research", None)
            .contains("Card of report")
    );
    w.manifest["items"] = json!([]);
    w.republish();
    assert!(
        w.text("/sections/research", None)
            .contains("Nothing has been published in this section yet.")
    );
    assert_eq!(w.status("/sections/other", None), 404);
    w.manifest["templates"]
        .as_object_mut()
        .unwrap()
        .remove("section:research");
    w.republish();
    assert_eq!(w.status("/sections/research", None), 404);
}

#[test]
fn aliases_and_ids_redirect_permanently_to_the_current_route() {
    let w = World::new();
    for path in ["/stories/old-slug", "/r/report"] {
        let r = w.get(path, None);
        assert_eq!(r.status, 308, "{path}");
        assert_eq!(header(&r, "Location").as_deref(), Some("/stories/report"));
    }
    let r = w.get(
        &format!("/_preview/{RELEASE}/r/draft"),
        Some("editor@example.test"),
    );
    assert_eq!(
        header(&r, "Location"),
        Some(format!("/_preview/{RELEASE}/stories/draft"))
    );
    let r = w.get("/r/report", Some("reader@example.test"));
    assert_eq!(header(&r, "Location").as_deref(), Some("/stories/report"));
    let r = w.get("/internal/r/report", Some("reader@example.test"));
    assert_eq!(
        header(&r, "Location").as_deref(),
        Some("/internal/stories/report")
    );
    assert_eq!(w.status("/r/missing", None), 404);
}

#[test]
fn robots_and_the_reading_script_are_served() {
    let w = World::new();
    let robots = w.text("/robots.txt", None);
    assert!(
        robots.contains(
            "Allow: /_content/*/og.png\nAllow: /_releases/*/_content/*/og.png\nDisallow: /_\n"
        ) && robots.contains("Disallow: /_\n")
            && robots.contains("Sitemap: https://news.example.test/sitemap.xml")
    );
    let js = w.get("/_reading.js", None);
    assert_eq!(js.status, 200);
    assert!(
        header(&js, "Content-Type")
            .unwrap()
            .starts_with("text/javascript")
    );
    assert!(
        String::from_utf8(js.body)
            .unwrap()
            .contains("richview:height")
    );
}

#[test]
fn runtimes_are_served_from_the_release_or_the_ledger_and_nothing_else() {
    let mut w = World::new();
    let id = sha256(&serde_json::to_vec(&w.manifest["runtime"]).unwrap());
    let js = w.get(&format!("/_runtime/{id}/richview_runtime.js"), None);
    assert_eq!(js.status, 200);
    assert_eq!(
        header(&js, "Access-Control-Allow-Origin").as_deref(),
        Some("*")
    );
    let wasm = w.get(&format!("/_runtime/{id}/richview_runtime_bg.wasm"), None);
    assert_eq!(
        header(&wasm, "Content-Type").as_deref(),
        Some("application/wasm")
    );
    assert_eq!(w.status("/_runtime/unknown/richview_runtime.js", None), 404);
    assert_eq!(w.status(&format!("/_runtime/{id}/other.js"), None), 404);
    assert_eq!(
        w.status(&format!("/_runtime/{id}/richview_runtime.js/extra"), None),
        404
    );
    w.state.runtimes.insert(
        "kept".into(),
        BTreeMap::from([("richview_runtime.js".to_string(), "rtjs".to_string())]),
    );
    assert_eq!(w.status("/_runtime/kept/richview_runtime.js", None), 200);
    assert_eq!(
        w.status("/_runtime/kept/richview_runtime_bg.wasm", None),
        404
    );
}

#[test]
fn a_kept_runtime_from_before_the_rename_is_still_served_under_its_old_names() {
    let mut w = World::new();
    w.state.runtimes.insert(
        "old".into(),
        BTreeMap::from([
            ("richview_web.js".to_string(), "rtjs".to_string()),
            ("richview_web_bg.wasm".to_string(), "rtwasm".to_string()),
        ]),
    );
    assert_eq!(w.status("/_runtime/old/richview_web.js", None), 200);
    let wasm = w.get("/_runtime/old/richview_web_bg.wasm", None);
    assert_eq!(wasm.status, 200);
    assert_eq!(
        header(&wasm, "Content-Type").as_deref(),
        Some("application/wasm")
    );
    // A name the runtime does not hold is still refused.
    assert_eq!(w.status("/_runtime/old/richview_runtime.js", None), 404);
}

#[test]
fn pages_allow_same_origin_fonts_and_keep_every_other_policy() {
    let w = World::new();
    let csp = header(&w.get("/", None), "Content-Security-Policy").unwrap();
    assert_eq!(
        directive(&csp, "font-src").as_deref(),
        Some("font-src 'self'")
    );
    assert_eq!(
        directive(&csp, "default-src").as_deref(),
        Some("default-src 'none'")
    );
    assert_eq!(
        directive(&csp, "script-src").as_deref(),
        Some("script-src 'self'")
    );
    assert_eq!(
        directive(&csp, "frame-ancestors").as_deref(),
        Some("frame-ancestors 'self'")
    );
    assert_eq!(
        directive(&csp, "img-src").as_deref(),
        Some("img-src 'self' data: https://images.unsplash.com")
    );
    let frame = w.get("/_content/report/report.html?embed=1", None);
    assert!(
        header(&frame, "Content-Security-Policy")
            .unwrap()
            .starts_with(
                "sandbox allow-scripts allow-downloads allow-popups allow-popups-to-escape-sandbox;"
            )
    );
}

#[test]
fn a_report_frame_allows_unsplash_only_when_its_html_uses_a_photo() {
    let mut w = World::new();
    let img = |path: &str| {
        header(&w.get(path, None), "Content-Security-Policy").and_then(|c| directive(&c, "img-src"))
    };
    let plain = header(
        &w.get("/_content/report/report.html?embed=1", None),
        "Content-Security-Policy",
    )
    .unwrap();
    assert_eq!(
        directive(&plain, "img-src").as_deref(),
        Some("img-src 'self' data:")
    );
    assert_eq!(
        directive(&plain, "connect-src").as_deref(),
        Some("connect-src 'self'")
    );
    assert!(
        !plain.contains("unsplash")
            && !plain.contains("allow-same-origin")
            && !plain.contains("allow-top-navigation")
    );
    assert!(img("/_content/report/report.html").is_some());
    w.set_object(
        "content",
        "<html><head></head><body><img src=\"https://images.unsplash.com/photo-1?w=1600\" alt=\"Sea\"></body></html>",
    );
    let img = |path: &str, who: Option<&str>| {
        header(&w.get(path, who), "Content-Security-Policy").and_then(|c| directive(&c, "img-src"))
    };
    let photo = "img-src 'self' data: https://images.unsplash.com";
    assert_eq!(
        img("/_content/report/report.html?embed=1", None).as_deref(),
        Some(photo)
    );
    assert_eq!(
        img(
            &format!("/_preview/{RELEASE}/_content/report/report.html"),
            Some("editor@example.test")
        )
        .as_deref(),
        Some(photo)
    );
    for body in [
        "<img src=\"https://example.org/a.jpg\">",
        "<img src=\"https://images.unsplash.com.evil.test/a.jpg\">",
        "<img src=\"//images.unsplash.com/a.jpg\">",
        "<p>images.unsplash.com</p>",
        "<p>https://images.unsplash.com/photo-1 and src=&quot;https://images.unsplash.com/photo-1&quot;</p>",
        "<a href=\"https://images.unsplash.com/photo-1\">photo</a>",
        "<img data-src=\"https://images.unsplash.com/photo-1\">",
    ] {
        w.set_object(
            "content",
            &format!("<html><head></head><body>{body}</body></html>"),
        );
        assert_eq!(
            header(
                &w.get("/_content/report/report.html", None),
                "Content-Security-Policy"
            )
            .and_then(|c| directive(&c, "img-src"))
            .as_deref(),
            Some("img-src 'self' data:"),
            "{body}"
        );
    }
    // A stored image stays script-free and cannot load remote pictures.
    let image = header(
        &w.get("/_content/report/cover", None),
        "Content-Security-Policy",
    )
    .unwrap();
    assert!(image.starts_with("sandbox; ") && image.contains("img-src data:;"));
}

#[test]
fn embedded_frames_get_the_embed_style_and_a_height_reporter() {
    let mut w = World::new();
    w.set_object("content", "<html><head></head><body><p>x</p></body></html>");
    let plain = w.text("/_content/report/report.html", None);
    assert!(!plain.contains("<style>") && plain.contains("richview:height"));
    let paper = w.text("/_content/report/report.html?embed=1", None);
    assert!(
        paper.contains("background:transparent!important")
            && paper.contains("font-family:\"Newsreader\"")
    );
    w.manifest["site"]["theme"] = json!("ink");
    w.republish();
    let ink = w.text("/_content/report/report.html?embed=1", None);
    assert!(!ink.contains("background:transparent") && ink.contains("28px 28px 32px"));
    assert_eq!(w.status("/_content/report/missing", None), 404);
    assert_eq!(w.status("/_content/nobody/report.html", None), 404);
}

#[test]
fn downloads_are_attachments() {
    let mut w = World::new();
    w.manifest["items"][0]["assets"] = json!({"data": "data"});
    w.republish();
    w.set_object("data", "a,b");
    let r = w.get("/_content/report/data", None);
    assert_eq!(
        header(&r, "Content-Disposition").as_deref(),
        Some("attachment")
    );
    assert_eq!(header(&r, "X-Robots-Tag").as_deref(), Some("noindex"));
}

#[test]
fn previews_and_member_routes_keep_report_runtimes_on_the_shared_root_path() {
    let mut w = World::new();
    w.set_object(
        "content",
        "<html><head></head><body><script src=\"/_runtime/abc/richview_runtime.js\"></script><a href=\"/stories/report\">x</a></body></html>",
    );
    let preview = w.text(
        &format!("/_preview/{RELEASE}/_content/report/report.html"),
        Some("editor@example.test"),
    );
    assert!(preview.contains("src=\"/_runtime/abc/richview_runtime.js\""));
    assert!(preview.contains(&format!("href=\"/_preview/{RELEASE}/stories/report\"")));
    let old = w.text(
        &format!("/_releases/{OLD}/_content/report/report.html"),
        None,
    );
    assert!(old.contains(&format!("href=\"/_releases/{OLD}/stories/report\"")));
}

#[test]
fn rewriting_leaves_hotlinked_photos_and_their_credits_alone() {
    let mut w = World::new();
    let photo = "https://images.unsplash.com/photo-1?ixid=M3w&amp;w=1600";
    let profile = "https://unsplash.com/@marulla?utm_source=richview&amp;utm_medium=referral";
    w.set_object(
        "content",
        &format!("<html><head></head><body><img src=\"{photo}\"><a href=\"{profile}\" target=\"_blank\">M</a><img src=\"//images.unsplash.com/p\"><a href=\"/stories/report\">x</a><form action=\"/search\"></form></body></html>"),
    );
    let preview = w.text(
        &format!("/_preview/{RELEASE}/_content/report/report.html"),
        Some("editor@example.test"),
    );
    assert!(preview.contains(&format!("<img src=\"{photo}\">")));
    assert!(preview.contains(&format!("<a href=\"{profile}\" target=\"_blank\">")));
    assert!(preview.contains("<img src=\"//images.unsplash.com/p\">"));
    assert!(preview.contains(&format!("href=\"/_preview/{RELEASE}/stories/report\"")));
    assert!(preview.contains(&format!("action=\"/_preview/{RELEASE}/search\"")));
}

#[test]
fn an_editors_own_private_item_is_hidden_from_readers_and_feeds() {
    let mut w = World::new();
    w.manifest["items"].as_array_mut().unwrap().push({
        let mut i = item("fresh", "public", "published");
        i["title"] = json!("Fresh private story");
        i
    });
    w.republish();
    w.state.policies.insert(
        "fresh".into(),
        serde_json::from_value(json!({"id": "fresh", "section": "research", "state": "published", "audience": "public"})).unwrap(),
    );
    w.state.private.insert(
        "fresh".into(),
        Private {
            actor: "email:editor@example.test".into(),
            since: "2026-10-27T00:00:00Z".into(),
        },
    );
    for who in [
        None,
        Some("reader@example.test"),
        Some("outsider@example.test"),
    ] {
        for path in [
            "/stories/fresh",
            "/_content/fresh/report.html",
            "/r/fresh",
            &format!("/_releases/{OLD}/stories/fresh"),
        ] {
            assert_eq!(w.status(path, who), 404, "{path} {who:?}");
        }
        for path in [
            "/",
            "/archive",
            "/search?q=Fresh",
            "/feed.xml",
            "/search.json",
            "/sitemap.xml",
        ] {
            let body = w.text(path, who);
            assert!(
                !body.contains("Fresh private")
                    && !body.contains("Card of fresh")
                    && !body.contains("fresh"),
                "{path} {who:?}"
            );
        }
    }
    // A scoped editor reads it and sees it listed, but syndication stays public-only.
    assert_eq!(w.status("/stories/fresh", Some("editor@example.test")), 200);
    assert!(
        w.text("/", Some("editor@example.test"))
            .contains("Card of fresh")
    );
    for path in ["/feed.xml", "/search.json", "/sitemap.xml"] {
        assert!(
            !w.text(path, Some("editor@example.test")).contains("fresh"),
            "{path}"
        );
    }
    assert_eq!(w.status("/stories/fresh", Some("super@example.test")), 200);
    // An editor outside the section does not.
    w.state
        .roles
        .get_mut("email:editor@example.test")
        .unwrap()
        .sections = vec!["other".into()];
    assert_eq!(w.status("/stories/fresh", Some("editor@example.test")), 404);
    // Making it public lifts the restriction.
    w.state.private.clear();
    assert_eq!(w.status("/stories/fresh", None), 200);
    assert!(w.text("/feed.xml", None).contains("Fresh private story"));
}

#[test]
fn unknown_hosts_releases_and_malformed_ids_are_not_found() {
    let w = World::new();
    assert_eq!(
        w.status(
            &format!("/_releases/{}/", "c".repeat(64)),
            Some("super@example.test")
        ),
        404
    );
    assert_eq!(
        w.status("/_releases/short/", Some("super@example.test")),
        404
    );
    assert_eq!(w.status("/missing-page", None), 404);
    assert_eq!(w.status(&format!("/_releases/{OLD}"), None), 200);
}

const SLOT: &str = "<!--PUBLISH:SOCIAL-->";

fn with_slots(w: &mut World) {
    w.set_object("article", &format!("<h1>Public finding</h1>{SLOT}"));
    w.set_object("private", &format!("<h1>Secret member title</h1>{SLOT}"));
}

#[test]
fn only_a_story_an_anonymous_reader_may_read_advertises_its_preview_image() {
    let mut w = World::new();
    with_slots(&mut w);
    let page = w.text("/stories/report", None);
    assert!(!page.contains(SLOT));
    for tag in [
        "<meta property=\"og:type\" content=\"article\">",
        "<meta property=\"og:title\" content=\"Public finding\">",
        "<meta property=\"og:description\" content=\"Summary\">",
        &format!(
            "<meta property=\"og:image\" content=\"https://news.example.test/_releases/{RELEASE}/_content/report/og.png\">"
        ),
        "<meta property=\"og:image:alt\" content=\"Public finding, by Writer\">",
        "<meta name=\"twitter:card\" content=\"summary_large_image\">",
    ] {
        assert!(page.contains(tag), "{tag}");
    }
    let png = w.get("/_content/report/og.png", None);
    assert_eq!(png.status, 200);
    assert_eq!(header(&png, "Content-Type").as_deref(), Some("image/png"));
    // The advertised address is the version's own, and serves the same image.
    let versioned = w.get(
        &format!("/_releases/{RELEASE}/_content/report/og.png"),
        None,
    );
    assert_eq!((versioned.status, &versioned.body), (200, &png.body));
    // A members-only story, read by a member, carries no image tag, and nobody else reads it.
    let member = w.text("/stories/member", Some("reader@example.test"));
    assert!(!member.contains("og:image") && !member.contains(SLOT));
    assert!(member.contains("content=\"website\""));
    assert_eq!(w.status("/stories/member", None), 404);
    assert_eq!(w.status("/_content/member/og.png", None), 404);
    assert_eq!(
        w.status("/_content/draft/og.png", Some("reader@example.test")),
        404
    );
    // Previews and old releases are not link targets.
    let preview = w.text(
        &format!("/_preview/{RELEASE}/stories/report"),
        Some("editor@example.test"),
    );
    assert!(!preview.contains("og:image"));
    let old = w.text(&format!("/_releases/{OLD}/stories/report"), None);
    assert!(!old.contains("og:image"));
    // Feeds, search and the sitemap do not mention the image.
    for path in ["/feed.xml", "/search.json", "/sitemap.xml"] {
        assert!(!w.text(path, None).contains("og.png"), "{path}");
    }
}

#[test]
fn a_private_hidden_or_floored_story_never_advertises_or_serves_its_preview_image() {
    let mut w = World::new();
    with_slots(&mut w);
    // An editor's own private publish: readers get nothing, the editor gets the page without a tag.
    w.state.private.insert(
        "report".into(),
        Private {
            actor: "email:editor@example.test".into(),
            since: "2026-10-27T00:00:00Z".into(),
        },
    );
    for who in [
        None,
        Some("reader@example.test"),
        Some("outsider@example.test"),
    ] {
        assert_eq!(w.status("/stories/report", who), 404, "{who:?}");
        assert_eq!(w.status("/_content/report/og.png", who), 404, "{who:?}");
    }
    let editor = w.text("/stories/report", Some("editor@example.test"));
    assert!(editor.contains("Public finding") && !editor.contains("og:image"));
    w.state.private.clear();
    // The ledger's floor narrowed the story to members: no tag for the member who can still read it.
    w.state.policies.get_mut("report").unwrap().audience = "members".into();
    assert_eq!(w.status("/_content/report/og.png", None), 404);
    let member = w.text("/stories/report", Some("reader@example.test"));
    assert!(member.contains("Public finding") && !member.contains("og:image"));
    w.state.policies.get_mut("report").unwrap().audience = "public".into();
    assert!(w.text("/stories/report", None).contains("og:image"));
    // An urgent hide vetoes the page and the image for everyone.
    w.state.hidden.insert(
        "report".into(),
        Hide {
            revision: "r".into(),
            actor: "email:editor@example.test".into(),
            reason: "Correction drill".into(),
            at: "2026-10-27T00:00:00Z".into(),
            reconciliation: "pending".into(),
            pull: None,
        },
    );
    for who in [
        None,
        Some("reader@example.test"),
        Some("editor@example.test"),
    ] {
        assert_eq!(w.status("/stories/report", who), 404, "{who:?}");
        assert_eq!(w.status("/_content/report/og.png", who), 404, "{who:?}");
    }
}

#[test]
fn a_public_storys_image_is_cached_by_version_and_revalidated_at_the_latest_address() {
    let mut w = World::new();
    let latest = w.get("/_content/report/og.png", None);
    assert_eq!(
        header(&latest, "Cache-Control").as_deref(),
        Some("public, no-cache")
    );
    let versioned = w.get(
        &format!("/_releases/{RELEASE}/_content/report/og.png"),
        None,
    );
    assert_eq!(
        header(&versioned, "Cache-Control").as_deref(),
        Some("public, max-age=31536000, immutable")
    );
    // A signed-in reader gets the same image and the same policy: the bytes do not depend on who asks.
    let signed_in = w.get(
        &format!("/_releases/{RELEASE}/_content/report/og.png"),
        Some("reader@example.test"),
    );
    assert_eq!(
        header(&signed_in, "Cache-Control"),
        header(&versioned, "Cache-Control")
    );
    // Stories that are not public to the whole web are never given a shared-cache policy.
    for who in [Some("reader@example.test"), Some("editor@example.test")] {
        let member = w.get("/_content/member/og.png", who);
        if member.status == 200 {
            assert_eq!(
                header(&member, "Cache-Control").as_deref(),
                Some("private, no-store, max-age=0"),
                "{who:?}"
            );
        }
    }
    // Other assets of a public story keep the default.
    let cover = w.get("/_content/report/cover", None);
    assert_eq!(
        header(&cover, "Cache-Control").as_deref(),
        Some("private, no-store, max-age=0")
    );
    // Once the story stops being public the same address stops being cacheable and then stops existing.
    w.state.policies.get_mut("report").unwrap().audience = "members".into();
    let narrowed = w.get(
        &format!("/_releases/{RELEASE}/_content/report/og.png"),
        Some("reader@example.test"),
    );
    assert_ne!(
        header(&narrowed, "Cache-Control").as_deref(),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(
        w.status(
            &format!("/_releases/{RELEASE}/_content/report/og.png"),
            None
        ),
        404
    );
}

/// FEA-SF-64: an anonymous request cannot tell a private, hidden, narrowed, draft or missing story
/// from another by the image's status, headers or bytes, on the latest and the versioned address.
#[test]
fn a_private_hidden_draft_or_missing_storys_image_answers_identically_to_an_anonymous_request() {
    let mut w = World::new();
    let paths = |id: &str| {
        [
            format!("/_content/{id}/og.png"),
            format!("/_releases/{RELEASE}/_content/{id}/og.png"),
        ]
    };
    let answer = |w: &World, path: &str| {
        let r = w.get(path, None);
        (r.status, r.headers.clone(), r.body.clone())
    };
    let missing = [
        answer(&w, &paths("no-such-story")[0]),
        answer(&w, &paths("no-such-story")[1]),
    ];
    assert_eq!(missing[0].0, 404);
    // A members-only story and an unpublished draft.
    for id in ["member", "draft"] {
        for (i, path) in paths(id).iter().enumerate() {
            assert_eq!(answer(&w, path), missing[i], "{id} {path}");
        }
    }
    // A private publish (OPN-54).
    w.state.private.insert(
        "report".into(),
        Private {
            actor: "email:editor@example.test".into(),
            since: "2026-10-27T00:00:00Z".into(),
        },
    );
    for (i, path) in paths("report").iter().enumerate() {
        assert_eq!(answer(&w, path), missing[i], "private {path}");
    }
    w.state.private.clear();
    // An urgent hide.
    w.state.hidden.insert(
        "report".into(),
        Hide {
            revision: "r".into(),
            actor: "email:editor@example.test".into(),
            reason: "Correction drill".into(),
            at: "2026-10-27T00:00:00Z".into(),
            reconciliation: "pending".into(),
            pull: None,
        },
    );
    for (i, path) in paths("report").iter().enumerate() {
        assert_eq!(answer(&w, path), missing[i], "hidden {path}");
    }
    // The failure page names no story and carries no cache policy beyond the default.
    let (_, headers, body) = &missing[0];
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "Cache-Control" && v.starts_with("private, no-store"))
    );
    let text = String::from_utf8_lossy(body);
    assert!(!text.contains("member") && !text.contains("report"));
}
