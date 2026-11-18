//! HTML bodies for hosted Cowork. Routes and authorization remain in the server handlers.

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Who is looking and which organization the page works in.
pub(super) struct Nav<'a> {
    pub name: &'a str,
    pub email: &'a str,
    pub csrf: &'a str,
    /// The current organization: its code and its name.
    pub org: Option<(&'a str, &'a str)>,
    /// The person's role in the current organization, as a word: Owner, Admin, Author or Reader.
    pub role: Option<&'a str>,
    /// Every organization the person belongs to: code and name.
    pub orgs: &'a [(String, String)],
}

/// The app's one stroke set on a 16 px grid, drawn for RichView: navigation, actions and who can
/// open a report. Pages that carry the app's chrome include it once; [`icon`] draws from it.
pub(super) const ICONS: &str = concat!(
    "<svg class=\"sprite\" width=\"0\" height=\"0\" aria-hidden=\"true\">",
    "<symbol id=\"i-report\" viewBox=\"0 0 16 16\"><rect x=\"2.75\" y=\"1.75\" width=\"10.5\" height=\"12.5\" rx=\"1.75\"/><path d=\"M5.5 5.5h5M5.5 8h3.5M5.5 10.5h2\"/></symbol>",
    "<symbol id=\"i-users\" viewBox=\"0 0 16 16\"><circle cx=\"6\" cy=\"5.5\" r=\"2.5\"/><path d=\"M1.75 13.75c.35-2.4 2.1-3.75 4.25-3.75s3.9 1.35 4.25 3.75M10.5 3.25a2.25 2.25 0 0 1 0 4.5M12 10.25c1.2.45 2 1.55 2.25 3.5\"/></symbol>",
    "<symbol id=\"i-key\" viewBox=\"0 0 16 16\"><circle cx=\"5\" cy=\"11\" r=\"2.75\"/><path d=\"M7 9l6.25-6.25M11.25 4.75l1.75 1.75M9.5 6.5l1.5 1.5\"/></symbol>",
    "<symbol id=\"i-user\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"5.25\" r=\"2.75\"/><path d=\"M2.75 14c.55-2.75 2.65-4.25 5.25-4.25s4.7 1.5 5.25 4.25\"/></symbol>",
    "<symbol id=\"i-book\" viewBox=\"0 0 16 16\"><path d=\"M2.25 3h3.75A2 2 0 0 1 8 5v9a1.5 1.5 0 0 0-1.5-1.5H2.25zM13.75 3H10a2 2 0 0 0-2 2v9a1.5 1.5 0 0 1 1.5-1.5h4.25z\"/></symbol>",
    "<symbol id=\"i-search\" viewBox=\"0 0 16 16\"><circle cx=\"7\" cy=\"7\" r=\"4.5\"/><path d=\"M10.4 10.4L14 14\"/></symbol>",
    "<symbol id=\"i-updown\" viewBox=\"0 0 16 16\"><path d=\"M5 6l3-3 3 3M5 10l3 3 3-3\"/></symbol>",
    "<symbol id=\"i-share\" viewBox=\"0 0 16 16\"><circle cx=\"6\" cy=\"5.5\" r=\"2.5\"/><path d=\"M1.75 13.75c.35-2.4 2.1-3.75 4.25-3.75s3.9 1.35 4.25 3.75M12.5 4.5v4M10.5 6.5h4\"/></symbol>",
    "<symbol id=\"i-lock\" viewBox=\"0 0 16 16\"><rect x=\"3\" y=\"7\" width=\"10\" height=\"7\" rx=\"1.5\"/><path d=\"M5.25 7V5a2.75 2.75 0 0 1 5.5 0v2\"/></symbol>",
    "<symbol id=\"i-globe\" viewBox=\"0 0 16 16\"><circle cx=\"8\" cy=\"8\" r=\"6\"/><path d=\"M2 8h12M8 2c1.6 1.7 2.4 3.7 2.4 6S9.6 12.3 8 14c-1.6-1.7-2.4-3.7-2.4-6S6.4 3.7 8 2z\"/></symbol>",
    "<symbol id=\"i-org\" viewBox=\"0 0 16 16\"><path d=\"M2.75 14V3.25A1.25 1.25 0 0 1 4 2h5a1.25 1.25 0 0 1 1.25 1.25V14M10.25 6.5H12a1.25 1.25 0 0 1 1.25 1.25V14M1.5 14h13M5 5h2.25M5 8h2.25M5 11h2.25\"/></symbol>",
    "<symbol id=\"i-inbox\" viewBox=\"0 0 16 16\"><path d=\"M2 9.5h3l1 1.75h4l1-1.75h3M3.6 3.5h8.8L14 9.5v3a1.25 1.25 0 0 1-1.25 1.25H3.25A1.25 1.25 0 0 1 2 12.5v-3z\"/></symbol>",
    "<symbol id=\"i-out\" viewBox=\"0 0 16 16\"><path d=\"M6.25 2.5H3.5a1.25 1.25 0 0 0-1.25 1.25v8.5A1.25 1.25 0 0 0 3.5 13.5h2.75M10.25 5l3 3-3 3M13.25 8h-7\"/></symbol>",
    "<symbol id=\"i-down\" viewBox=\"0 0 16 16\"><path d=\"M8 3v10M4.75 9.75L8 13l3.25-3.25\"/></symbol>",
    "<symbol id=\"i-more\" viewBox=\"0 0 16 16\"><circle cx=\"3.5\" cy=\"8\" r=\".75\"/><circle cx=\"8\" cy=\"8\" r=\".75\"/><circle cx=\"12.5\" cy=\"8\" r=\".75\"/></symbol>",
    "<symbol id=\"i-chev\" viewBox=\"0 0 16 16\"><path d=\"M4.5 6.25L8 9.75l3.5-3.5\"/></symbol>",
    "<symbol id=\"i-check\" viewBox=\"0 0 16 16\"><path d=\"M3.25 8.5l3 3 6.5-7\"/></symbol>",
    "<symbol id=\"i-mail\" viewBox=\"0 0 16 16\"><rect x=\"2\" y=\"3.5\" width=\"12\" height=\"9\" rx=\"1.5\"/><path d=\"M2.5 4.75L8 9l5.5-4.25\"/></symbol>",
    "</svg>"
);

/// One icon from [`ICONS`], hidden from assistive technology: the words beside it carry the meaning.
fn icon(id: &str) -> String {
    format!("<svg class=\"ic\" aria-hidden=\"true\"><use href=\"#i-{id}\"/></svg>")
}

/// The mark, in the colour of the text around it (white on the ink rail, STY-03).
const MARK: &str = "<svg class=\"app-mark\" viewBox=\"0 0 268.58 269.73\" aria-hidden=\"true\"><g fill=\"currentColor\"><path d=\"M221.4,51.39 H0 V0 h221.4 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><path d=\"M255.32,124.17 H0 V72.78 h255.32 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><path d=\"M221.4,196.95 H0 v-51.39 h221.4 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><path d=\"M93.12,269.73 H0 v-51.39 h93.12 c7.32,0 13.25,5.93 13.25,13.25 v24.89 c0,7.32 -5.93,13.25 -13.25,13.25 z\"/><circle cx=\"242.88\" cy=\"244.04\" r=\"25.7\"/></g></svg>";

/// The first letters of a name's first and last words, as an avatar shows them.
pub(in crate::server) fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = words.first().and_then(|word| word.chars().next());
    let last = (words.len() > 1)
        .then(|| words.last().and_then(|word| word.chars().next()))
        .flatten();
    first
        .into_iter()
        .chain(last)
        .flat_map(char::to_uppercase)
        .collect()
}

/// The app's chrome on a signed-in page: the rail (the organization, the pages, the person) and the
/// top bar (where the page is, and search). The page renderer lifts both out of the page's body.
fn navigation(nav: &Nav<'_>, current: &str) -> String {
    app_chrome(nav, current, "all", "", None)
}

/// The chrome of a page about one report: its crumb ends with the report's title, after a link
/// back to the reports.
fn report_navigation(nav: &Nav<'_>, title: &str) -> String {
    app_chrome(nav, "reports", "all", "", Some(title))
}

fn app_chrome(
    nav: &Nav<'_>,
    current: &str,
    show: &str,
    query: &str,
    report: Option<&str>,
) -> String {
    let link = |href: &str, label: &str, id: &str, glyph: &str| {
        format!(
            "<a href=\"{href}\"{}>{}<span>{label}</span></a>",
            if id == current {
                " aria-current=\"page\""
            } else {
                ""
            },
            icon(glyph)
        )
    };
    let csrf = escape(nav.csrf);
    let head = |name: &str, role: Option<&str>| {
        format!(
            "<span class=\"app-org__tile\" aria-hidden=\"true\">{}</span><span class=\"app-org__name\"><strong>{}</strong>{}</span>",
            escape(
                &name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>())
                    .unwrap_or_default()
            ),
            escape(name),
            role.map_or_else(String::new, |role| format!("<span>{}</span>", escape(role)))
        )
    };
    let organization = match (nav.org, nav.orgs.len()) {
        (Some((current, name)), count) if count > 1 => {
            let items: String = nav
                .orgs
                .iter()
                .map(|(id, org)| {
                    format!(
                        "<button class=\"app-org__item\" type=\"submit\" name=\"org\" value=\"{}\"{}><span class=\"app-org__tile\" aria-hidden=\"true\">{}</span>{}</button>",
                        escape(id),
                        if id == current { " aria-current=\"true\"" } else { "" },
                        escape(&org.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default()),
                        escape(org)
                    )
                })
                .collect();
            format!(
                "<details class=\"app-org app-org--switch\"><summary>{}<span class=\"vh\">, switch organization</span>{}</summary><form class=\"app-org__menu\" method=\"post\" action=\"/ui/organization\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><p>Switch organization</p>{items}</form></details>",
                head(name, nav.role),
                icon("updown")
            )
        }
        // An organization of one carries its owner's name: say it once.
        (Some((_, name)), _) if name == nav.name => String::new(),
        (Some((_, name)), _) => format!("<div class=\"app-org\">{}</div>", head(name, nav.role)),
        (None, _) => String::new(),
    };
    // Members and CLI tokens belong to an organization; someone with only shared reports has none.
    let in_organization = nav.org.is_some() || !nav.orgs.is_empty();
    let page = match current {
        "members" => "Members",
        "tokens" => "CLI tokens",
        "account" => "Account",
        _ => "Reports",
    };
    let crumbs = match nav.org {
        Some((_, name)) => format!(
            "<span>{}</span><span aria-hidden=\"true\">/</span>",
            escape(name)
        ),
        None => String::new(),
    };
    // A long title gives way at the end of the line; its full text stays in the title attribute.
    let place = match report {
        Some(title) => format!(
            "<a href=\"/\">{page}</a><span aria-hidden=\"true\">/</span><strong title=\"{0}\">{0}</strong>",
            escape(title)
        ),
        None => format!("<strong>{page}</strong>"),
    };
    let scope = if show == "all" {
        String::new()
    } else {
        format!(
            "<input type=\"hidden\" name=\"show\" value=\"{}\">",
            escape(show)
        )
    };
    format!(
        "<aside class=\"app-rail\"><div class=\"app-rail__in\"><a class=\"app-brand\" href=\"/\">{MARK}<span>RichView</span></a>{organization}<nav class=\"app-nav\" aria-label=\"Account navigation\">{}{}{}{}<a class=\"app-nav__docs\" href=\"https://richview.ai/docs/\">{}<span>Docs</span></a></nav><div class=\"app-me\"><span class=\"app-me__avatar\" aria-hidden=\"true\">{}</span><span class=\"app-me__who\"><strong>{}</strong><span>{}</span></span><form method=\"post\" action=\"/ui/sign-out\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"app-me__out\" type=\"submit\">{}<span>Sign out</span></button></form></div></div></aside><header class=\"app-top\"><p class=\"app-crumbs\">{crumbs}{place}</p><form class=\"app-search\" method=\"get\" action=\"/\" role=\"search\">{scope}{}<input type=\"search\" name=\"q\" value=\"{}\" placeholder=\"Search reports\" aria-label=\"Search reports by title\"></form></header>",
        link("/", "Reports", "reports", "report"),
        if in_organization {
            link("/members", "Members", "members", "users")
        } else {
            String::new()
        },
        if in_organization {
            link("/tokens", "CLI tokens", "tokens", "key")
        } else {
            String::new()
        },
        link("/account", "Account", "account", "user"),
        icon("book"),
        escape(&initials(nav.name)),
        escape(nav.name),
        escape(nav.email),
        icon("out"),
        icon("search"),
        escape(query)
    )
}

