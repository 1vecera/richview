//! Reader routes: pages, search, feeds, status, content frames, redirects and old releases (FEA-BR-24, -25, -44 to -46).
//!
//! Every item decision goes through `access::visible`, so the urgent-hide veto, audiences, the ledger floor and
//! OPN-54's private items hold on every route. Fonts are not here: the adapter serves them.
use crate::{
    access::{Access, Reading, visible},
    host::*,
    ledger::State,
    model::{Item, Kind, Mode, OG_WEBSITE, Release, SHARE_IMAGE, SOCIAL_SLOT, Theme, escape},
    respond,
};
use std::rc::Rc;

/// What a reader sees when storage fails: never why, and never access.
const UNAVAILABLE: &str = "The publication is temporarily unavailable. No access was granted.";
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const READING_JS: &str = r##"const frame=document.querySelector('iframe.report');addEventListener('message',e=>{if(frame&&e.source===frame.contentWindow&&e.data?.type==='richview:height'&&Number.isFinite(e.data.height)){frame.style.height=Math.max(500,Math.min(30000,e.data.height))+"px";}});const nav=document.querySelector('.sections');if(nav){const end=()=>{nav.toggleAttribute('data-start',nav.scrollLeft>2);nav.toggleAttribute('data-end',nav.scrollLeft+nav.clientWidth>=nav.scrollWidth-2);};const a=nav.querySelector('[aria-current]');if(a&&nav.scrollWidth>nav.clientWidth)nav.scrollLeft+=a.getBoundingClientRect().left-nav.getBoundingClientRect().left-(nav.clientWidth-a.offsetWidth)/2;end();nav.addEventListener('scroll',end,{passive:true});addEventListener('resize',end);}"##;
const LATIN: &str = "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD";
const LATIN_EXT: &str = "U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF";
/// Report typography mapped onto the publication: display headings, figures-strip metrics, compact tables.
const EMBED_TYPE: &str = r#".rv-report__body :is(h1,h2,h3),.rv-figure__title{font-family:"Newsreader",Georgia,serif!important;font-weight:500!important;letter-spacing:-.015em}.rv-metric__value{font:500 64px/1 "Newsreader",Georgia,serif!important;letter-spacing:-.03em;font-variant-numeric:tabular-nums lining-nums}.rv-metric__label{margin-top:8px;font:400 15px/1.45 "Geist",system-ui,sans-serif!important}.rv-metric__delta{font-family:"IBM Plex Mono",ui-monospace,monospace!important}.rv-report__body table{width:auto!important}.rv-report__body :is(td,th).num{font-family:"IBM Plex Mono",ui-monospace,monospace!important;font-size:.92em;font-variant-numeric:tabular-nums}"#;

/// The publication faces a report frame borrows for headings, metrics and figures.
fn embed_fonts() -> String {
    let mut css = String::new();
    for (family, file, weight) in [
        ("Newsreader", "newsreader", 500),
        ("Geist", "geist", 400),
        ("IBM Plex Mono", "ibm-plex-mono", 400),
    ] {
        for (subset, range) in [("latin", LATIN), ("latin-ext", LATIN_EXT)] {
            css.push_str(&format!(
                "@font-face{{font-family:\"{family}\";src:url(/_fonts/{file}-{subset}-{weight}-normal.woff2) format(\"woff2\");font-weight:{weight};font-display:swap;unicode-range:{range}}}"
            ));
        }
    }
    css
}

/// Embedded reports sit on the page itself: no panel, one text column aligned with the article header.
pub fn embed_style(theme: &Theme) -> String {
    let ink = *theme == Theme::Ink;
    format!(
        "<style>{}{EMBED_TYPE}html,body{{height:auto!important;min-height:0!important{}}}html:root{{--rv-measure:620px!important}}.rv-report__header,.rv-report__footer{{display:none}}.rv-report{{max-width:none!important;margin:0!important;padding:{}!important}}.rv-report__body>:is(.rv-chart,.rv-table,table,.rv-image,.rv-embed,.rv-keep),.rv-report__body>.rv-keep>:is(.rv-chart,.rv-image,.rv-embed){{max-width:100%}}.rv-report__body{{min-width:0}}.rv-report__body :not(pre)>code,.rv-report__body a{{overflow-wrap:anywhere}}.rv-report__body :is(.rv-table,table){{display:block;max-width:100%;overflow-x:auto}}.rv-report__body>:is(.rv-table,table){{max-width:var(--rv-measure)}}.rv-report__body .rv-table table{{display:table}}</style></head>",
        embed_fonts(),
        if ink {
            ""
        } else {
            ";background:transparent!important"
        },
        if ink { "28px 28px 32px" } else { "4px 0 8px" },
    )
}

