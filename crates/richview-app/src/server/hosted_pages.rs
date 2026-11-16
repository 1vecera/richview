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
    pub csrf: &'a str,
    /// The current organization: its code and its name.
    pub org: Option<(&'a str, &'a str)>,
    /// Every organization the person belongs to: code and name.
    pub orgs: &'a [(String, String)],
}

fn navigation(nav: &Nav<'_>, current: &str) -> String {
    let link = |href: &str, label: &str, id: &str| {
        format!(
            "<a href=\"{href}\"{}>{label}</a>",
            if id == current {
                " aria-current=\"page\""
            } else {
                ""
            }
        )
    };
    let csrf = escape(nav.csrf);
    let organization = match (nav.org, nav.orgs.len()) {
        (Some((current, _)), count) if count > 1 => {
            let options: String = nav
                .orgs
                .iter()
                .map(|(id, name)| {
                    format!(
                        "<option value=\"{}\"{}>{}</option>",
                        escape(id),
                        if id == current { " selected" } else { "" },
                        escape(name)
                    )
                })
                .collect();
            format!(
                "<form class=\"org-switch\" method=\"post\" action=\"/ui/organization\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><select name=\"org\" aria-label=\"Organization\">{options}</select><button class=\"docs-button docs-button--secondary\" type=\"submit\">Switch</button></form>"
            )
        }
        // An organization of one carries its owner's name: say it once.
        (Some((_, name)), _) if name == nav.name => String::new(),
        (Some((_, name)), _) => format!("<span class=\"org-name\">{}</span>", escape(name)),
        (None, _) => String::new(),
    };
    // Members and CLI tokens belong to an organization; someone with only shared reports has none.
    let in_organization = nav.org.is_some() || !nav.orgs.is_empty();
    format!(
        "<div class=\"cowork-nav\"><nav aria-label=\"Account navigation\">{}{}{}{}</nav><div class=\"cowork-who\">{organization}<form method=\"post\" action=\"/ui/sign-out\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><span>{}</span><button class=\"docs-button docs-button--secondary\" type=\"submit\">Sign out</button></form></div></div>",
        link("/", "Reports", "reports"),
        if in_organization {
            link("/members", "Members", "members")
        } else {
            String::new()
        },
        if in_organization {
            link("/tokens", "CLI tokens", "tokens")
        } else {
            String::new()
        },
        link("/account", "Account", "account"),
        escape(nav.name)
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
    let usage = list.usage.map_or_else(String::new, |u| {
        let used = u.reports;
        let report_limit = u.report_limit;
        let storage = escape(&mebibytes(u.bytes));
        let limit = escape(&mebibytes(u.byte_limit));
        let percent = if u.byte_limit == 0 { 0 } else { ((u.bytes as u128 * 100 / u.byte_limit as u128).min(100)) as u64 };
        format!(r#"<p class="usage"><span>Reports <strong>{used} of {report_limit}</strong></span><progress max="{report_limit}" value="{used}" aria-label="Reports used">{used} of {report_limit}</progress><span>Storage <strong>{storage} of {limit}</strong></span><progress max="100" value="{percent}" aria-label="Storage used">{storage} of {limit}</progress></p>"#)
    });
    let mut body = format!(
        r#"{}<div class="page-head"><div><h1>Reports</h1>{lede}</div>{usage}</div>{}"#,
        navigation(nav, "reports"),
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
                body.push_str(&format!(r#"<ol class="first"><li><h2>Install RichView</h2><p>The command-line tool your agent uses to write and publish reports.</p><a class="docs-button docs-button--secondary" href="https://richview.ai/docs/install">Install RichView</a></li><li><h2>Create a CLI token</h2><p>It lets the tool publish as you. You see it once, with the two lines that set it.</p><a class="docs-button" href="/tokens">Create a CLI token</a></li><li class="wide"><h2>Publish</h2><p>Ask your agent to publish a report, or run:</p>{}</li><li class="wide plain"><p><a href="/members">Invite your colleagues</a>, then share a report with them from its page.</p></li></ol>"#, shell(&command)));
            }
            Some(_) => {}
        }
        return body;
    }
    // Someone who only has reports shared with them can still start an organization of their own.
    let start = match nav.org {
        None if list.show == "all" => format!(
            "<section><h2>Publish your own</h2>{}</section>",
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
    body.push_str(&format!(
        "<div class=\"report-tools\"><nav class=\"report-filter\" aria-label=\"Show\">{}{}{}{}</nav><form class=\"report-search\" method=\"get\" action=\"/\" role=\"search\"><input type=\"hidden\" name=\"show\" value=\"{}\"><input type=\"search\" name=\"q\" value=\"{query}\" placeholder=\"Search by title\" aria-label=\"Search reports by title\"><button class=\"docs-button docs-button--secondary\" type=\"submit\">Search</button></form></div>",
        filter("all", "All"),
        nav.org
            .map_or_else(String::new, |_| filter("mine", "Mine")),
        filter("shared", "Shared with me"),
        nav.org.map_or_else(String::new, |(_, name)| filter("org", name)),
        escape(list.show)
    ));
    if list.rows.is_empty() {
        body.push_str(match (list.query.is_empty(), list.show) {
            (false, _) => "<p>No report title matches your search.</p>",
            (true, "mine") => "<p>You haven’t published a report here yet.</p>",
            (true, "shared") => "<p>Nobody has shared a report with you by name yet.</p>",
            _ => "<p>Nothing here yet.</p>",
        });
        body.push_str(&start);
        return body;
    }
    body.push_str(r#"<ul class="rows report-rows">"#);
    for row in list.rows {
        let key = escape(row.key);
        let title = escape(row.title);
        let (action, label) = if row.can_share {
            ("Share", "Sharing and versions")
        } else {
            ("Details", "Details and versions")
        };
        body.push_str(&format!(r#"<li><div class="main"><h2><a href="/reports/{key}">{title}</a></h2><p class="about">{}</p></div><p class="open-to"><span class="vis vis--{}">{}</span><span>{}</span></p><p class="when"><span><strong>{}</strong> · {}</span><span>{}{}</span></p><div class="acts"><a class="act" href="/r/{key}">Open</a><figure class="docs-code copy-link"><code hidden>{}</code><button class="act" type="button" data-copy-code aria-label="Copy link to {title}">Copy link</button></figure><a class="act" href="/reports/{key}" aria-label="{label} of {title}">{action}</a></div></li>"#, escape(row.description), escape(row.kind), escape(row.badge), escape(row.who), escape(row.updated), escape(row.version), escape(&row.size.map_or_else(|| "Size unavailable".to_owned(), mebibytes)), if row.can_share { format!(" · {}", escape(row.owner)) } else { String::new() }, escape(row.url)));
    }
    body.push_str("</ul>");
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
    format!("<span class=\"vis vis--{kind}\">{label}</span>")
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
        navigation(page.nav, "reports"),
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
}

pub(super) fn role_label(role: &str) -> &'static str {
    match role {
        "owner" => "Owner",
        "admin" => "Admin",
        "author" => "Author",
        _ => "Reader",
    }
}

fn role_detail(role: &str) -> &'static str {
    match role {
        "owner" => "Owner: everything, including other owners",
        "admin" => "Admin: manages members and every report",
        "author" => "Author: publishes reports",
        _ => "Reader: opens what is shared",
    }
}

/// The people of the current organization and its open invitations.
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
    let mut body = format!(
        "{}<div class=\"page-head\"><div><h1>{}</h1><p class=\"lede\">Organization · {} {}</p></div></div>{}<section><h2>Members</h2><ul class=\"rows rows--3\">",
        navigation(nav, "members"),
        escape(name),
        page.members.len(),
        if page.members.len() == 1 {
            "member"
        } else {
            "members"
        },
        note(page.notice)
    );
    for member in page.members {
        let person = escape(member.person);
        let member_name = escape(member.name);
        let role = if member.manageable {
            let options: String = page
                .roles
                .iter()
                .map(|role| {
                    format!(
                        "<option value=\"{role}\"{}>{}</option>",
                        if *role == member.role {
                            " selected"
                        } else {
                            ""
                        },
                        role_label(role)
                    )
                })
                .collect();
            format!(
                "<form class=\"role-form\" method=\"post\" action=\"/ui/orgs/{org}/members/{person}/role\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><select name=\"role\" aria-label=\"Role of {member_name}\">{options}</select><button class=\"act\" type=\"submit\">Save</button></form>"
            )
        } else {
            role_label(member.role).to_owned()
        };
        let action = if member.me {
            format!(
                "<form method=\"post\" action=\"/ui/orgs/{org}/members/{person}/remove\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"act--danger\" type=\"submit\">Leave</button></form>"
            )
        } else if member.manageable {
            format!(
                "<form method=\"post\" action=\"/ui/orgs/{org}/members/{person}/remove\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"act--danger\" type=\"submit\" aria-label=\"Remove {member_name}\">Remove</button></form>"
            )
        } else {
            String::new()
        };
        body.push_str(&format!("<li><div class=\"who\"><strong>{member_name}</strong>{}<span class=\"quiet\">{}</span></div><div class=\"cells\">{role}</div><div class=\"acts\">{action}</div></li>", if member.me { " <span class=\"tag\">You</span>" } else { "" }, escape(member.email)));
    }
    body.push_str("</ul><p class=\"roles-key\"><strong>Reader</strong> opens what is shared. <strong>Author</strong> publishes reports. <strong>Admin</strong> manages members and every report. <strong>Owner</strong> does everything, including other owners.</p></section>");
    if !page.manager {
        return body;
    }
    let options: String = page
        .roles
        .iter()
        .rev()
        .map(|role| format!("<option value=\"{role}\">{}</option>", role_detail(role)))
        .collect();
    body.push_str(&format!("<section><h2>Invite a colleague</h2><form class=\"share-form\" method=\"post\" action=\"/ui/orgs/{org}/invitations\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><div class=\"field\"><label for=\"invite-email\">Email</label><input id=\"invite-email\" type=\"email\" name=\"email\" required autocomplete=\"off\"></div><div class=\"field\"><label for=\"invite-role\">Role</label><select id=\"invite-role\" name=\"role\">{options}</select></div><button type=\"submit\">Send invitation</button><p class=\"hint\">They get a mail with a link that works for one hour. The invitation stays open for a week, and you can send it again.</p></form>"));
    if !page.invites.is_empty() {
        body.push_str("<h3>Open invitations</h3><ul class=\"rows rows--3\">");
        for invite in page.invites {
            let id = escape(invite.id);
            let email = escape(invite.email);
            body.push_str(&format!("<li><div class=\"who\"><strong>{email}</strong><span class=\"quiet\">Invited by {}</span></div><div class=\"cells\">{} <span class=\"tag\">Open until {}</span></div><div class=\"acts\"><form method=\"post\" action=\"/ui/orgs/{org}/invitations/{id}/resend\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"act\" type=\"submit\" aria-label=\"Send the invitation to {email} again\">Send again</button></form><form method=\"post\" action=\"/ui/orgs/{org}/invitations/{id}/cancel\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><button class=\"act--danger\" type=\"submit\" aria-label=\"Cancel the invitation to {email}\">Cancel</button></form></div></li>", escape(invite.inviter), role_label(invite.role), escape(invite.expires)));
        }
        body.push_str("</ul>");
    }
    body.push_str("</section>");
    if page.owner {
        body.push_str(&format!("<section><h2>Organization name</h2><form class=\"share-form share-form--one\" method=\"post\" action=\"/ui/orgs/{org}/name\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><div class=\"field\"><label for=\"org-name\">Organization name</label><input id=\"org-name\" name=\"name\" required maxlength=\"80\" value=\"{}\"></div><button class=\"docs-button docs-button--secondary\" type=\"submit\">Rename</button></form></section>", escape(name)));
    }
    body
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

pub(super) fn accept_invite(page: &InvitePage<'_>) -> String {
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
            r#"<div class="cowork-auth"><h1>{title}</h1><p class="lede">{sentence}</p><form class="panel" method="post" action="/ui/link/{token}"><button type="submit">Accept and sign in</button><p class="fine">You already have a RichView account for this address. Accept, then sign in as usual.</p></form></div>"#
        );
    }
    let hint = if hint {
        format!(r#"<p class="hint">Set up your account to {action}.</p>"#)
    } else {
        String::new()
    };
    format!(
        r#"<div class="cowork-auth"><h1>{title}</h1><p class="lede">{sentence}</p>{hint}{notice}<form class="panel" method="post" action="/ui/link/{token}"><div class="field"><label for="name">Name</label><input id="name" name="name" required autocomplete="name"{name_value}></div><div class="field"><label for="password">Password</label><input id="password" type="password" name="password" required minlength="12" autocomplete="new-password" aria-describedby="password-hint"><p class="hint" id="password-hint">At least 12 characters.</p></div><div class="field"><label for="password-confirm">Confirm password</label><input id="password-confirm" type="password" name="password_confirm" required minlength="12" autocomplete="new-password"></div><button type="submit">{button}</button><p class="fine">By creating an account, you agree to the <a href="https://richview.ai/terms">Terms</a> and <a href="https://richview.ai/privacy">Privacy policy</a>.</p></form></div>"#
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
pub(super) fn delete_confirmation(key: &str, title: &str, csrf: &str) -> String {
    let key = escape(key);
    format!(
        "<div class=\"cowork-auth cowork-confirm\"><h1>Delete report?</h1><p class=\"lede\">Delete <strong>{}</strong> and all its versions? This cannot be undone.</p><ul class=\"destroys\"><li>Every version is removed.</li><li>Everyone it is shared with loses access, and its link stops working.</li><li>The address is never used again.</li></ul><form class=\"cowork-actions\" method=\"post\" action=\"/ui/reports/{key}/delete\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"docs-button docs-button--danger-solid\" type=\"submit\">Delete report</button><a class=\"docs-button docs-button--secondary\" href=\"/reports/{key}\">Keep it</a></form></div>",
        escape(title),
        escape(csrf)
    )
}

#[cfg(test)]
fn test_nav<'a>(name: &'a str, csrf: &'a str) -> Nav<'a> {
    Nav {
        name,
        csrf,
        org: Some(("ab123456", "Acme")),
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
    fn empty_reports_put_install_before_token_publish_and_inviting() {
        let nav = test_nav("Alex", "csrf");
        let html = reports(&list(&nav, &[], 0));
        let install = html.find("https://richview.ai/docs/install").unwrap();
        let token = html.find("Create a CLI token").unwrap();
        let publish = html.find("publish a report").unwrap();
        let invite = html.find("Invite your colleagues").unwrap();
        assert!(install < token && token < publish && publish < invite);
    }

    #[test]
    fn empty_states_say_what_to_do_next_for_each_person() {
        let nav = test_nav("Alex", "csrf");
        let mut reader = list(&nav, &[], 0);
        reader.can_publish = false;
        let html = reports(&reader);
        assert!(html.contains("Reports that colleagues share with Acme or with you appear here."));
        assert!(!html.contains("Create a CLI token"));
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
        }];
        let html = reports(&list(&nav, &rows, 1));
        assert!(html.contains("&lt;Alex&gt;"));
        assert!(html.contains("&lt;Revenue&gt;"));
        assert!(html.contains("A &amp; B"));
        assert!(html.contains("value=\"a&quot;b\""));
        assert!(
            html.contains("Every member of Acme")
                && html.contains("1.2 MiB · Jana &lt;N&gt;")
                && html.contains("11 Nov 2026</strong> · v1.0.0")
        );
        assert!(html.contains("href=\"/reports/ab123456/report\""));
        assert!(html.contains("href=\"/?show=shared\""));
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
        // The current setting has no button; the other two post the same field the server reads.
        assert!(html.contains("name=\"visibility\" value=\"private\""));
        assert!(html.contains("name=\"visibility\" value=\"public\""));
        assert!(!html.contains("name=\"visibility\" value=\"org\""));
        assert!(html.contains("action=\"/ui/share/ab123456/report\""));
        assert!(
            html.contains(
                "aria-current=\"true\"><span class=\"vis vis--org\">A&lt;b&gt;cme</span>"
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
        assert!(html.contains("<span class=\"vis vis--you\">Shared with you</span>"));
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

    #[test]
    fn members_page_shows_controls_only_to_managers() {
        let nav = test_nav("Alex", "csrf");
        let rows = [
            Member {
                person: "u-1",
                name: "Alex",
                email: "alex@example.test",
                role: "owner",
                me: true,
                manageable: true,
            },
            Member {
                person: "u-2",
                name: "<Bo>",
                email: "bo@example.test",
                role: "author",
                me: false,
                manageable: true,
            },
        ];
        let page = |manager: bool| {
            members(&MembersPage {
                nav: &nav,
                members: &rows,
                invites: &[],
                manager,
                owner: manager,
                roles: &["owner", "admin", "author", "reader"],
                notice: "",
            })
        };
        let html = page(true);
        assert!(html.contains("<ul class=\"rows rows--3\">"));
        assert!(html.contains("action=\"/ui/orgs/ab123456/members/u-1/role\""));
        assert!(html.contains("action=\"/ui/orgs/ab123456/invitations\""));
        assert!(html.contains("action=\"/ui/orgs/ab123456/members/u-2/remove\""));
        assert!(html.contains("&lt;Bo&gt;"));
        assert!(!html.contains("<Bo>"));
        assert!(!page(false).contains("/invitations"));
    }
}

#[cfg(test)]
mod auth_page_tests {
    use super::*;

    #[test]
    fn someone_without_an_organization_gets_no_members_or_tokens_tab() {
        let outside = Nav {
            name: "Dana",
            csrf: "csrf",
            org: None,
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