/// Whether an organization carries a person's own name, as one made at sign-up does until it is
/// renamed: then a page or mail names the person once.
pub(super) fn same_name(person: &str, organization: &str) -> bool {
    person.trim().to_lowercase() == organization.trim().to_lowercase()
}

/// ` · {organization}` after a person's name, or nothing when it carries their name.
fn and_organization(person: &str, organization: &str) -> String {
    if same_name(person, organization) {
        String::new()
    } else {
        format!(" · {}", escape(organization))
    }
}

fn note(notice: &str) -> String {
    if notice.is_empty() {
        return String::new();
    }
    let (kind, role) = if notice == "Sign in to open this report."
        || notice == "Sign in first."
        || notice.starts_with("Accepted. Sign in to ")
    {
        ("info", "status")
    } else if !notice.contains("could not be sent")
        && ([
            "Saved.",
            "Role saved.",
            "Name saved.",
            "Removed.",
            "Person removed.",
            "Member removed.",
            "Invitation cancelled.",
            "Token revoked.",
            "Report deleted.",
            "You left the organization.",
            "Password set; the person was signed out.",
            "This report was shared with you.",
            "Anyone with the link can open the report.",
        ]
        .contains(&notice)
            || [
                "Saved ",
                "Invited ",
                "Shared with ",
                "Added ",
                "You joined ",
                "Deleted “",
                "The report is private: ",
                "Every member of ",
            ]
            .iter()
            .any(|prefix| notice.starts_with(prefix))
            || notice.ends_with(" is ready."))
    {
        ("ok", "status")
    } else {
        ("error", "alert")
    };
    format!(
        r#"<p class="note note--{kind}" role="{role}">{}</p>"#,
        escape(notice)
    )
}

fn shell(command: &str) -> String {
    let code = command
        .lines()
        .map(|line| {
            if line.starts_with("export ")
                && let Some((name, value)) = line.split_once('=')
            {
                format!(
                    "{}=<wbr><span class=\"shell-value\">{}</span>",
                    escape(name),
                    escape(value)
                )
            } else {
                escape(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<div class=\"rv-report__body\"><figure class=\"docs-code\"><figcaption><span>Shell</span><button class=\"cowork-copy\" type=\"button\" data-copy-code aria-label=\"Copy shell command\">Copy</button></figcaption><pre tabindex=\"0\" aria-label=\"Shell command\"><code class=\"language-sh\">{code}</code></pre></figure></div>"
    )
}

/// The first step takes no password; the mail link proves the address first.
pub(super) fn sign_up() -> String {
    r#"<div class="cowork-auth"><h1>Create account</h1><p class="lede">Publish the reports your agent writes. They’re private by default, with 50 MiB and 50 reports free.</p><form class="panel" method="post" action="/ui/sign-up"><div class="field"><label for="email">Email</label><input id="email" type="email" name="email" required autocomplete="email" aria-describedby="email-hint"><p class="hint" id="email-hint">We’ll mail you a link. You set your name and password after you open it.</p></div><button type="submit">Send sign-up link</button><p class="fine">By creating an account, you agree to the <a href="https://richview.ai/terms">Terms</a> and <a href="https://richview.ai/privacy">Privacy policy</a>.</p></form><div class="steps-aside"><h2>After you sign up</h2><ol><li>Create a CLI token for your agent.</li><li>Your agent publishes a report with one command.</li><li>You choose who opens it: only you, your organization, people you name, or anyone with the link.</li></ol></div><p class="ways"><span>Already have an account? <a href="/">Sign in</a></span></p></div>"#.to_owned()
}

/// A report the caller has already found the signed-in person may open.
pub(super) struct Report<'a> {
    pub key: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    /// "You", or the owner's name.
    pub owner: &'a str,
    pub kind: &'a str,
    pub badge: &'a str,
    pub who: &'a str,
    pub size: Option<u64>,
    pub can_share: bool,
    pub updated: &'a str,
    pub version: &'a str,
    pub url: &'a str,
    /// A small copy of the version's share card as a `data:` URL, when one has been drawn.
    pub thumb: Option<&'a str>,
}

/// An invitation to an organization, addressed to the signed-in person.
pub(super) struct Invite<'a> {
    pub id: &'a str,
    pub organization: &'a str,
    pub inviter: &'a str,
    pub role: &'a str,
}

pub(super) struct ReportList<'a> {
    pub nav: &'a Nav<'a>,
    pub server: &'a str,
    pub rows: &'a [Report<'a>],
    /// Reports the person can open, before the filter and the search.
    pub total: usize,
    /// `all`, `mine`, `shared` or `org`.
    pub show: &'a str,
    pub query: &'a str,
    pub notice: &'a str,
    pub invites: &'a [Invite<'a>],
    /// Whether the person may publish in the current organization.
    pub can_publish: bool,
    pub usage: Option<&'a AccountUsage>,
}

fn create_organization(csrf: &str) -> String {
    format!(
        r#"<p>You’re not in an organization yet. Start your own to publish reports, or ask a colleague to invite you to theirs.</p><form class="share-form share-form--one" method="post" action="/ui/orgs"><input type="hidden" name="csrf" value="{}"><div class="field"><label for="org-name">Organization name</label><input id="org-name" name="name" required maxlength="80"></div><button type="submit">Create organization</button></form>"#,
        escape(csrf)
    )
}