/// How the request addresses a release: the live one, a protected preview, or an old one.
#[derive(Clone, Copy)]
struct View {
    preview: bool,
    historical: bool,
    internal: bool,
}
impl View {
    fn route(&self, path: &str, release: &Release) -> String {
        let base = if self.preview {
            format!("/_preview/{}", release.id)
        } else if self.historical {
            format!("/_releases/{}", release.id)
        } else {
            String::new()
        };
        format!(
            "{base}{}{path}",
            if self.internal { "/internal" } else { "" }
        )
    }
    /// Points root-relative links at the same view; runtimes, fonts and the identity and editor routes stay put.
    fn rewrite(&self, body: &str, release: &Release) -> String {
        if !(self.preview || self.historical || self.internal) {
            return body.to_string();
        }
        let mut out = String::with_capacity(body.len());
        let mut cursor = 0;
        for (at, _) in body.match_indices("=\"/") {
            let Some(attr) = ["href", "src", "action"]
                .into_iter()
                .find(|a| body[..at].ends_with(a))
            else {
                continue;
            };
            let start = at - attr.len();
            let path_start = at + 2;
            if start < cursor || body[path_start + 1..].starts_with('/') {
                continue;
            }
            let Some(len) = body[path_start..].find('"') else {
                continue;
            };
            let path = &body[path_start..path_start + len];
            out.push_str(&body[cursor..start]);
            // Runtimes are content-addressed and shared by every release, so both halves of a report keep one root path.
            let kept = [
                "/_editor",
                "/_auth",
                "/_status",
                "/_reading.js",
                "/_fonts/",
                "/_runtime/",
            ]
            .iter()
            .any(|p| path.starts_with(p))
                || (self.internal && path == "/internal");
            if kept {
                out.push_str(&body[start..path_start + len + 1]);
            } else {
                out.push_str(&format!("{attr}=\"{}\"", self.route(path, release)));
            }
            cursor = path_start + len + 1;
        }
        out.push_str(&body[cursor..]);
        out
    }
}

fn release_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}
fn manifest(ctx: &Ctx, id: &str) -> R<Release> {
    if !release_id(id) {
        return Err(error(404, "Release not found"));
    }
    let bytes = match ctx.blob(&format!("releases/{id}.json")) {
        Err(Stop::Http(_)) => return Err(error(404, "Release not found")),
        other => other?,
    };
    serde_json::from_slice(&bytes).map_err(|_| error(503, UNAVAILABLE))
}
/// The bytes of a stored release object; a registered object that is missing is an outage, not a 404.
fn object(ctx: &Ctx, hash: &str) -> R<Rc<Vec<u8>>> {
    if hash.is_empty() || !hash.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return Err(error(503, UNAVAILABLE));
    }
    match ctx.blob(&format!("objects/{hash}")) {
        Err(Stop::Http(_)) => Err(error(503, UNAVAILABLE)),
        other => other,
    }
}
fn object_text(ctx: &Ctx, hash: &str) -> R<String> {
    String::from_utf8(object(ctx, hash)?.to_vec()).map_err(|_| error(503, UNAVAILABLE))
}