/// The one list: the person's own reports, their organization's and those shared with them.
pub(super) fn reports(list: &ReportList<'_>) -> String {
    let nav = list.nav;
    let lede = if list.total == 0 {
        match nav.org {
            Some(_) if list.can_publish => "<p class=\"lede\">No reports yet. What you publish is private until you share it.</p>".to_owned(),
            Some((_, name)) => format!("<p class=\"lede\">No reports yet. Reports that colleagues share with {} or with you appear here.</p>", escape(name)),
            None => String::new(),
        }
    } else {
        String::new()
    };
    let usage = list.usage.map_or_else(String::new, meters);
    let lede = if lede.is_empty() && list.total > 0 {
        format!(
            "<p class=\"lede\">{} {} you can open</p>",
            list.total,
            if list.total == 1 { "report" } else { "reports" }
        )
    } else {
        lede
    };
    let mut body = format!(
        r#"{}<div class="page-head"><div><h1>Reports</h1>{lede}</div>{usage}</div>{}"#,
        app_chrome(nav, "reports", list.show, list.query, None),
        note(list.notice)
    );
    for invite in list.invites {
        body.push_str(&format!(r#"<form class="note note--info invite" method="post" action="/ui/invitations/{}/accept"><input type="hidden" name="csrf" value="{}"><span>{} invited you to join <strong>{}</strong> as {}.</span><button type="submit">Join {}</button></form>"#, escape(invite.id), escape(nav.csrf), escape(invite.inviter), escape(invite.organization), escape(invite.role), escape(invite.organization)));
    }
    if list.total == 0 {
        match nav.org {
            None => body.push_str(&create_organization(nav.csrf)),
            Some(_) if list.can_publish => {
                let command = format!(
                    "export RICHVIEW_SERVER={}\nexport RICHVIEW_TOKEN=your-token\nrichview publish report.rmd --new",
                    shell_quote(list.server)
                );
                // An organization still carries the name sign-up gave it, after its owner, until
                // it is renamed; colleagues see that name, so the owner hears where it changes.
                let rename = match nav.org {
                    Some((_, name))
                        if nav.role == Some("Owner")
                            && name == crate::access::first_organization_name(nav.name) =>
                    {
                        format!(
                            "Your organization is “{}” until you rename it in <a href=\"/members#organization\">Organization settings</a>; colleagues see its name on invitations and shared reports. ",
                            escape(name)
                        )
                    }
                    _ => String::new(),
                };
                // The list's place comes first and the set-up second: someone who signs in expects
                // their reports here (Daniel, 16 Nov).
                body.push_str(&format!(r#"<ul class="rows report-rows report-rows--none"><li><div class="main"><h2>Your reports are listed here</h2><p class="about">Each with who can open it, its versions and a link to share.</p></div><div class="acts"><a class="act" href="https://richview.ai/showcase/openai-vs-anthropic/">Open an example</a></div></li></ul><section class="first"><h2>Publish your first report</h2><ol><li><p><strong>Install RichView.</strong> The command-line tool your agent uses to write and publish reports.</p><a class="act" href="https://richview.ai/docs/install">Install RichView</a></li><li><p><strong>Create a CLI token.</strong> It lets the tool publish as you. You see it once, with the two lines that set it.</p><a class="act" href="/tokens">Create a CLI token</a></li><li><p><strong>Publish.</strong> Ask your agent to publish a report, or run:</p>{}</li></ol><p class="then">{}<a href="/members">Invite your colleagues</a>, then share a report with them from its page.</p></section>"#, shell(&command), rename));
            }
            Some(_) => {}
        }
        return body;
    }
    // Someone who only has reports shared with them can still start an organization of their own.
    // They came to read, so the form waits behind one line; while an invitation waits above, joining
    // is the way in and the line goes.
    let start = match nav.org {
        None if list.show == "all" && list.invites.is_empty() => format!(
            "<details class=\"start-own\"><summary>Publish reports of your own</summary>{}</details>",
            create_organization(nav.csrf)
        ),
        _ => String::new(),
    };
    let query = escape(list.query);
    let filter = |id: &str, label: &str| {
        let href = match (id, list.query.is_empty()) {
            ("all", true) => "/".to_owned(),
            ("all", false) => format!("/?q={}", super::encode_value(list.query)),
            (_, true) => format!("/?show={id}"),
            (_, false) => format!("/?show={id}&q={}", super::encode_value(list.query)),
        };
        format!(
            "<a href=\"{}\"{}>{}</a>",
            escape(&href),
            if id == list.show {
                " aria-current=\"true\""
            } else {
                ""
            },
            escape(label)
        )
    };
    // The search box sits in the top bar; a search names what it looked for here.
    let searched = if list.query.is_empty() {
        String::new()
    } else {
        format!(
            "<p class=\"report-searched\">Titles with “{query}” · <a href=\"{}\">Clear search</a></p>",
            if list.show == "all" {
                "/".to_owned()
            } else {
                format!("/?show={}", escape(list.show))
            }
        )
    };
    body.push_str(&format!(
        "<div class=\"report-panel\"><div class=\"report-tools\"><nav class=\"report-filter\" aria-label=\"Show\">{}{}{}{}</nav>{searched}<p class=\"report-order\">Newest first</p></div>",
        filter("all", "All"),
        nav.org
            .map_or_else(String::new, |_| filter("mine", "Mine")),
        filter("shared", "Shared with me"),
        nav.org.map_or_else(String::new, |(_, name)| filter("org", name)),
    ));
    if list.rows.is_empty() {
        body.push_str(match (list.query.is_empty(), list.show) {
            (false, _) => "<p class=\"report-none\">No report title matches your search.</p>",
            (true, "mine") => {
                "<p class=\"report-none\">You haven’t published a report here yet.</p>"
            }
            (true, "shared") => {
                "<p class=\"report-none\">Nobody has shared a report with you by name yet.</p>"
            }
            _ => "<p class=\"report-none\">Nothing here yet.</p>",
        });
        body.push_str("</div>");
        body.push_str(&start);
        return body;
    }
    body.push_str(r#"<table class="report-table"><thead><tr><th scope="col">Name</th><th scope="col">Who can open it</th><th scope="col" class="r-owner">Owner</th><th scope="col">Updated</th><th scope="col"><span class="vh">Actions</span></th></tr></thead><tbody>"#);
    for row in list.rows {
        let key = escape(row.key);
        let title = escape(row.title);
        let (action, label) = if row.can_share {
            ("Share", "Sharing and versions")
        } else {
            ("Details", "Details and versions")
        };
        let thumb = row.thumb.map_or_else(
            || {
                format!(
                    "<span class=\"thumb thumb--none\">{}</span>",
                    icon("report")
                )
            },
            |src| {
                format!(
                    "<img class=\"thumb\" src=\"{}\" alt=\"\" width=\"60\" height=\"32\">",
                    escape(src)
                )
            },
        );
        let access = match row.kind {
            "org" => "org",
            "public" => "globe",
            "you" => "inbox",
            _ => "lock",
        };
        let (url, more) = (escape(row.url), icon("more"));
        body.push_str(&format!(r#"<tr><td class="r-name"><div>{thumb}<div><h2><a href="/reports/{key}">{title}</a><a class="r-open" href="/r/{key}">{title}</a></h2><p class="about">{}</p></div></div></td><td class="r-access"><span class="vis vis--{}">{}{}</span><span class="r-who">{}</span></td><td class="r-owner">{}</td><td class="r-when"><span>{}</span><span>{}</span></td><td class="r-acts"><div class="acts"><a class="act act--open" href="/r/{key}">Open</a><figure class="docs-code copy-link"><code hidden>{}</code><button class="act" type="button" data-copy-code aria-label="Copy link to {title}">Copy link</button></figure><a class="act" href="/reports/{key}" aria-label="{label} of {title}">{action}</a></div><details class="r-more"><summary aria-label="More for {title}">{more}</summary><div class="r-more__menu"><figure class="docs-code copy-link"><code hidden>{url}</code><button type="button" data-copy-code aria-label="Copy link to {title}">Copy link</button></figure><a href="/reports/{key}" aria-label="{label} of {title}">{action}</a></div></details></td></tr>"#, escape(row.description), escape(row.kind), icon(access), escape(row.badge), escape(row.who), escape(row.owner), escape(row.updated), escape(&[row.version.to_owned(), row.size.map_or_else(|| "Size unavailable".to_owned(), mebibytes)].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")), escape(row.url)));
    }
    body.push_str("</tbody></table></div>");
    body.push_str(&start);
    body
}

/// A person named on a report.
pub(super) struct Person<'a> {
    pub email: &'a str,
    /// Shown only for a member of the report's organization.
    pub name: Option<&'a str>,
    /// `viewer` or `editor`.
    pub role: &'a str,
    pub pending: bool,
}

/// The sharing part of a report page, for someone who may change it.
pub(super) struct Sharing<'a> {
    pub owner_name: &'a str,
    pub owner_email: &'a str,
    pub people: &'a [Person<'a>],
}

pub(super) struct ReportPage<'a> {
    pub nav: &'a Nav<'a>,
    pub key: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub owner: &'a str,
    pub organization: &'a str,
    /// `private`, `org` or `public`.
    pub visibility: &'a str,
    /// The person reaches the report because it was shared with them by name.
    pub shared_with_you: bool,
    pub url: &'a str,
    /// Version, date, newest first.
    pub versions: &'a [(String, String)],
    /// The version `url` opens.
    pub latest: &'a str,
    pub sharing: Option<Sharing<'a>>,
    pub can_delete: bool,
    pub notice: &'a str,
}

/// Who can open a report, as one badge: the class suffix and the words.
fn visibility_badge(kind: &str, organization: &str) -> String {
    let label = match kind {
        "org" => escape(organization),
        "public" => "Public link".to_owned(),
        "you" => "Shared with you".to_owned(),
        _ => "Private".to_owned(),
    };
    let kind = match kind {
        "org" | "public" | "you" => kind,
        _ => "private",
    };
    let glyph = match kind {
        "org" => "org",
        "public" => "globe",
        "you" => "inbox",
        _ => "lock",
    };
    format!(
        "<span class=\"vis vis--{kind}\">{}{label}</span>",
        icon(glyph)
    )
}

fn people_count(count: usize) -> String {
    match count {
        0 => "Not shared with anyone".to_owned(),
        1 => "Shared with 1 person".to_owned(),
        n => format!("Shared with {n} people"),
    }
}

/// One report: its link, who can open it, the people it is shared with, its versions and its removal.
pub(super) fn report(page: &ReportPage<'_>) -> String {
    let key = escape(page.key);
    let csrf = escape(page.nav.csrf);
    let organization = escape(page.organization);
    let date = page
        .versions
        .iter()
        .find(|(version, _)| version == page.latest)
        .map(|(_, date)| format!(" · {}", escape(date)))
        .unwrap_or_default();
    let (badge, who) = match &page.sharing {
        Some(sharing) => (
            visibility_badge(page.visibility, page.organization),
            format!(
                "<span>{}</span><span>{}{}</span>",
                match page.visibility {
                    "org" => format!("Every member of {organization}"),
                    "public" => "Anyone with the link".to_owned(),
                    _ => people_count(sharing.people.len()),
                },
                escape(page.owner),
                and_organization(sharing.owner_name, page.organization)
            ),
        ),
        None => (
            visibility_badge(
                if page.shared_with_you {
                    "you"
                } else {
                    page.visibility
                },
                page.organization,
            ),
            format!(
                "<span>By {}{}</span>",
                escape(page.owner),
                and_organization(page.owner, page.organization)
            ),
        ),
    };
    let description = if page.description.is_empty() {
        String::new()
    } else {
        format!("<p class=\"lede\">{}</p>", escape(page.description))
    };
    let url = escape(page.url);
    let mut body = format!(
        "{}<p class=\"back\"><a href=\"/\">Reports</a></p><div class=\"page-head report-head\"><div><h1>{}</h1>{description}<p class=\"state\">{badge}{who}<span>v{}{date}</span></p><div class=\"address\"><a class=\"docs-button\" href=\"/r/{key}\">Open report</a><input class=\"url\" readonly value=\"{url}\" aria-label=\"Link to this report\"><figure class=\"docs-code copy-link\"><code hidden>{url}</code><button class=\"act\" type=\"button\" data-copy-code aria-label=\"Copy link to this report\">Copy link</button></figure></div></div></div>{}",
        report_navigation(page.nav, page.title),
        escape(page.title),
        escape(page.latest),
        note(page.notice),
    );
    if let Some(sharing) = &page.sharing {
        let way = |value: &str, label: &str, text: &str, button: &str, class: &str| {
            let now = page.visibility == value;
            format!(
                "<div class=\"open-way open-way--{value}{}\"{}>{}<p>{text}</p><div class=\"way-foot\">{}</div></div>",
                if now { " is-now" } else { "" },
                if now { " aria-current=\"true\"" } else { "" },
                visibility_badge(value, label),
                if now {
                    "<span class=\"now\">Current setting</span>".to_owned()
                } else {
                    format!(
                        "<button class=\"docs-button {class}\" type=\"submit\" name=\"visibility\" value=\"{value}\">{button}</button>"
                    )
                }
            )
        };
        body.push_str(&format!(
            "<section><h2>Who can open it</h2><p>One of three. A change takes effect at once, and you can change it back.</p><form class=\"open-ways\" method=\"post\" action=\"/ui/share/{key}\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">{}{}{}</form></section>",
            way(
                "private",
                page.organization,
                &format!("Only the owner, the owners and admins of {organization}, and the people named below."),
                "Make private",
                "docs-button--secondary",
            ),
            way(
                "org",
                page.organization,
                &format!("Every member of {organization} can open it. Nobody outside can, unless you name them below."),
                &format!("Share with {organization}"),
                "docs-button--secondary",
            ),
            way(
                "public",
                page.organization,
                "<strong>Anyone with the link can open it, without signing in.</strong> Use it only for a report that holds nothing private.",
                "Make public",
                "docs-button--danger",
            ),
        ));
        let group = |who: &str, why: &str, can: &str| {
            format!(
                "<li><div class=\"who\"><strong>{who}</strong><span class=\"quiet\">{why}</span></div><div class=\"cells\">{can}</div><div class=\"acts\"></div></li>"
            )
        };
        body.push_str(&format!(
            "<section><h2>People</h2><p>Name someone to give them this report, inside or outside {organization}.</p><form class=\"share-form\" method=\"post\" action=\"/ui/reports/{key}/people\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><div class=\"field\"><label for=\"share-email\">Email</label><input id=\"share-email\" type=\"email\" name=\"email\" required autocomplete=\"off\"></div><div class=\"field\"><label for=\"share-role\">Can</label><select id=\"share-role\" name=\"role\"><option value=\"viewer\">Read</option><option value=\"editor\">Read and publish versions</option></select></div><button type=\"submit\">Share</button><p class=\"hint\">They get a mail with a link to the report. Someone without an account sets one up from that link and sees this report only.</p></form><ul class=\"rows rows--3\"><li><div class=\"who\"><strong>{}</strong> <span class=\"tag\">Owner</span><span class=\"quiet\">{}</span></div><div class=\"cells\">Everything</div><div class=\"acts\"></div></li>{}",
            escape(sharing.owner_name),
            escape(sharing.owner_email),
            group(&format!("Owners and admins of {organization}"), "Organization", "Everything"),
        ));
        if page.visibility != "private" {
            body.push_str(&group(
                &format!("Every member of {organization}"),
                "Organization",
                "Read",
            ));
        }
        if page.visibility == "public" {
            body.push_str(&group("Anyone with the link", "Public link", "Read"));
        }
        // The notice after sharing names the address first; that person's row says what happens next.
        let notice = page.notice.to_lowercase();
        for person in sharing.people {
            let email = escape(person.email);
            let who = person.name.map_or_else(
                || format!("<strong>{email}</strong>"),
                |name| {
                    format!(
                        "<strong>{}</strong><span class=\"quiet\">{email}</span>",
                        escape(name)
                    )
                },
            );
            let just_shared = notice
                .starts_with(&format!("shared with {}.", person.email.to_lowercase()))
                && !notice.contains("could not be sent");
            body.push_str(&format!(
                "<li{}><div class=\"who\">{who}</div><div class=\"cells\">{}{}</div><div class=\"acts\"><form method=\"post\" action=\"/ui/reports/{key}/people/remove\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><input type=\"hidden\" name=\"email\" value=\"{email}\"><button class=\"act--danger\" type=\"submit\" aria-label=\"Remove {email}\">Remove</button></form></div>{}</li>",
                if just_shared { " class=\"is-new\"" } else { "" },
                if person.role == "editor" {
                    "Read and publish versions"
                } else {
                    "Read"
                },
                if person.pending {
                    " <span class=\"tag\">Invited, not opened yet</span>"
                } else {
                    ""
                },
                if just_shared {
                    format!(
                        "<p class=\"row-said\">We’re sending {email} a mail with a link. Until they open it, they show here as invited.</p>"
                    )
                } else {
                    String::new()
                }
            ));
        }
        body.push_str("</ul></section>");
    }
    body.push_str("<section><h2>Versions</h2><ul class=\"rows rows--3 narrow\">");
    for (version, date) in page.versions {
        let v = escape(version);
        body.push_str(&format!(
            "<li><div class=\"who\"><strong>v{v}</strong>{}</div><div class=\"cells\">{}</div><div class=\"acts\"><a class=\"act\" href=\"/r/{key}/v/{v}\" aria-label=\"Open version {v}\">Open</a></div></li>",
            if version == page.latest {
                " <span class=\"tag\">Latest</span>"
            } else {
                ""
            },
            escape(date)
        ));
    }
    body.push_str("</ul></section>");
    if page.can_delete {
        let versions = match page.versions.len() {
            1 => "its one version".to_owned(),
            2 => "both versions".to_owned(),
            n if n < 50 => format!("all {n} versions"),
            _ => "every version".to_owned(),
        };
        body.push_str(&format!(
            "<section class=\"danger-zone\"><div><h2>Delete this report</h2><p>Removes {versions} and ends everyone’s access. The address is never used again.</p></div><a class=\"docs-button docs-button--danger\" href=\"/reports/{key}/delete\">Delete report…</a></section>"
        ));
    }
    body
}

pub(super) struct Member<'a> {
    pub person: &'a str,
    pub name: &'a str,
    pub email: &'a str,
    /// `owner`, `admin`, `author` or `reader`.
    pub role: &'a str,
    pub me: bool,
    /// Whether the person looking may change this member's role or remove them.
    pub manageable: bool,
    /// The day they joined.
    pub joined: &'a str,
    /// How many of the organization's reports they own: shown to owners and admins only.
    pub reports: Option<usize>,
}

pub(super) struct PendingInvite<'a> {
    pub id: &'a str,
    pub email: &'a str,
    pub role: &'a str,
    pub inviter: &'a str,
    pub expires: &'a str,
}

pub(super) struct MembersPage<'a> {
    pub nav: &'a Nav<'a>,
    pub members: &'a [Member<'a>],
    /// Empty unless the person looking is an owner or admin.
    pub invites: &'a [PendingInvite<'a>],
    pub manager: bool,
    pub owner: bool,
    /// The roles the person looking may give, highest first.
    pub roles: &'a [&'a str],
    pub notice: &'a str,
    /// The organization's usage, for those who may publish there.
    pub usage: Option<&'a AccountUsage>,
    /// Who takes the person's reports if they leave: the longest-standing other owner. `None`
    /// when they are its only owner, who cannot leave.
    pub heir: Option<&'a str>,
    /// How many of the organization's reports the person looking owns, when known.
    pub own_reports: Option<usize>,
}