/// Answers a GET or HEAD for a reader.
pub fn read(ctx: &mut Ctx, req: &Request, access: &Access) -> R<Response> {
    let ctx = &*ctx;
    let state = ctx.state;
    let mut path = req.path.clone();
    if path == "/_reading.js" {
        return Ok(respond::text(
            "text/javascript; charset=utf-8",
            READING_JS.into(),
        ));
    }
    if path == "/_status" {
        let commit = match &state.active {
            Some(id) => Some(manifest(ctx, id)?.commit),
            None => None,
        };
        return Ok(respond::json(
            &serde_json::json!({
                "state": if state.active.is_some() { "live" } else { "unpublished" },
                "release": state.active,
                "generation": state.generation,
                "paused": state.paused,
                "commit": commit,
            }),
            200,
        ));
    }
    let Some(active) = &state.active else {
        return Err(error(503, "The first publication is being prepared."));
    };
    let mut view = View {
        preview: false,
        historical: false,
        internal: false,
    };
    let mut release = manifest(ctx, active)?;
    if path.starts_with("/_preview/") || path.starts_with("/_releases/") {
        let parts: Vec<&str> = path.split('/').collect();
        view.preview = parts[1] == "_preview";
        view.historical = !view.preview;
        release = manifest(ctx, parts[2])?;
        path = format!("/{}", parts[3..].join("/"));
        if view.preview && !access.is_editor() {
            return Err(error(404, "Preview unavailable"));
        }
        if view.historical && !state.activated.contains_key(&release.id) {
            return Err(error(404, "Release was never published"));
        }
    }
    let member = ctx.principal.is_some() && !access.audiences.is_empty();
    if path == "/internal" || path.starts_with("/internal/") {
        if !member {
            return Err(error(
                401,
                "This edition requires authenticated membership.",
            ));
        }
        view.internal = true;
        path = match &path["/internal".len()..] {
            "" => "/".into(),
            rest => rest.to_string(),
        };
    }
    if release.site.mode == Mode::Internal && !member {
        return Err(error(
            401,
            "This publication requires authenticated membership.",
        ));
    }
    if path.starts_with("/_runtime/") {
        return runtime(ctx, &release, &path);
    }
    let how = Reading {
        preview: view.preview,
        internal: view.internal,
    };
    let eligible = |item: &Item| visible(access, item, &release, state, how);
    if path.starts_with("/_content/") {
        return content(ctx, req, &release, &path, view, &eligible);
    }
    let mut items: Vec<&Item> = release.items.iter().filter(|i| eligible(i)).collect();
    if ["/feed.xml", "/sitemap.xml", "/search.json"].contains(&path.as_str()) {
        // Syndication is always public-only, even for an authenticated request, and never lists a private item.
        items.retain(|i| {
            !view.internal
                && !view.preview
                && release.site.mode == Mode::External
                && i.meta.audience == "public"
                && state
                    .policies
                    .get(&i.meta.id)
                    .is_some_and(|p| p.audience == "public")
                && i.meta.kind != Kind::Page
                && !state.private.contains_key(&i.meta.id)
        });
        return Ok(syndication(&release, &path, &items));
    }
    if path == "/robots.txt" {
        let tail = if release.site.mode == Mode::Internal {
            "Disallow: /\n".to_string()
        } else {
            format!("Sitemap: {}/sitemap.xml\n", release.site.base_url)
        };
        return Ok(respond::text(
            "text/plain; charset=utf-8",
            format!(
                "User-agent: *\nAllow: /_content/*/og.png\nAllow: /_releases/*/_content/*/og.png\nDisallow: /_\nDisallow: /internal\n{tail}"
            ),
        ));
    }
    let mut body;
    if path == "/" || path == "/archive" || path == "/search" || path.starts_with("/sections/") {
        body = listing(ctx, req, &release, &path, items)?;
    } else {
        let item = release
            .items
            .iter()
            .find(|i| {
                i.route == path
                    || format!("/r/{}", i.meta.id) == path
                    || i.meta.aliases.iter().any(|a| {
                        path == match i.meta.kind {
                            Kind::Page => format!("/{a}"),
                            Kind::Report => format!("/stories/{a}"),
                            Kind::Dashboard => format!("/dashboards/{a}"),
                        }
                    })
            })
            .ok_or_else(|| error(404, "Item unavailable"))?;
        // A dashboard page in a release stored before FEA-BR-30 is gone for everyone, whatever the reader holds.
        if item.meta.kind == Kind::Dashboard {
            return Err(error(410, "This item is unavailable"));
        }
        let item = Some(item)
            .filter(|i| eligible(i))
            .ok_or_else(|| error(404, "Item unavailable"))?;
        if path != item.route {
            return Ok(respond::redirect(308, &view.route(&item.route, &release)));
        }
        body = object_text(ctx, &item.article)?;
        // Related stories are chosen per reader, so restricted titles never leak.
        let related: Vec<&Item> = release
            .items
            .iter()
            .filter(|i| {
                i.meta.id != item.meta.id
                    && i.meta.section == item.meta.section
                    && i.meta.kind != Kind::Page
                    && eligible(i)
            })
            .take(3)
            .collect();
        let title = release
            .site
            .sections
            .iter()
            .find(|s| s.id == item.meta.section)
            .map_or(item.meta.section.as_str(), |s| s.title.as_str());
        let more = if related.is_empty() {
            String::new()
        } else {
            format!(
                "<section class=\"more\" aria-labelledby=\"more\"><h2 class=\"list-title\" id=\"more\">More in {}</h2><div class=\"rows\">{}</div></section>",
                escape(title),
                related.iter().map(|i| i.card.as_str()).collect::<String>()
            )
        };
        body = body.replacen("<!--PUBLISH:MORE-->", &more, 1);
        // Link-preview tags are for anonymous crawlers: they are emitted only when an anonymous reader
        // may read this story right now, in this release and in the ledger's floor. Nothing is stored.
        let previewable = !view.preview
            && !view.historical
            && !view.internal
            && public_story(item, &release, state)
            && item
                .assets
                .get(SHARE_IMAGE)
                .and_then(|h| release.objects.get(h))
                .is_some_and(|o| o.content_type == "image/png");
        let social = if previewable {
            let who = if item.meta.byline.is_empty() {
                item.title.clone()
            } else {
                format!("{}, by {}", item.title, item.meta.byline)
            };
            format!(
                "<meta property=\"og:type\" content=\"article\"><meta property=\"og:title\" content=\"{title}\"><meta property=\"og:description\" content=\"{summary}\"><meta property=\"og:image\" content=\"{base}/_releases/{release}/_content/{id}/{SHARE_IMAGE}\"><meta property=\"og:image:type\" content=\"image/png\"><meta property=\"og:image:width\" content=\"1200\"><meta property=\"og:image:height\" content=\"630\"><meta property=\"og:image:alt\" content=\"{alt}\"><meta name=\"twitter:card\" content=\"summary_large_image\">",
                title = escape(&item.title),
                summary = escape(&item.meta.summary),
                base = escape(release.site.base_url.trim_end_matches('/')),
                release = escape(&release.id),
                id = escape(&item.meta.id),
                alt = escape(&who),
            )
        } else {
            OG_WEBSITE.to_owned()
        };
        body = body.replacen(SOCIAL_SLOT, &social, 1);
    }
    body = view.rewrite(
        &body.replace(
            "<!--PUBLISH:RELEASE-->",
            release.id.get(..12).unwrap_or(&release.id),
        ),
        &release,
    );
    if view.preview || view.internal {
        let notice = if view.preview {
            "Protected preview · not published"
        } else {
            "Member edition · authenticated access"
        };
        body = body.replacen(
            "<main ",
            &format!("<p class=\"notice\">{notice}</p><main "),
            1,
        );
    }
    let quiet = ctx.principal.is_some()
        || view.preview
        || view.internal
        || release.site.mode == Mode::Internal;
    let extra: &[(&str, String)] = if quiet {
        &[("X-Robots-Tag", "noindex".into())]
    } else {
        &[]
    };
    Ok(respond::html(body, 200, extra))
}

/// `/_runtime/<id>/<file>`: the release's own runtime, or an older one the ledger kept.
fn runtime(ctx: &Ctx, release: &Release, path: &str) -> R<Response> {
    let parts: Vec<&str> = path.split('/').collect();
    let missing = || error(404, "Runtime not found");
    if parts.len() != 4 {
        return Err(missing());
    }
    let (id, name) = (parts[2], parts[3]);
    let current = sha256(&serde_json::to_vec(&release.runtime).unwrap_or_default());
    let map = if current == id {
        &release.runtime
    } else {
        ctx.state.runtimes.get(id).ok_or_else(missing)?
    };
    // Releases PR #70's Worker served were built when the files were still called richview_web.*; their kept runtimes stay readable.
    if ![
        "richview_runtime.js",
        "richview_runtime_bg.wasm",
        "richview_web.js",
        "richview_web_bg.wasm",
    ]
    .contains(&name)
    {
        return Err(missing());
    }
    let hash = map.get(name).ok_or_else(missing)?;
    let kind = if name.ends_with(".wasm") {
        "application/wasm"
    } else {
        "text/javascript; charset=utf-8"
    };
    Ok(respond::with(
        200,
        kind,
        object(ctx, hash)?.to_vec(),
        &[("Access-Control-Allow-Origin", "*".into())],
    ))
}