pub(super) fn role_label(role: &str) -> &'static str {
    match role {
        "owner" => "Owner",
        "admin" => "Admin",
        "author" => "Author",
        _ => "Reader",
    }
}

/// What a role can do, after its name in a role menu.
fn role_detail(role: &str) -> &'static str {
    match role {
        "owner" => "Everything, including other owners",
        "admin" => "Manages members and every report",
        "author" => "Publishes reports and shares them",
        _ => "Opens what is shared",
    }
}

/// What a role can do, as a sentence under the invitation's role choice.
fn role_sentence(role: &str, organization: &str) -> String {
    match role {
        "owner" => "Owners can do everything, including managing other owners.".to_owned(),
        "admin" => "Admins manage members, invitations and every report.".to_owned(),
        "author" => "Authors publish reports and choose who opens them.".to_owned(),
        _ => format!("Readers open the reports shared with them or with {organization}."),
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A person's initials in a circle, as the rail shows the person signed in.
fn avatar(name: &str, me: bool) -> String {
    format!(
        "<span class=\"avatar{}\" aria-hidden=\"true\">{}</span>",
        if me { " avatar--me" } else { "" },
        escape(&initials(name))
    )
}

/// A menu that works without script: a details element whose summary is the trigger. While it is
/// open the summary's backdrop covers the page, so a click anywhere closes it; on a phone the menu
/// is a sheet at the bottom of the screen.
fn menu(class: &str, trigger: &str, label: &str, body: &str) -> String {
    format!(
        "<details class=\"pick{class}\"><summary class=\"pick__btn\"{label}>{trigger}</summary>{body}</details>"
    )
}

/// A member's role as a chip whose menu lists the roles the person looking may give; one click on
/// another posts the same `role` field the server reads.
fn role_menu(org: &str, csrf: &str, member: &Member<'_>, roles: &[&str]) -> String {
    let name = escape(member.name);
    let items: String = roles
        .iter()
        .rev()
        .map(|role| {
            let now = *role == member.role;
            format!(
                "<button class=\"pick__item\" type=\"submit\" name=\"role\" value=\"{role}\" aria-pressed=\"{now}\">{}<span><strong>{}</strong><span>{}</span></span></button>",
                icon("check"),
                role_label(role),
                role_detail(role)
            )
        })
        .collect();
    menu(
        "",
        &format!(
            "<span class=\"vh\">Role of {name}: </span>{}{}",
            role_label(member.role),
            icon("chev")
        ),
        "",
        &format!(
            "<form class=\"pick__menu\" method=\"post\" action=\"/ui/orgs/{org}/members/{}/role\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><p class=\"pick__head\">Role of <strong>{name}</strong></p>{items}</form>",
            escape(member.person)
        ),
    )
}

/// A destructive step behind a confirmation that says what happens: the summary opens it, and
/// again closes it.
fn confirm(class: &str, open: &str, close: &str, said: &str, form: &str) -> String {
    format!(
        "<details class=\"confirm{class}\"><summary><span class=\"confirm__open\">{open}</span><span class=\"confirm__close\">{close}</span></summary><div class=\"confirm__body\"><p>{said}</p>{form}</div></details>"
    )
}

fn invite_bar(page: &MembersPage<'_>, org: &str, csrf: &str, organization: &str) -> String {
    let roles: Vec<&str> = page.roles.iter().rev().copied().collect();
    let segments: String = roles
        .iter()
        .enumerate()
        .map(|(at, role)| {
            format!(
                "<input type=\"radio\" id=\"invite-role-{role}\" name=\"role\" value=\"{role}\"{} aria-describedby=\"invite-says-{role}\"><label for=\"invite-role-{role}\">{}</label>",
                if at == 0 { " checked" } else { "" },
                role_label(role)
            )
        })
        .collect();
    let says: String = roles
        .iter()
        .map(|role| {
            format!(
                "<span id=\"invite-says-{role}\" data-role=\"{role}\">{}</span>",
                role_sentence(role, organization)
            )
        })
        .collect();
    format!(
        "<form class=\"invite\" id=\"invite\" method=\"post\" action=\"/ui/orgs/{org}/invitations\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><div class=\"field invite__email\"><label for=\"invite-email\">Invite by email</label><input id=\"invite-email\" type=\"email\" name=\"email\" required autocomplete=\"off\" inputmode=\"email\" placeholder=\"name@company.com\"></div><fieldset class=\"invite__role\"><legend>Role</legend><div class=\"seg\">{segments}</div></fieldset><button class=\"docs-button\" type=\"submit\">Send invitation</button><p class=\"invite__says\">{says} They get a mail whose link works once within 24 hours; the invitation stays open for a week.</p></form>"
    )
}

/// The people of the current organization, its open invitations and its settings.
pub(super) fn members(page: &MembersPage<'_>) -> String {
    let nav = page.nav;
    let csrf = escape(nav.csrf);
    let Some((org, name)) = nav.org else {
        return format!(
            "{}<h1>Members</h1>{}{}",
            navigation(nav, "members"),
            note(page.notice),
            create_organization(nav.csrf)
        );
    };
    let org = escape(org);
    let organization = escape(name);
    let mut lede = format!(
        "{} of {organization}",
        plural(page.members.len(), "member", "members")
    );
    if !page.invites.is_empty() {
        lede.push_str(&format!(
            " · {} open",
            plural(page.invites.len(), "invitation", "invitations")
        ));
    }
    let mut body = format!(
        "{}<div class=\"members\"><div class=\"page-head\"><div><h1>Members</h1><p class=\"lede\">{lede}</p></div><a class=\"docs-button docs-button--secondary to-settings\" href=\"#organization\">{}<span>Organization settings</span></a></div>{}<div class=\"people\">",
        navigation(nav, "members"),
        icon("org"),
        note(page.notice)
    );
    if page.manager {
        body.push_str(&invite_bar(page, &org, &csrf, &organization));
    }
    // Only owners and admins act on a row, so only they get the report counts and the actions.
    let columns = if page.manager { 5 } else { 3 };
    body.push_str(&format!(
        "<table class=\"people-table\"><thead><tr><th scope=\"col\">Name</th><th scope=\"col\" class=\"p-role\">Role</th>{}<th scope=\"col\" class=\"p-joined\">Joined</th>{}</tr></thead><tbody>",
        if page.manager {
            "<th scope=\"col\" class=\"p-num\">Reports</th>"
        } else {
            ""
        },
        if page.manager {
            "<th scope=\"col\" class=\"p-acts\"><span class=\"vh\">Actions</span></th>"
        } else {
            ""
        }
    ));
    for member in page.members {
        let member_name = escape(member.name);
        let email = escape(member.email);
        let joined = escape(member.joined);
        let role = if member.manageable {
            role_menu(&org, &csrf, member, page.roles)
        } else {
            format!(
                "<span class=\"p-role-text\">{}</span>",
                role_label(member.role)
            )
        };
        let meta = [
            member
                .reports
                .map(|count| plural(count, "report", "reports")),
            (!member.joined.is_empty()).then(|| format!("joined {joined}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let more = if member.manageable && !member.me {
            let passes = match member.reports {
                Some(0) | None => String::new(),
                Some(count) => {
                    format!(" Their {} pass to you.", plural(count, "report", "reports"))
                }
            };
            menu(
                " pick--more",
                &icon("more"),
                &format!(" aria-label=\"More for {member_name}\""),
                &format!(
                    "<div class=\"pick__menu\"><p class=\"pick__head\"><strong>{member_name}</strong> {email}</p>{}</div>",
                    confirm(
                        " confirm--item",
                        &format!("Remove from {organization}…"),
                        &format!("Keep {member_name}"),
                        &format!(
                            "{member_name} loses access to {organization} at once.{passes} Their CLI tokens for {organization} stop working."
                        ),
                        &format!(
                            "<form method=\"post\" action=\"/ui/orgs/{org}/members/{}/remove\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"docs-button docs-button--danger-solid\" type=\"submit\">Remove {member_name}</button></form>",
                            escape(member.person)
                        )
                    )
                ),
            )
        } else {
            String::new()
        };
        body.push_str(&format!(
            "<tr{}><td class=\"p-who\"><div>{}<div><strong>{member_name}</strong>{}<span class=\"p-mail\">{email}</span><span class=\"p-meta\">{meta}</span></div></div></td><td class=\"p-role\">{role}</td>{}<td class=\"p-joined\">{joined}</td>{}</tr>",
            if member.me { " class=\"is-me\"" } else { "" },
            avatar(member.name, member.me),
            if member.me { " <span class=\"tag\">You</span>" } else { "" },
            member
                .reports
                .map_or_else(String::new, |count| format!("<td class=\"p-num\">{count}</td>")),
            if page.manager {
                format!("<td class=\"p-acts\">{more}</td>")
            } else {
                String::new()
            }
        ));
    }
    if page.manager && page.members.len() == 1 && page.invites.is_empty() {
        body.push_str(&format!(
            "<tr class=\"p-alone\"><td colspan=\"{columns}\">Just you so far. Invite a colleague to share reports with them, or to let them publish to {organization}.</td></tr>"
        ));
    }
    body.push_str("</tbody>");
    if !page.invites.is_empty() {
        // The notice after inviting names the address; that row is marked as the new one.
        let notice = page.notice.to_lowercase();
        body.push_str(&format!(
            "<tbody class=\"p-invited\"><tr class=\"p-group\"><th scope=\"rowgroup\" colspan=\"{columns}\">Invited <span>{}</span></th></tr>",
            page.invites.len()
        ));
        for invite in page.invites {
            let id = escape(invite.id);
            let email = escape(invite.email);
            let address = invite.email.to_lowercase();
            let new = (notice.starts_with(&format!("invited {address}."))
                || notice.starts_with(&format!("invited {address} again.")))
                && !notice.contains("could not be sent");
            let again = |class: &str| {
                format!(
                    "<form method=\"post\" action=\"/ui/orgs/{org}/invitations/{id}/resend\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"{class}\" type=\"submit\" aria-label=\"Send the invitation to {email} again\">Send again</button></form>"
                )
            };
            let cancel = |class: &str, label: &str| {
                format!(
                    "<form method=\"post\" action=\"/ui/orgs/{org}/invitations/{id}/cancel\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"{class}\" type=\"submit\" aria-label=\"Cancel the invitation to {email}\">{label}</button></form>"
                )
            };
            body.push_str(&format!(
                "<tr class=\"p-invite{}\"><td class=\"p-who\"><div><span class=\"avatar avatar--invite\" aria-hidden=\"true\">{}</span><div><strong>{email}</strong><span class=\"p-mail\">Invited by {} · open until {}</span></div></div></td><td class=\"p-role\"><span class=\"p-role-text\">{}</span></td>{}<td class=\"p-joined\"></td><td class=\"p-acts\"><div class=\"acts p-inline\">{}{}</div>{}</td></tr>",
                if new { " is-new" } else { "" },
                icon("mail"),
                escape(invite.inviter),
                escape(invite.expires),
                role_label(invite.role),
                if page.manager { "<td class=\"p-num\"></td>" } else { "" },
                again("act"),
                cancel("act act--quiet-danger", "Cancel"),
                menu(
                    " pick--more",
                    &icon("more"),
                    &format!(" aria-label=\"More for the invitation to {email}\""),
                    &format!(
                        "<div class=\"pick__menu\"><p class=\"pick__head\"><strong>{email}</strong> Open until {}</p>{}{}</div>",
                        escape(invite.expires),
                        again("pick__item pick__item--plain"),
                        cancel("pick__item pick__item--plain pick__item--danger", "Cancel invitation")
                    ),
                )
            ));
        }
        body.push_str("</tbody>");
    }
    body.push_str("</table>");
    if !page.manager {
        body.push_str("<p class=\"roles-key\"><strong>Reader</strong> opens what is shared. <strong>Author</strong> publishes reports. <strong>Admin</strong> manages members and every report. <strong>Owner</strong> does everything, including other owners.</p>");
    }
    body.push_str("</div>");
    body.push_str(&organization_settings(
        page,
        &org,
        &csrf,
        &organization,
        name,
    ));
    body.push_str("</div>");
    body
}

/// The organization's own settings, one click from its people: its name, its usage and leaving it.
fn organization_settings(
    page: &MembersPage<'_>,
    org: &str,
    csrf: &str,
    organization: &str,
    raw_name: &str,
) -> String {
    let name = if page.owner {
        format!(
            "<form class=\"setting__form\" method=\"post\" action=\"/ui/orgs/{org}/name\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><label class=\"vh\" for=\"org-name\">Organization name</label><input id=\"org-name\" name=\"name\" required maxlength=\"80\" value=\"{}\"><button class=\"docs-button docs-button--secondary\" type=\"submit\">Rename</button></form>",
            escape(raw_name)
        )
    } else {
        format!("<p class=\"setting__value\">{organization}</p>")
    };
    let usage = page.usage.map_or_else(String::new, |usage| {
        format!(
            "<div class=\"setting\"><div class=\"setting__about\"><h3>Usage</h3><p>Every member’s reports count toward the organization’s limits.</p></div>{}</div>",
            meters(usage)
        )
    });
    let leave = match page.heir {
        None => "<p class=\"setting__value setting__value--quiet\">You’re its only owner. Make someone else an owner first, then you can leave.</p>".to_owned(),
        Some(heir) => {
            let passes = match page.own_reports {
                Some(0) | None => String::new(),
                Some(count) => format!(
                    " Your {} pass to {}.",
                    plural(count, "report", "reports"),
                    escape(heir)
                ),
            };
            confirm(
                " confirm--inline",
                &format!("Leave {organization}…"),
                "Stay",
                &format!(
                    "You lose access to {organization}’s reports at once.{passes} Your CLI tokens for {organization} stop working."
                ),
                &format!(
                    "<form method=\"post\" action=\"/ui/orgs/{org}/members/{}/remove\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"docs-button docs-button--danger-solid\" type=\"submit\">Leave {organization}</button></form>",
                    escape(&me_person(page))
                ),
            )
        }
    };
    format!(
        "<section class=\"org-settings\" id=\"organization\" aria-labelledby=\"organization-title\"><h2 id=\"organization-title\">Organization</h2><div class=\"settings\"><div class=\"setting\"><div class=\"setting__about\"><h3>Name</h3><p>Shown in the rail, on invitations and on the reports you share.</p></div>{name}</div>{usage}<div class=\"setting setting--danger\"><div class=\"setting__about\"><h3>Leave {organization}</h3><p>Someone can invite you again later.</p></div>{leave}</div></div></section>"
    )
}

fn me_person(page: &MembersPage<'_>) -> String {
    page.members
        .iter()
        .find(|member| member.me)
        .map_or_else(String::new, |member| member.person.to_owned())
}

/// Usage as the mark's bars, as the reports list shows it.
fn meters(usage: &AccountUsage) -> String {
    let (used, report_limit) = (usage.reports, usage.report_limit);
    let storage = escape(&mebibytes(usage.bytes));
    let limit = escape(&mebibytes(usage.byte_limit));
    let percent = if usage.byte_limit == 0 {
        0
    } else {
        ((usage.bytes as u128 * 100 / usage.byte_limit as u128).min(100)) as u64
    };
    format!(
        r#"<p class="usage"><span>Reports <strong>{used} of {report_limit}</strong></span><progress max="{report_limit}" value="{used}" aria-label="Reports used">{used} of {report_limit}</progress><span>Storage <strong>{storage} of {limit}</strong></span><progress max="100" value="{percent}" aria-label="Storage used">{storage} of {limit}</progress></p>"#
    )
}

/// What an invitation link opens. `token` is validated by the link handler; GET spends nothing.
pub(super) struct InvitePage<'a> {
    pub token: &'a str,
    pub inviter: &'a str,
    pub inviter_email: &'a str,
    pub organization: &'a str,
    /// The membership role in a sentence, for an invitation to the organization.
    pub role: Option<&'a str>,
    /// The report's title, for a shared report.
    pub report: Option<&'a str>,
    pub account_exists: bool,
    pub submitted_name: &'a str,
    pub error: &'a str,
}

/// What an invitation opens and who sent it, for the ink side beside its form, in place of the
/// flagship's card: the page renderer lifts it out of the body (`ui::INVITE_OPENING`). It repeats
/// what the lede says, so assistive technology reads the lede alone.
fn invitation_card(page: &InvitePage<'_>) -> String {
    let (verb, kind, glyph, name, from) = match page.report {
        Some(report) => (
            "shared a report with you",
            "Report",
            "report",
            report,
            (!same_name(page.inviter, page.organization)).then_some(page.organization),
        ),
        None => (
            "invited you to join",
            "Organization",
            "org",
            page.organization,
            None,
        ),
    };
    let from = from.map_or_else(String::new, |org| {
        format!(
            "<p class=\"auth-invite__org\">{}{}</p>",
            icon("org"),
            escape(org)
        )
    });
    format!(
        "<figure class=\"auth-invite\" aria-hidden=\"true\"><p class=\"auth-invite__from\"><span class=\"auth-invite__avatar\">{}</span><span><strong>{}</strong><span>{verb}</span></span></p><p class=\"auth-invite__kind\">{}{kind}</p><p class=\"auth-invite__title\">{}</p>{from}</figure>",
        escape(&initials(page.inviter)),
        escape(page.inviter),
        icon(glyph),
        escape(name)
    )
}

pub(super) fn accept_invite(page: &InvitePage<'_>) -> String {
    let card = invitation_card(page);
    let organization = escape(page.organization);
    let (title, sentence, action, button, hint) = match (page.report, page.role) {
        (Some(report), _) => (
            "Open the shared report".to_owned(),
            format!(
                "{} ({}) shared <strong>{}</strong>{} with you.",
                escape(page.inviter),
                escape(page.inviter_email),
                escape(report),
                if same_name(page.inviter, page.organization) {
                    String::new()
                } else {
                    format!(" from {organization}")
                }
            ),
            "open the report",
            "Create account and open the report",
            true,
        ),
        (None, role) => (
            format!("Join {organization}"),
            format!(
                "{} ({}) invited you to join {organization} on RichView as {}.",
                escape(page.inviter),
                escape(page.inviter_email),
                escape(role.unwrap_or("a member"))
            ),
            "join",
            "Create account and join",
            false,
        ),
    };
    let token = escape(page.token);
    let notice = note(page.error);
    let name_value = if page.submitted_name.is_empty() {
        String::new()
    } else {
        format!(r#" value="{}""#, escape(page.submitted_name))
    };
    if page.account_exists {
        return format!(
            r#"{card}<div class="cowork-auth"><h1>{title}</h1><p class="lede">{sentence}</p><form class="panel" method="post" action="/ui/link/{token}"><button type="submit">Accept and sign in</button><p class="fine">You already have a RichView account for this address. Accept, then sign in as usual.</p></form></div>"#
        );
    }
    let hint = if hint {
        format!(r#"<p class="hint">Set up your account to {action}.</p>"#)
    } else {
        String::new()
    };
    format!(
        r#"{card}<div class="cowork-auth"><h1>{title}</h1><p class="lede">{sentence}</p>{hint}{notice}<form class="panel" method="post" action="/ui/link/{token}"><div class="field"><label for="name">Name</label><input id="name" name="name" required autocomplete="name"{name_value}></div><div class="field"><label for="password">Password</label><input id="password" type="password" name="password" required minlength="12" autocomplete="new-password" aria-describedby="password-hint"><p class="hint" id="password-hint">At least 12 characters.</p></div><div class="field"><label for="password-confirm">Confirm password</label><input id="password-confirm" type="password" name="password_confirm" required minlength="12" autocomplete="new-password"></div><button type="submit">{button}</button><p class="fine">By creating an account, you agree to the <a href="https://richview.ai/terms">Terms</a> and <a href="https://richview.ai/privacy">Privacy policy</a>.</p></form></div>"#
    )
}

pub(super) fn sign_in(notice: &str, next: Option<&str>) -> String {
    let next = next.map_or_else(String::new, |value| {
        format!(
            r#"<input type="hidden" name="next" value="{}">"#,
            escape(value)
        )
    });
    format!(
        r#"<div class="cowork-auth"><h1>Sign in to RichView</h1>{}{}<form class="panel" method="post" action="/ui/sign-in">{next}<div class="field"><label for="email">Email</label><input id="email" type="email" name="email" required autocomplete="username"></div><div class="field field--password"><label for="password">Password</label><input id="password" type="password" name="password" required autocomplete="current-password"><a class="field__aside" href="/forgot">Forgot password?</a></div><button type="submit">Sign in</button></form><p class="ways"><span>New to RichView? <a href="/sign-up">Create account</a></span></p></div>"#,
        // Someone sent here to open a report needs the reason, not the author's pitch.
        if notice == "Sign in to open this report."
            || notice.starts_with("Accepted. Sign in to open")
        {
            ""
        } else {
            r#"<p class="lede">Open the reports your agent published and choose who reads them.</p>"#
        },
        note(notice)
    )
}

pub(super) fn check_mail(manual: bool, sender: Option<&str>) -> String {
    let message = if manual {
        "We’ll send a link to your inbox. In these first days we send links by hand, so it can take a few hours. If nothing arrives, look in your junk or spam folder too.".to_owned()
    } else {
        let from = sender.map_or_else(String::new, |address| {
            format!(" The mail is from {}.", escape(address))
        });
        format!(
            "We sent a link to your inbox.{from} If nothing arrives within a few minutes, look in your junk or spam folder too."
        )
    };
    format!(
        r#"<div class="cowork-auth"><h1>Check your mail</h1><p class="lede">{message}</p><p class="ways"><a href="/">Back to sign in</a></p></div>"#
    )
}

pub(super) fn forgot() -> String {
    r#"<div class="cowork-auth"><h1>Reset password</h1><p class="lede">Enter your email and we’ll send you a link if we can help.</p><form class="panel" method="post" action="/ui/forgot"><div class="field"><label for="email">Email</label><input id="email" type="email" name="email" required autocomplete="email"></div><button type="submit">Send reset link</button></form><p class="ways"><a href="/">Back to sign in</a></p></div>"#.to_owned()
}

/// The link is inspected without consuming it.
pub(super) fn complete_link(token: &str, signup: bool) -> String {
    complete_link_with_error(token, signup, "", "")
}

pub(super) fn complete_link_with_error(
    token: &str,
    signup: bool,
    name: &str,
    error: &str,
) -> String {
    let token = escape(token);
    let notice = note(error);
    let name_value = if name.is_empty() {
        String::new()
    } else {
        format!(r#" value="{}""#, escape(name))
    };
    if signup {
        format!(
            r#"<div class="cowork-auth"><h1>Set up your account</h1><p class="lede">Your address is confirmed. Add your name and choose a password.</p>{notice}<form class="panel" method="post" action="/ui/link/{token}"><div class="field"><label for="name">Name</label><input id="name" name="name" required autocomplete="name"{name_value}></div><div class="field"><label for="password">Password</label><input id="password" type="password" name="password" required minlength="12" autocomplete="new-password" aria-describedby="password-hint"><p class="hint" id="password-hint">At least 12 characters.</p></div><div class="field"><label for="password-confirm">Confirm password</label><input id="password-confirm" type="password" name="password_confirm" required minlength="12" autocomplete="new-password"></div><button type="submit">Create account</button></form></div>"#
        )
    } else {
        format!(
            r#"<div class="cowork-auth"><h1>Choose a new password</h1>{notice}<form class="panel" method="post" action="/ui/link/{token}"><div class="field"><label for="password">Password</label><input id="password" type="password" name="password" required minlength="12" autocomplete="new-password" aria-describedby="password-hint"><p class="hint" id="password-hint">At least 12 characters.</p></div><div class="field"><label for="password-confirm">Confirm password</label><input id="password-confirm" type="password" name="password_confirm" required minlength="12" autocomplete="new-password"></div><button type="submit">Save password</button></form></div>"#
        )
    }
}

pub(super) fn expired_link() -> String {
    r#"<div class="cowork-auth"><h1>Link expired or already used</h1><p class="lede">Request a new link to continue. For an invitation, ask the person who invited you to send it again, or sign in if you have an account: it is waiting on your reports page.</p><p class="cowork-actions"><a class="docs-button" href="/">Sign in</a></p><p class="ways"><a href="/sign-up">Create an account</a><a href="/forgot">Reset password</a></p></div>"#.to_owned()
}

pub(super) fn signups_paused() -> String {
    r#"<div class="cowork-auth"><h1>Sign-ups are paused</h1><p class="lede">We can’t create new accounts right now. Please try again later.</p><p class="ways"><a href="/">Sign in</a></p></div>"#.to_owned()
}

pub(super) fn suspended(nav: &Nav<'_>) -> String {
    format!(
        "{}<div class=\"cowork-text\"><h1>Account unavailable</h1><p>This organization is suspended. <a href=\"https://richview.ai/impressum\">Contact RichView</a> for help.</p></div>",
        navigation(nav, "reports")
    )
}

pub(super) struct Token<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub created: &'a str,
}

/// Only the response to token creation receives `secret`; it must never be stored in a URL.
pub(super) fn tokens(
    nav: &Nav<'_>,
    server: &str,
    rows: &[Token<'_>],
    secret: Option<&str>,
) -> String {
    let csrf = escape(nav.csrf);
    let mut body = format!(
        "{}<div class=\"page-head\"><div><h1>CLI tokens</h1><p class=\"lede\">A token lets the RichView command line, and your agent through it, publish to your organization with your role there.</p></div></div>",
        navigation(nav, "tokens")
    );
    if let Some(secret) = secret {
        let command = format!(
            "export RICHVIEW_SERVER={}\nexport RICHVIEW_TOKEN={}",
            shell_quote(server),
            shell_quote(secret)
        );
        body.push_str(&format!("<div class=\"token-ready\"><p class=\"note note--ok\" role=\"status\">Copy this setup command now. The token will not be shown again.</p>{}<p class=\"next\">Then run <code>richview publish report.rmd --new</code>, or ask your agent to publish.</p></div>", shell(&command)));
    }
    body.push_str(&format!("<section><h2>New token</h2><form class=\"share-form share-form--one\" method=\"post\" action=\"/ui/tokens\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><div class=\"field\"><label for=\"token-name\">Token name</label><input id=\"token-name\" name=\"name\" required aria-describedby=\"token-name-hint\"><p class=\"hint\" id=\"token-name-hint\">Name it after where it runs, such as “laptop” or “CI”.</p></div><button type=\"submit\">Create token</button></form></section><section><h2>Your tokens</h2>"));
    if rows.is_empty() {
        body.push_str("<p>No tokens yet. Create one to publish from the command line.</p>");
    } else {
        body.push_str("<p>Revoking a token stops everything that publishes with it, at once.</p><ul class=\"rows rows--3\">");
        for row in rows {
            let name = escape(row.name);
            body.push_str(&format!("<li><div class=\"who\"><strong>{name}</strong></div><div class=\"cells\">Created {}</div><div class=\"acts\"><form method=\"post\" action=\"/ui/tokens/{}/revoke\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"act--danger\" type=\"submit\" aria-label=\"Revoke {name}\">Revoke</button></form></div></li>", escape(row.created), escape(row.id)));
        }
        body.push_str("</ul>");
    }
    body.push_str("</section>");
    body
}

pub(super) struct AccountUsage {
    pub reports: usize,
    pub report_limit: u64,
    pub bytes: u64,
    pub byte_limit: u64,
}

fn mebibytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 MiB".into();
    }
    let value = bytes as f64 / (1024.0 * 1024.0);
    if value < 0.05 {
        "<0.1 MiB".into()
    } else if bytes.is_multiple_of(1024 * 1024) {
        format!("{} MiB", bytes / (1024 * 1024))
    } else {
        format!("{value:.1} MiB")
    }
}

pub(super) fn account(nav: &Nav<'_>, email: &str, usage: Option<&AccountUsage>) -> String {
    let usage = usage.map_or_else(
        || "<p>Usage is temporarily unavailable. Try again later.</p>".to_owned(),
        |usage| {
            let organization = escape(nav.org.map_or("your organization", |(_, name)| name));
            let used = escape(&mebibytes(usage.bytes));
            let limit = escape(&mebibytes(usage.byte_limit));
            let percent = if usage.byte_limit == 0 { 0 } else { (((u128::from(usage.bytes) * 100 + u128::from(usage.byte_limit) / 2) / u128::from(usage.byte_limit)).min(100)) as u64 };
            format!("<p>Usage of {organization}: {} of {} reports · {used} of {limit}</p><p class=\"usage\"><span>Reports <strong>{} of {}</strong></span><progress max=\"{}\" value=\"{}\" aria-label=\"Reports used\">{} of {}</progress><span>Storage <strong>{used} of {limit}</strong></span><progress max=\"100\" value=\"{percent}\" aria-label=\"Storage used\">{used} of {limit}</progress></p>", usage.reports, usage.report_limit, usage.reports, usage.report_limit, usage.report_limit, usage.reports, usage.reports, usage.report_limit)
        },
    );
    format!(
        "{}<div class=\"page-head\"><div><h1>Account</h1></div></div><dl class=\"facts narrow\"><dt>Name</dt><dd>{}</dd><dt>Email</dt><dd>{}</dd><dt>Password</dt><dd><a href=\"/forgot\">Change password</a></dd></dl><section class=\"narrow\"><h2>Usage</h2>{usage}</section><section class=\"danger-zone narrow\"><div><h2>Delete account</h2><p>To delete your account and its reports, write to the address in the <a href=\"https://richview.ai/impressum\">Impressum</a>.</p></div></section>",
        navigation(nav, "account"),
        escape(nav.name),
        escape(email)
    )
}

/// The key is supplied by an authorized report lookup, not an unvalidated path.
pub(super) fn delete_confirmation(nav: &Nav<'_>, key: &str, title: &str) -> String {
    let key = escape(key);
    let csrf = nav.csrf;
    format!(
        "{}<div class=\"cowork-auth cowork-confirm\"><h1>Delete report?</h1><p class=\"lede\">Delete <strong>{}</strong> and all its versions? This cannot be undone.</p><ul class=\"destroys\"><li>Every version is removed.</li><li>Everyone it is shared with loses access, and its link stops working.</li><li>The address is never used again.</li></ul><form class=\"cowork-actions\" method=\"post\" action=\"/ui/reports/{key}/delete\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"docs-button docs-button--danger-solid\" type=\"submit\">Delete report</button><a class=\"docs-button docs-button--secondary\" href=\"/reports/{key}\">Keep it</a></form></div>",
        report_navigation(nav, title),
        escape(title),
        escape(csrf)
    )
}

#[cfg(test)]
fn test_nav<'a>(name: &'a str, csrf: &'a str) -> Nav<'a> {
    Nav {
        name,
        email: "alex@example.test",
        csrf,
        org: Some(("ab123456", "Acme")),
        role: Some("Owner"),
        orgs: &[],
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn token_setup_quotes_shell_values_and_escapes_html() {
        let nav = test_nav("Alex", "csrf");
        let html = tokens(&nav, "https://example.test", &[], Some("abc'<script>"));
        assert!(html.contains(
            "RICHVIEW_SERVER=<wbr><span class=\"shell-value\">&#39;https://example.test&#39;"
        ));
        assert!(html.contains("abc&#39;\\&#39;&#39;&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(!tokens(&nav, "https://example.test", &[], None).contains("abc"));
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;

    fn list<'a>(nav: &'a Nav<'a>, rows: &'a [Report<'a>], total: usize) -> ReportList<'a> {
        ReportList {
            nav,
            server: "https://app.example",
            rows,
            total,
            show: "all",
            query: "",
            notice: "",
            invites: &[],
            can_publish: true,
            usage: None,
        }
    }

    #[test]
    fn empty_reports_put_the_list_first_then_install_token_publish_and_inviting() {
        let nav = test_nav("Alex", "csrf");
        let html = reports(&list(&nav, &[], 0));
        let listed = html.find("Your reports are listed here").unwrap();
        let install = html.find("https://richview.ai/docs/install").unwrap();
        let token = html.find("Create a CLI token").unwrap();
        let publish = html.find("publish a report").unwrap();
        let invite = html.find("Invite your colleagues").unwrap();
        assert!(listed < install && install < token && token < publish && publish < invite);
    }

    #[test]
    fn a_first_reports_page_names_the_organizations_sign_up_name_and_where_it_changes() {
        let named = crate::access::first_organization_name("Alex");
        let nav = Nav {
            org: Some(("ab123456", &named)),
            ..test_nav("Alex", "csrf")
        };
        let html = reports(&list(&nav, &[], 0));
        assert!(html.contains("Your organization is “Alex’s organization” until you rename it in <a href=\"/members#organization\">Organization settings</a>"));
        // Once renamed, or for someone who cannot rename it, the line goes.
        assert!(!reports(&list(&test_nav("Alex", "csrf"), &[], 0)).contains("until you rename it"));
        let author = Nav {
            role: Some("Author"),
            ..nav
        };
        assert!(!reports(&list(&author, &[], 0)).contains("until you rename it"));
    }

    #[test]
    fn empty_states_say_what_to_do_next_for_each_person() {
        let nav = test_nav("Alex", "csrf");
        let mut reader = list(&nav, &[], 0);
        reader.can_publish = false;
        let html = reports(&reader);
        assert!(html.contains("Reports that colleagues share with Acme or with you appear here."));
        assert!(!html.contains("Create a CLI token"));
        assert!(!html.contains("Your reports are listed here"));
        let alone = Nav {
            org: None,
            ..test_nav("Alex", "csrf")
        };
        let html = reports(&list(&alone, &[], 0));
        assert!(html.contains("action=\"/ui/orgs\""));
        assert!(html.contains("Create organization"));
        let mut searched = list(&nav, &[], 3);
        searched.query = "q<3";
        let html = reports(&searched);
        assert!(html.contains("No report title matches your search."));
        assert!(html.contains("value=\"q&lt;3\""));
    }

    #[test]
    fn a_reader_outside_any_organization_is_offered_one_only_behind_a_line_and_not_while_invited() {
        let alone = Nav {
            org: None,
            ..test_nav("Eve", "csrf")
        };
        let rows = [Report {
            key: "ab123456/report",
            title: "Revenue",
            description: "",
            owner: "Alex",
            kind: "you",
            badge: "Shared with you",
            who: "By Alex",
            size: None,
            can_share: false,
            updated: "11 Nov 2026",
            version: "v1.0.0",
            url: "https://example.test/r/ab123456/report",
            thumb: None,
        }];
        let html = reports(&list(&alone, &rows, 1));
        assert!(html.contains(
            "<details class=\"start-own\"><summary>Publish reports of your own</summary>"
        ));
        assert!(html.contains("action=\"/ui/orgs\""));
        let invites = [Invite {
            id: "i-1",
            organization: "Acme",
            inviter: "Alex",
            role: "an author",
        }];
        let mut invited = list(&alone, &rows, 1);
        invited.invites = &invites;
        let html = reports(&invited);
        assert!(html.contains("action=\"/ui/invitations/i-1/accept\""));
        assert!(!html.contains("start-own") && !html.contains("action=\"/ui/orgs\""));
    }

    #[test]
    fn account_shows_live_usage_and_links_to_help() {
        let nav = test_nav("Alex", "csrf");
        let html = account(
            &nav,
            "a@example.test",
            Some(&AccountUsage {
                reports: 2,
                report_limit: 50,
                bytes: 1_258_291,
                byte_limit: 50 * 1024 * 1024,
            }),
        );
        assert!(html.contains("Usage of Acme: 2 of 50 reports · 1.2 MiB of 50 MiB"));
        assert!(html.contains("<progress max=\"50\" value=\"2\" aria-label=\"Reports used\">"));
        assert!(html.contains("<progress max=\"100\" value=\"2\" aria-label=\"Storage used\">"));
        assert!(html.contains("href=\"/forgot\">Change password</a>"));
        assert!(html.contains("href=\"https://richview.ai/impressum\">Impressum</a>"));
        assert_eq!(mebibytes(0), "0 MiB");
        assert_eq!(mebibytes(1), "<0.1 MiB");
        assert!(
            account(&nav, "a@example.test", None).contains("Usage is temporarily unavailable.")
        );
    }

    #[test]
    fn check_mail_points_to_the_junk_folder_in_both_delivery_modes() {
        let automatic = check_mail(false, Some("accounts@richview.ai"));
        assert!(automatic.contains("accounts@richview.ai"));
        assert!(automatic.contains("junk or spam"));
        assert!(
            check_mail(false, Some("custom&sender@example.test"))
                .contains("custom&amp;sender@example.test")
        );
        let manual = check_mail(true, None);
        assert!(manual.contains("send links by hand"));
        assert!(manual.contains("junk or spam"));
        assert!(!manual.contains("accounts@richview.ai"));
    }

    #[test]
    fn rows_say_owner_visibility_and_date_and_escape_report_content() {
        let nav = test_nav("<Alex>", "a\"b");
        let rows = [Report {
            key: "ab123456/report",
            title: "<Revenue>",
            description: "A & B",
            owner: "Jana <N>",
            kind: "org",
            badge: "Acme",
            who: "Every member of Acme",
            size: Some(1_258_291),
            can_share: true,
            updated: "11 Nov 2026",
            version: "v1.0.0",
            url: "https://example.test/r/ab123456/report",
            thumb: None,
        }];
        let html = reports(&list(&nav, &rows, 1));
        assert!(html.contains("&lt;Alex&gt;"));
        assert!(html.contains("&lt;Revenue&gt;"));
        assert!(html.contains("A &amp; B"));
        assert!(html.contains("value=\"a&quot;b\""));
        assert!(
            html.contains("Every member of Acme")
                && html.contains("<td class=\"r-owner\">Jana &lt;N&gt;</td>")
                && html.contains("<td class=\"r-when\"><span>11 Nov 2026</span><span>v1.0.0 · 1.2 MiB</span></td>")
        );
        assert!(html.contains("href=\"/reports/ab123456/report\""));
        assert!(html.contains("href=\"/?show=shared\""));
        // On a phone the title opens the report and the other actions sit behind one menu.
        assert!(
            html.contains("<a class=\"r-open\" href=\"/r/ab123456/report\">&lt;Revenue&gt;</a>")
        );
        assert!(html.contains(
            "<details class=\"r-more\"><summary aria-label=\"More for &lt;Revenue&gt;\">"
        ));
        // Without a share card the row shows the report icon; with one, its small copy.
        assert!(html.contains("<span class=\"thumb thumb--none\">"));
        let card = [Report {
            thumb: Some("data:image/png;base64,iVBOR\"x"),
            ..rows[0]
        }];
        let html = reports(&list(&nav, &card, 1));
        assert!(html.contains(
            "<img class=\"thumb\" src=\"data:image/png;base64,iVBOR&quot;x\" alt=\"\" width=\"60\" height=\"32\">"
        ));
        assert!(!html.contains("thumb--none"));
    }

    fn page_for<'a>(
        nav: &'a Nav<'a>,
        people: &'a [Person<'a>],
        visibility: &'a str,
        sharing: bool,
        notice: &'a str,
    ) -> ReportPage<'a> {
        ReportPage {
            nav,
            key: "ab123456/report",
            title: "<Revenue>",
            description: "",
            owner: "You",
            organization: "A<b>cme",
            visibility,
            shared_with_you: !sharing,
            url: "https://example.test/r/ab123456/report",
            versions: &[],
            latest: "1.0.0",
            sharing: sharing.then_some(Sharing {
                owner_name: "Alex",
                owner_email: "alex@example.test",
                people,
            }),
            can_delete: sharing,
            notice,
        }
    }

    #[test]
    fn a_report_page_offers_the_two_other_visibilities_and_names_people_without_markup() {
        let nav = test_nav("Alex", "csrf");
        let people = [Person {
            email: "eve@example.test",
            name: None,
            role: "viewer",
            pending: true,
        }];
        let html = report(&page_for(&nav, &people, "org", true, ""));
        // The crumb ends with the report, escaped, after a link back to the list.
        assert!(html.contains(
            "<a href=\"/\">Reports</a><span aria-hidden=\"true\">/</span><strong title=\"&lt;Revenue&gt;\">&lt;Revenue&gt;</strong></p>"
        ));
        // The current setting has no button; the other two post the same field the server reads.
        assert!(html.contains("name=\"visibility\" value=\"private\""));
        assert!(html.contains("name=\"visibility\" value=\"public\""));
        assert!(!html.contains("name=\"visibility\" value=\"org\""));
        assert!(html.contains("action=\"/ui/share/ab123456/report\""));
        assert!(
            html.contains(
                "aria-current=\"true\"><span class=\"vis vis--org\"><svg class=\"ic\" aria-hidden=\"true\"><use href=\"#i-org\"/></svg>A&lt;b&gt;cme</span>"
            )
        );
        assert!(html.contains("Every member of A&lt;b&gt;cme can open it."));
        assert!(!html.contains("<b>cme"));
        assert!(html.contains("Invited, not opened yet"));
        assert!(html.contains("action=\"/ui/reports/ab123456/report/people/remove\""));
        assert!(html.contains("href=\"/reports/ab123456/report/delete\""));
        assert!(!html.contains("row-said"));
    }

    #[test]
    fn the_person_just_shared_with_hears_what_happens_next_unless_the_mail_failed() {
        let nav = test_nav("Alex", "csrf");
        let people = [Person {
            email: "eve@example.test",
            name: None,
            role: "viewer",
            pending: true,
        }];
        let sent = report(&page_for(
            &nav,
            &people,
            "private",
            true,
            "Shared with Eve@example.test. In these first days we send mails by hand, so it can take a few hours.",
        ));
        assert!(sent.contains("<li class=\"is-new\">"));
        assert!(sent.contains("We’re sending eve@example.test a mail with a link."));
        let failed = report(&page_for(
            &nav,
            &people,
            "private",
            true,
            "Shared with eve@example.test. The mail could not be sent right now; send it again later.",
        ));
        assert!(!failed.contains("is-new"));
    }

    #[test]
    fn a_reader_with_the_report_by_name_sees_shared_with_you_and_no_controls() {
        let nav = test_nav("Eve", "csrf");
        let html = report(&page_for(&nav, &[], "private", false, ""));
        assert!(html.contains("<span class=\"vis vis--you\"><svg class=\"ic\" aria-hidden=\"true\"><use href=\"#i-inbox\"/></svg>Shared with you</span>"));
        assert!(!html.contains(">Private<"));
        assert!(!html.contains("/ui/share/"));
        assert!(!html.contains("/delete"));
    }

    #[test]
    fn an_organization_carrying_the_owners_name_is_named_once() {
        let nav = test_nav("Eve", "csrf");
        let mut page = page_for(&nav, &[], "private", false, "");
        page.owner = "Alex Novak";
        page.organization = "Alex Novak";
        assert!(report(&page).contains("<span>By Alex Novak</span>"));
        page.organization = "Acme";
        assert!(report(&page).contains("<span>By Alex Novak · Acme</span>"));
        let mut own = page_for(&nav, &[], "private", true, "");
        own.organization = "alex ";
        assert!(report(&own).contains("<span>You</span>"));
    }

    #[test]
    fn the_report_page_notice_sits_under_its_head() {
        let nav = test_nav("Alex", "csrf");
        let html = report(&page_for(
            &nav,
            &[],
            "public",
            true,
            "Anyone with the link can open the report.",
        ));
        let head = html.find("report-head").unwrap();
        let notice = html.find("note note--ok").unwrap();
        assert!(head < notice);
    }

    fn member<'a>(person: &'a str, name: &'a str, role: &'a str, me: bool) -> Member<'a> {
        Member {
            person,
            name,
            email: "x@example.test",
            role,
            me,
            manageable: true,
            joined: "12 Nov 2026",
            reports: Some(2),
        }
    }

    fn members_page<'a>(
        nav: &'a Nav<'a>,
        rows: &'a [Member<'a>],
        invites: &'a [PendingInvite<'a>],
        manager: bool,
    ) -> MembersPage<'a> {
        MembersPage {
            nav,
            members: rows,
            invites,
            manager,
            owner: manager,
            roles: &["owner", "admin", "author", "reader"],
            notice: "",
            usage: None,
            heir: Some("Alex"),
            own_reports: None,
        }
    }

    #[test]
    fn members_page_shows_controls_only_to_managers() {
        let nav = test_nav("Alex", "csrf");
        let rows = [
            member("u-1", "Alex", "owner", true),
            member("u-2", "<Bo>", "author", false),
        ];
        let html = members(&members_page(&nav, &rows, &[], true));
        // A role is a menu of submit buttons posting the field the server reads; the current one
        // is pressed, and nothing needs a Save button.
        assert!(html.contains("action=\"/ui/orgs/ab123456/members/u-2/role\""));
        assert!(html.contains("name=\"role\" value=\"admin\" aria-pressed=\"false\""));
        assert!(html.contains("name=\"role\" value=\"author\" aria-pressed=\"true\""));
        assert!(!html.contains(">Save<"));
        // Removal waits behind a confirmation that says what happens to the reports.
        let confirm = html.find("Remove from Acme…").unwrap();
        let remove = html
            .find("action=\"/ui/orgs/ab123456/members/u-2/remove\"")
            .unwrap();
        assert!(confirm < remove);
        assert!(
            html.contains("&lt;Bo&gt; loses access to Acme at once. Their 2 reports pass to you.")
        );
        assert!(html.contains("&lt;Bo&gt;") && !html.contains("<Bo>"));
        // Inviting: the role as segments, the first and least one chosen.
        assert!(html.contains("action=\"/ui/orgs/ab123456/invitations\""));
        assert!(html.contains("id=\"invite-role-reader\" name=\"role\" value=\"reader\" checked"));
        assert!(html.contains("<th scope=\"col\" class=\"p-num\">Reports</th>"));
        // The same lifetime the invitation's mail states.
        assert!(html.contains("whose link works once within 24 hours"));
        // Someone who does not manage sees the roles as words and no controls.
        let reader = [
            Member {
                manageable: false,
                reports: None,
                ..member("u-1", "Alex", "owner", false)
            },
            Member {
                manageable: false,
                reports: None,
                ..member("u-3", "Cy", "reader", true)
            },
        ];
        let html = members(&members_page(&nav, &reader, &[], false));
        assert!(!html.contains("/invitations"));
        assert!(!html.contains("/role\""));
        assert!(!html.contains("p-num") && !html.contains("p-acts"));
        assert!(html.contains("<span class=\"p-role-text\">Owner</span>"));
        assert!(html.contains("<strong>Reader</strong> opens what is shared."));
    }

    #[test]
    fn open_invitations_follow_the_members_and_the_new_one_is_marked() {
        let nav = test_nav("Alex", "csrf");
        let rows = [member("u-1", "Alex", "owner", true)];
        let invites = [
            PendingInvite {
                id: "i-1",
                email: "eve@example.test",
                role: "reader",
                inviter: "Alex",
                expires: "24 Nov 2026",
            },
            PendingInvite {
                id: "i-2",
                email: "eve@example.tes",
                role: "author",
                inviter: "Alex",
                expires: "24 Nov 2026",
            },
        ];
        let mut page = members_page(&nav, &rows, &invites, true);
        page.notice = "Invited eve@example.test. If the mail doesn’t arrive within a few minutes, ask them to look in their junk folder.";
        let html = members(&page);
        assert!(html.contains("1 member of Acme · 2 invitations open"));
        assert!(html.contains("<th scope=\"rowgroup\" colspan=\"5\">Invited <span>2</span></th>"));
        // Only the address the notice names, not one that merely starts the same way.
        assert_eq!(html.matches("p-invite is-new").count(), 1);
        let marked = html.find("p-invite is-new").unwrap();
        assert!(html[marked..].starts_with(
            "p-invite is-new\"><td class=\"p-who\"><div><span class=\"avatar avatar--invite\""
        ));
        assert!(
            html[marked..]
                .find("<strong>")
                .map(|at| html[marked + at..].starts_with("<strong>eve@example.test</strong>"))
                == Some(true)
        );
        assert!(html.contains("action=\"/ui/orgs/ab123456/invitations/i-1/resend\""));
        assert!(html.contains("action=\"/ui/orgs/ab123456/invitations/i-2/cancel\""));
        assert!(!html.contains("Just you so far"));
        let alone = members(&members_page(&nav, &rows, &[], true));
        assert!(alone.contains("Just you so far."));
    }

    #[test]
    fn leaving_names_who_takes_the_reports_or_why_it_cannot_happen() {
        let nav = test_nav("Alex", "csrf");
        let rows = [member("u-1", "Alex", "author", true)];
        let mut page = members_page(&nav, &rows, &[], false);
        page.heir = Some("Jana <N>");
        page.own_reports = Some(3);
        let html = members(&page);
        assert!(html.contains("Your 3 reports pass to Jana &lt;N&gt;."));
        assert!(html.contains("action=\"/ui/orgs/ab123456/members/u-1/remove\""));
        assert!(html.contains("href=\"#organization\""));
        assert!(html.contains("id=\"organization\""));
        page.heir = None;
        let html = members(&page);
        assert!(html.contains("You’re its only owner."));
        assert!(!html.contains("/remove\""));
    }
}