/// Whether a report's own HTML has an element whose `src` is a credited Unsplash photo.
fn hotlinks_unsplash(html: &str) -> bool {
    const ORIGIN: &str = "https://images.unsplash.com/";
    html.match_indices("src=").any(|(at, _)| {
        html[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
            && html[at + 4..]
                .strip_prefix(['"', '\''])
                .is_some_and(|rest| rest.starts_with(ORIGIN))
    })
}

/// Whether anyone on the web may read this story right now, in this release and in the ledger. The
/// link-preview tags and the shared-cache headers rest on it: only such a story is told to crawlers
/// and caches, so a private, hidden, retired or member-only one never is.
fn public_story(item: &Item, release: &Release, state: &State) -> bool {
    release.site.mode == Mode::External
        && item.meta.kind == Kind::Report
        && item.meta.audience == "public"
        && visible(
            &Access::anonymous(),
            item,
            release,
            state,
            Reading::default(),
        )
        && state
            .policies
            .get(&item.meta.id)
            .is_none_or(|p| p.audience == "public")
}

/// `/_content/<id>/<asset>`: a report frame or a stored asset, sandboxed.
fn content(
    ctx: &Ctx,
    req: &Request,
    release: &Release,
    path: &str,
    view: View,
    eligible: &dyn Fn(&Item) -> bool,
) -> R<Response> {
    let parts: Vec<&str> = path.split('/').collect();
    let unavailable = || error(404, "Item unavailable");
    let item = release
        .items
        .iter()
        .find(|i| i.meta.id == parts[2])
        .filter(|i| eligible(i))
        .ok_or_else(unavailable)?;
    let name = parts[3..].join("/");
    let hash = if name == "report.html" {
        Some(&item.content)
    } else {
        item.assets.get(&name)
    };
    let (hash, meta) = hash
        .and_then(|h| release.objects.get(h).map(|m| (h, m)))
        .ok_or_else(|| error(404, "Asset unavailable"))?;
    let bytes = object(ctx, hash)?;
    let kind = meta.content_type.as_str();
    let report = kind.starts_with("text/html");
    let (body, csp) = if report {
        let mut html = view.rewrite(&object_text(ctx, hash)?, release);
        // A report may load images from the one hotlink origin only when its own HTML uses one.
        let photos = if hotlinks_unsplash(&html) {
            " https://images.unsplash.com"
        } else {
            ""
        };
        if req.query.get("embed").is_some_and(|v| v == "1") {
            html = html.replacen("</head>", &embed_style(&release.site.theme), 1);
        }
        html = html.replacen(
            "</body>",
            "<script>new ResizeObserver(()=>parent.postMessage({type:\"richview:height\",height:Math.ceil(document.body.getBoundingClientRect().height)},\"*\")).observe(document.body);</script></body>",
            1,
        );
        (
            html.into_bytes(),
            format!(
                "sandbox allow-scripts allow-downloads allow-popups allow-popups-to-escape-sandbox; default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' 'self'; connect-src 'self'; style-src 'unsafe-inline'; img-src 'self' data:{photos}; font-src data: 'self'; media-src 'self' data:; base-uri 'none'; form-action 'none'"
            ),
        )
    } else {
        (
            bytes.to_vec(),
            "sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'".into(),
        )
    };
    let mut extra = vec![
        ("Content-Security-Policy", csp),
        ("X-Robots-Tag", "noindex".into()),
    ];
    if kind == "application/octet-stream" {
        extra.push(("Content-Disposition", "attachment".into()));
    }
    // A story's link-preview image is cacheable by anyone only while anyone may read the story (PRF-37):
    // a version's address never changes its bytes, the latest address is revalidated, and a story
    // that is not public stays `private, no-store` like every other answer.
    if name == SHARE_IMAGE
        && !view.preview
        && !view.internal
        && public_story(item, release, ctx.state)
    {
        let policy = if view.historical {
            "public, max-age=31536000, immutable"
        } else {
            "public, no-cache"
        };
        extra.push(("Cache-Control", policy.into()));
    }
    Ok(respond::with(200, kind, body, &extra))
}

fn syndication(release: &Release, path: &str, items: &[&Item]) -> Response {
    let base = &release.site.base_url;
    if path == "/search.json" {
        let rows: Vec<_> = items
            .iter()
            .map(|i| {
                serde_json::json!({
                    "id": i.meta.id, "title": i.title, "summary": i.meta.summary, "route": i.route,
                    "tags": i.meta.tags, "updated": i.meta.updated,
                })
            })
            .collect();
        return respond::json(&rows, 200);
    }
    let xml = if path == "/feed.xml" {
        let entries: String = items
            .iter()
            .take(50)
            .map(|i| {
                format!(
                    "<item><guid isPermaLink=\"false\">{}#{}</guid><title>{}</title><link>{}</link><description>{}</description><pubDate>{}</pubDate></item>",
                    escape(base),
                    i.meta.id,
                    escape(&i.title),
                    escape(&format!("{base}{}", i.route)),
                    escape(&i.meta.summary),
                    utc_string(&i.meta.published)
                )
            })
            .collect();
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><rss version=\"2.0\"><channel><title>{}</title><link>{}</link><description>{}</description>{entries}</channel></rss>",
            escape(&release.site.title),
            escape(base),
            escape(&release.site.description)
        )
    } else {
        let urls: String = items
            .iter()
            .map(|i| {
                format!(
                    "<url><loc>{}</loc><lastmod>{}</lastmod></url>",
                    escape(&format!("{base}{}", i.route)),
                    escape(&i.meta.updated)
                )
            })
            .collect();
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">{urls}</urlset>"
        )
    };
    respond::text("application/xml; charset=utf-8", xml)
}

/// Home, archive, search and section pages filled into the release's templates.
fn listing(
    ctx: &Ctx,
    req: &Request,
    release: &Release,
    path: &str,
    mut items: Vec<&Item>,
) -> R<String> {
    let mut key = if path == "/" {
        "home".to_string()
    } else {
        path[1..].to_string()
    };
    if let Some(section) = path.strip_prefix("/sections/") {
        key = format!("section:{section}");
        items.retain(|i| i.meta.section == section);
    }
    let query: String = req
        .query
        .get("q")
        .map(|q| q.chars().take(200).collect())
        .unwrap_or_default();
    if path == "/search" {
        let needle = query.to_lowercase();
        items.retain(|i| {
            format!(
                "{} {} {} {}",
                i.title,
                i.meta.summary,
                i.meta.byline,
                i.meta.tags.join(" ")
            )
            .to_lowercase()
            .contains(&needle)
        });
    }
    items.retain(|i| i.meta.kind != Kind::Page);
    // One lead only: the first selected story this reader may see.
    let leads: Vec<&Item> = if key == "home" {
        release
            .site
            .lead
            .iter()
            .filter_map(|id| items.iter().find(|i| &i.meta.id == id).copied())
            .take(1)
            .collect()
    } else {
        vec![]
    };
    let remaining: Vec<&Item> = items
        .iter()
        .filter(|i| !leads.iter().any(|l| l.meta.id == i.meta.id))
        .copied()
        .collect();
    let page = req
        .query
        .get("page")
        .map(|p| p.bytes().take_while(u8::is_ascii_digit).collect::<Vec<_>>())
        .and_then(|d| String::from_utf8(d).ok()?.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, 1000);
    let size = if key == "home" { 9 } else { 18 };
    let template = release
        .templates
        .get(&key)
        .ok_or_else(|| error(404, "Section unavailable"))?;
    let mut body = object_text(ctx, template)?;
    let figures: Vec<&str> = items
        .iter()
        .filter(|i| !i.stat.is_empty())
        .take(4)
        .map(|i| i.stat.as_str())
        .collect();
    let latest = items.iter().map(|i| i.meta.updated.as_str()).max();
    let dateline = match latest {
        Some(latest) => format!(
            "Updated <time datetime=\"{}\">{}</time> · {}",
            escape(latest),
            human_date(latest),
            plural(items.len(), "story", "stories")
        ),
        None => "Nothing published yet".into(),
    };
    let mut rows = String::new();
    let mut month = String::new();
    for i in remaining.iter().skip((page - 1) * size).take(size) {
        if key == "archive" && month_name(&i.meta.published) != month {
            month = month_name(&i.meta.published);
            rows.push_str(&format!("<h2 class=\"month\">{month}</h2>"));
        }
        rows.push_str(&i.card);
    }
    let empty = if path == "/search" && !query.is_empty() {
        format!(
            "Nothing matches “{}”. Try a shorter word, an author or a tag.",
            escape(&query)
        )
    } else if key.starts_with("section:") {
        "Nothing has been published in this section yet.".into()
    } else if key == "home" && !leads.is_empty() {
        "The lead story is everything published so far.".into()
    } else {
        "Nothing has been published yet.".into()
    };
    let q = encode_uri_component(&query);
    let pagination = format!(
        "<div class=\"pagination\">{}{}</div>",
        if page > 1 {
            format!("<a href=\"?q={q}&amp;page={}\">Newer stories</a>", page - 1)
        } else {
            String::new()
        },
        if remaining.len() > page * size {
            format!("<a href=\"?q={q}&amp;page={}\">Older stories</a>", page + 1)
        } else {
            String::new()
        }
    );
    let figures = if !figures.is_empty() && page == 1 {
        format!(
            "<section class=\"figures\" aria-label=\"Figures from recent stories\">{}</section>",
            figures.concat()
        )
    } else {
        String::new()
    };
    let lead = if page == 1 {
        leads.iter().map(|i| i.lead.as_str()).collect()
    } else {
        String::new()
    };
    let rows = if rows.is_empty() {
        format!("<p class=\"empty\">{empty}</p>")
    } else {
        rows
    };
    for (marker, value) in [
        ("<!--PUBLISH:DATELINE-->", dateline),
        ("<!--PUBLISH:FIGURES-->", figures),
        ("<!--PUBLISH:LEAD-->", lead),
        ("<!--PUBLISH:ITEMS-->", rows),
        ("<!--PUBLISH:PAGINATION-->", pagination),
        ("<!--PUBLISH:QUERY-->", escape(&query)),
    ] {
        body = body.replacen(marker, &value, 1);
    }
    Ok(body)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
fn number(part: Option<&str>) -> Option<usize> {
    part?.parse().ok()
}
/// `18 Oct 2026` from the date part of an RFC 3339 timestamp, matching the builder.
fn human_date(value: &str) -> String {
    let mut parts = value.get(..10).unwrap_or(value).split('-');
    let (y, m, d) = (parts.next(), number(parts.next()), number(parts.next()));
    match (y, m, d) {
        (Some(y), Some(m @ 1..=12), Some(d)) => format!("{d} {} {y}", MONTHS[m - 1]),
        _ => value.into(),
    }
}
fn month_name(value: &str) -> String {
    let mut parts = value.get(..7).unwrap_or(value).split('-');
    match (parts.next(), number(parts.next())) {
        (Some(y), Some(m @ 1..=12)) => format!("{} {y}", MONTH_NAMES[m - 1]),
        _ => value.into(),
    }
}
fn encode_uri_component(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
/// An RFC 3339 timestamp as an RSS date (`Sun, 18 Oct 2026 09:00:00 GMT`); anything unparsable is passed through.
fn utc_string(value: &str) -> String {
    let field = |from: usize, to: usize| value.get(from..to)?.parse::<i64>().ok();
    let parsed = (|| {
        let (y, m, d) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
        let (h, mi, s) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
        if !(1..=12).contains(&m) {
            return None;
        }
        let rest = value.get(19..)?;
        let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
        let offset = match rest.as_bytes().first()? {
            b'Z' | b'z' => 0,
            sign @ (b'+' | b'-') => {
                let minutes = rest.get(1..3)?.parse::<i64>().ok()? * 60
                    + rest.get(4..6)?.parse::<i64>().ok()?;
                if *sign == b'+' { minutes } else { -minutes }
            }
            _ => return None,
        };
        // Days since 1970-01-01 from the civil date (Howard Hinnant's algorithm).
        let (yy, mm) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
        let era = yy.div_euclid(400);
        let yoe = yy - era * 400;
        let doy = (153 * mm + 2) / 5 + d - 1;
        let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
        Some(days * 86400 + h * 3600 + mi * 60 + s - offset * 60)
    })();
    let Some(secs) = parsed else {
        return value.into();
    };
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize];
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{weekday}, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        MONTHS[month as usize - 1],
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