#[cfg(test)]
mod auth_page_tests {
    use super::*;

    #[test]
    fn someone_without_an_organization_gets_no_members_or_tokens_tab() {
        let outside = Nav {
            name: "Dana",
            email: "dana@example.test",
            csrf: "csrf",
            org: None,
            role: None,
            orgs: &[],
        };
        let html = navigation(&outside, "reports");
        assert!(!html.contains("href=\"/members\""));
        assert!(!html.contains("href=\"/tokens\""));
        let html = navigation(&test_nav("Alex", "csrf"), "reports");
        assert!(html.contains("href=\"/members\"") && html.contains("href=\"/tokens\""));
    }

    #[test]
    fn sign_in_for_a_report_gives_the_reason_without_the_authors_pitch() {
        let pitch = "Open the reports your agent published";
        assert!(!sign_in("Sign in to open this report.", None).contains(pitch));
        assert!(!sign_in("Accepted. Sign in to open the report.", None).contains(pitch));
        assert!(sign_in("", None).contains(pitch));
        assert!(note("Deleted “Q3 revenue”.").contains("note--ok\" role=\"status"));
    }

    #[test]
    fn notices_have_semantic_kinds_and_escape_text() {
        assert!(note("Saved.").contains("note--ok\" role=\"status"));
        assert!(note("Shared with a@example.test.").contains("note--ok\" role=\"status"));
        assert!(note("Shared with a@example.test. The mail could not be sent right now; send it again later.").contains("note--error\" role=\"alert"));
        assert!(note("Sign in to open this report.").contains("note--info\" role=\"status"));
        // A change of who can open a report, a new organization and a share accepted are
        // successes; a prompt to sign in after accepting is information.
        for done in [
            "Anyone with the link can open the report.",
            "Every member of Acme can open the report.",
            "The report is private: only the owner and the people named here can open it.",
            "Acme is ready.",
            "This report was shared with you.",
        ] {
            assert!(note(done).contains("note--ok\" role=\"status"), "{done}");
        }
        for prompt in [
            "Accepted. Sign in to open the report.",
            "Accepted. Sign in to continue.",
            "Sign in first.",
        ] {
            assert!(
                note(prompt).contains("note--info\" role=\"status"),
                "{prompt}"
            );
        }
        let html = note("Wrong <password>");
        assert!(html.contains("note--error\" role=\"alert"));
        assert!(html.contains("Wrong &lt;password&gt;"));
        assert!(note("").is_empty());
    }
}
