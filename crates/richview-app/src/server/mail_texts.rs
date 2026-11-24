//! Messages for hosted accounts, invitations and shared reports, each as plain text and as HTML.
//! Delivery is handled separately. Each part carries exactly one link, and its subject names
//! RichView and the action. The HTML part is one column at most 560px wide with inline styles
//! only, no images and no web fonts, and its one link is the button.

const FOOTER: &str = "RichView · operated by Daniel Vecera\n";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// What an HTML mail says, as plain text: [`html`] escapes every part.
struct Card<'a> {
    heading: &'a str,
    eyebrow: &'a str,
    title: &'a str,
    about: &'a str,
    button: &'a str,
    link: &'a str,
    after: &'a str,
    ignore: &'a str,
}

/// The HTML part: the heading, then a card with the title, a line about it and the one button,
/// then what happens after the link expires and when to ignore the mail.
fn html(card: &Card<'_>) -> String {
    const TEXT: &str = "color:#5e5750;font-size:13.5px;line-height:1.55";
    let eyebrow = if card.eyebrow.is_empty() {
        String::new()
    } else {
        format!(
            "<p style=\"margin:0 0 6px;color:#706960;font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase\">{}</p>",
            escape(card.eyebrow)
        )
    };
    let (gap, about) = if card.about.is_empty() {
        ("22px", String::new())
    } else {
        (
            "8px",
            format!(
                "<p style=\"margin:0 0 22px;color:#5e5750;font-size:15px;line-height:1.5\">{}</p>",
                escape(card.about)
            ),
        )
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"light\"></head>\
<body style=\"margin:0;padding:0;background:#faf9f6\">\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" style=\"background:#faf9f6\"><tr><td align=\"center\" style=\"padding:32px 24px 28px\">\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" style=\"max-width:560px;font-family:-apple-system,'Segoe UI',system-ui,Roboto,Helvetica,Arial,sans-serif;color:#1f1c19;text-align:left\">\
<tr><td style=\"padding:0 0 28px;font-size:17px;font-weight:700;letter-spacing:-.01em;color:#1a6dac\">RichView</td></tr>\
<tr><td style=\"padding:0 0 18px;font-size:22px;line-height:1.3;font-weight:700;letter-spacing:-.01em\">{heading}</td></tr>\
<tr><td style=\"padding:22px 22px 24px;border:1px solid #ddd5c9;border-radius:12px;background:#fffefb\">{eyebrow}\
<p style=\"margin:0 0 {gap};font-size:21px;line-height:1.25;font-weight:700;letter-spacing:-.01em\">{title}</p>{about}\
<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\"><tr><td style=\"border-radius:999px;background:#1a6dac\">\
<a href=\"{link}\" style=\"display:inline-block;padding:12px 24px;border-radius:999px;color:#ffffff;font-size:15px;font-weight:600;text-decoration:none\">{button}</a>\
</td></tr></table></td></tr>\
<tr><td style=\"padding:22px 0 0;{TEXT}\">{after}</td></tr>\
<tr><td style=\"padding:10px 0 0;{TEXT}\">{ignore}</td></tr>\
<tr><td style=\"padding:28px 0 0\"><p style=\"margin:0;padding-top:16px;border-top:1px solid #ddd5c9;color:#706960;font-size:12px\">RichView · operated by Daniel Vecera</p></td></tr>\
</table></td></tr></table></body></html>",
        heading = escape(card.heading),
        title = escape(card.title),
        link = escape(card.link),
        button = escape(card.button),
        after = escape(card.after),
        ignore = escape(card.ignore),
    )
}

/// Invite a person to finish creating their account with a single-use link.
pub(crate) fn signup(link: &str) -> String {
    format!(
        "Finish creating your RichView account\n\nOpen this link to set your name and password:\n{link}\n\nThe link expires in 24 hours and can be used once. If you didn't request an account, you can ignore this email.\n\n{FOOTER}"
    )
}

/// Help an existing account holder choose a new password.
pub(crate) fn reset(link: &str) -> String {
    format!(
        "Reset your RichView password\n\nOpen this link to choose a new password:\n{link}\n\nThe link expires in one hour and can be used once. If you didn't request this, you can ignore this email.\n\n{FOOTER}"
    )
}

/// Help an existing account holder who asked to sign up recover access instead.
pub(crate) fn already_registered(link: &str) -> String {
    format!(
        "Your RichView account\n\nAn account already exists for this email address. Open this link to choose a new password:\n{link}\n\nThe link expires in one hour and can be used once. If you didn't request this, you can ignore this email.\n\n{FOOTER}"
    )
}

/// The HTML part of [`signup`].
pub(crate) fn signup_html(link: &str) -> String {
    html(&Card {
        heading: "Finish creating your account",
        eyebrow: "",
        title: "Set your name and password",
        about: "One step left: open the link to choose them.",
        button: "Finish sign-up",
        link,
        after: "The button works once, within 24 hours.",
        ignore: "If you didn’t ask for an account, you can ignore this email.",
    })
}

/// The HTML part of [`reset`].
pub(crate) fn reset_html(link: &str) -> String {
    html(&Card {
        heading: "Reset your password",
        eyebrow: "",
        title: "Choose a new password",
        about: "Open the link to set it. Your old password works until you do.",
        button: "Choose a new password",
        link,
        after: "The button works once, within an hour.",
        ignore: "If you didn’t ask for this, you can ignore this email.",
    })
}

/// The HTML part of [`already_registered`].
pub(crate) fn already_registered_html(link: &str) -> String {
    html(&Card {
        heading: "You already have an account",
        eyebrow: "",
        title: "Choose a new password",
        about: "An account already exists for this email address. Open the link to choose a new password and sign in.",
        button: "Choose a new password",
        link,
        after: "The button works once, within an hour.",
        ignore: "If you didn’t ask for this, you can ignore this email.",
    })
}

/// The subject of the notice that a provider's account was joined to an account.
pub(crate) fn connected_subject(provider: &str) -> String {
    format!("{provider} was connected to your RichView account")
}

/// Tell the holder of an account that a provider's account now signs in to it. `when` is the
/// time, as a person reads it; `link` is the Account page, where it can be disconnected.
pub(crate) fn connected(provider: &str, when: &str, link: &str) -> String {
    format!(
        "{provider} was connected to your RichView account\n\nAt {when}, a {provider} account was connected to your RichView account. You can now sign in to RichView with it.\n\nIf this was you, there is nothing to do. If it wasn't, disconnect it on your Account page and choose a new password:\n{link}\n\n{FOOTER}"
    )
}

/// The HTML part of [`connected`].
pub(crate) fn connected_html(provider: &str, when: &str, link: &str) -> String {
    html(&Card {
        heading: "A sign-in was connected",
        eyebrow: "",
        title: &format!("{provider} was connected"),
        about: &format!("At {when}. You can now sign in to RichView with it."),
        button: "Review your account",
        link,
        after: "If it wasn’t you, disconnect it on the Account page and choose a new password.",
        ignore: "If it was you, there is nothing to do.",
    })
}

/// A name someone else typed, as it may appear in a mail: one line, bounded, and never
/// something a mail client would turn into a second link.
pub(crate) fn plain(text: &str, fallback: &str) -> String {
    match one_line(text) {
        Some(line) => line.chars().take(80).collect(),
        None => fallback.to_owned(),
    }
}

/// A report's description as a mail shows it: what [`plain`] keeps, up to 200 characters, cut
/// at a word with an ellipsis; empty when [`plain`] would drop it.
pub(crate) fn about(text: &str) -> String {
    const LIMIT: usize = 200;
    let Some(line) = one_line(text) else {
        return String::new();
    };
    if line.chars().count() <= LIMIT {
        return line;
    }
    let cut: String = line.chars().take(LIMIT).collect();
    let words = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", words.trim_end_matches([',', ';', ':', '.', ' ']))
}

/// The text on one line, or `None` when it is empty or a mail client could make a link of it.
fn one_line(text: &str) -> Option<String> {
    let line = text
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if line.is_empty() || line.contains("://") || line.contains('@') || names_a_host(&line) {
        return None;
    }
    Some(line)
}

/// Whether a mail client could read part of the text as a host name: a dot with a letter,
/// digit or hyphen before it and two or more letters after it, as in `login.evil.com`,
/// `evil.com/path` or `www.evil.com`. `Acme Inc.` and `v1.2` have no such dot.
fn names_a_host(text: &str) -> bool {
    // Full-width and ideographic full stops are dots to a resolver.
    let chars: Vec<char> = text
        .chars()
        .map(|c| {
            if matches!(c, '。' | '．' | '｡') {
                '.'
            } else {
                c
            }
        })
        .collect();
    chars.iter().enumerate().any(|(at, c)| {
        *c == '.'
            && at > 0
            && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '-')
            && chars
                .get(at + 1..at + 3)
                .is_some_and(|next| next.iter().all(|c| c.is_alphabetic()))
    })
}

/// Who sent an invitation: their name with the address the recipient can check it against.
pub(crate) struct Sender<'a> {
    pub name: &'a str,
    pub email: &'a str,
}

/// How a membership role reads in a sentence.
pub(crate) fn role_phrase(role: crate::access::OrgRole) -> &'static str {
    use crate::access::OrgRole;
    match role {
        OrgRole::Owner => "an owner",
        OrgRole::Admin => "an admin",
        OrgRole::Author => "an author, who can publish reports",
        OrgRole::Reader => "a reader",
    }
}

pub(crate) fn invite_subject(sender: &Sender<'_>, organization: &str) -> String {
    format!(
        "{} invited you to {} on RichView",
        plain(sender.name, "A colleague"),
        plain(organization, "their organization")
    )
}

/// Invite an address to an organization. The same text goes to an address with an account and
/// to one without.
pub(crate) fn invite(
    sender: &Sender<'_>,
    organization: &str,
    role: crate::access::OrgRole,
    link: &str,
) -> String {
    let name = plain(sender.name, "A colleague");
    let organization = plain(organization, "their organization");
    format!(
        "Join {organization} on RichView\n\n{name} ({}) invited you to join {organization} as {}.\n\nAccept the invitation:\n{link}\n\nThe link expires in 24 hours and can be used once. After that, sign in if you already have an account: the invitation is on your reports page. Otherwise ask {name} to send it again.\n\nIf you don't know {name}, you can ignore this email.\n\n{FOOTER}",
        sender.email,
        role_phrase(role)
    )
}

/// The HTML part of [`invite`]: the organization is the card's title.
pub(crate) fn invite_html(
    sender: &Sender<'_>,
    organization: &str,
    role: crate::access::OrgRole,
    link: &str,
) -> String {
    let name = plain(sender.name, "A colleague");
    html(&Card {
        heading: &format!("{name} invited you to join"),
        eyebrow: "Invitation",
        title: &plain(organization, "Their organization"),
        about: &format!("Join as {}.", role_phrase(role)),
        button: "Accept the invitation",
        link,
        after: &format!(
            "{name} ({}) sent this invitation. The button works once, within 24 hours. After that, sign in to RichView: the invitation is on your reports page. Otherwise ask {name} to send it again.",
            sender.email
        ),
        ignore: &format!("If you don’t know {name}, you can ignore this email."),
    })
}

/// The subject of a shared report's mail names the sender and the report.
pub(crate) fn share_subject(sender: &Sender<'_>, title: &str) -> String {
    let name = plain(sender.name, "A colleague");
    match plain(title, "") {
        title if title.is_empty() => format!("{name} shared a RichView report with you"),
        title => format!("{name} shared “{title}” with you"),
    }
}

/// A report shared with an address: its title, its description when it has one, and whether
/// the recipient may publish versions too.
pub(crate) struct Shared<'a> {
    pub organization: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub editor: bool,
}

impl Shared<'_> {
    /// The organization a report is from, unless it carries the sender's own name, as one made at
    /// sign-up does: then the mail names the person once.
    fn from(&self, sender: &Sender<'_>) -> Option<String> {
        let organization = plain(self.organization, "");
        let own = sender.name.trim().to_lowercase() == self.organization.trim().to_lowercase();
        (!own && !organization.is_empty()).then_some(organization)
    }

    fn can(&self) -> &'static str {
        if self.editor {
            "read it and publish new versions"
        } else {
            "read it"
        }
    }
}

/// Tell an address that a report was shared with it. The same text goes to an address with an
/// account and to one without.
pub(crate) fn share(sender: &Sender<'_>, report: &Shared<'_>, link: &str) -> String {
    let name = plain(sender.name, "A colleague");
    let about = match about(report.description) {
        about if about.is_empty() => String::new(),
        about => format!("{about}\n\n"),
    };
    format!(
        "{name} shared a report with you\n\n{name} ({}) shared “{}”{} with you, so you can {}.\n\n{about}Open the report:\n{link}\n\nThe link expires in 24 hours and can be used once. After that, sign in if you already have an account: the report is in your list under “Shared with me”. Otherwise ask {name} to share the report again.\n\nIf you don't know {name}, you can ignore this email.\n\n{FOOTER}",
        sender.email,
        plain(report.title, "a report"),
        report
            .from(sender)
            .map_or_else(String::new, |organization| format!(" from {organization}")),
        report.can()
    )
}

/// The HTML part of [`share`]: the report's title and description are the card.
pub(crate) fn share_html(sender: &Sender<'_>, report: &Shared<'_>, link: &str) -> String {
    let name = plain(sender.name, "A colleague");
    html(&Card {
        heading: &format!("{name} shared a report with you"),
        eyebrow: &report.from(sender).unwrap_or_default(),
        title: &plain(report.title, "A RichView report"),
        about: &about(report.description),
        button: "Open the report",
        link,
        after: &format!(
            "{name} ({}) shared it with you so you can {}. The button works once, within 24 hours. After that, sign in to RichView: the report is under Shared with me. Otherwise ask {name} to share it again.",
            sender.email,
            report.can()
        ),
        ignore: &format!("If you don’t know {name}, you can ignore this email."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::OrgRole;

    fn links(message: &str) -> usize {
        message.matches("https://").count() + message.matches("http://").count()
    }

    const LINK: &str = "https://example.test/link/token";

    fn jana() -> Sender<'static> {
        Sender {
            name: "Jana Novak",
            email: "jana@example.test",
        }
    }

    fn report<'a>(organization: &'a str, title: &'a str, description: &'a str) -> Shared<'a> {
        Shared {
            organization,
            title,
            description,
            editor: false,
        }
    }

    #[test]
    fn messages_include_the_link_and_expiry() {
        for (message, expiry) in [
            (signup(LINK), "expires in 24 hours"),
            (reset(LINK), "expires in one hour"),
            (already_registered(LINK), "expires in one hour"),
        ] {
            assert!(message.contains(LINK));
            assert!(message.contains(expiry), "{message}");
            assert!(message.contains("used once"));
            assert!(message.contains("ignore this email"));
            assert!(message.ends_with("RichView · operated by Daniel Vecera\n"));
            assert_eq!(links(&message), 1);
        }
    }

    #[test]
    fn each_html_part_has_its_one_link_as_the_button() {
        let organization = report("Acme Analytics", "Q3 revenue", "Revenue by region.");
        // A password reset keeps one hour; the mails a person may read much later last a day.
        for (html, expiry) in [
            (signup_html(LINK), "within 24 hours"),
            (reset_html(LINK), "within an hour"),
            (already_registered_html(LINK), "within an hour"),
            (
                invite_html(&jana(), "Acme Analytics", OrgRole::Reader, LINK),
                "within 24 hours",
            ),
            (share_html(&jana(), &organization, LINK), "within 24 hours"),
        ] {
            assert!(html.starts_with("<!doctype html>"));
            assert!(html.ends_with("</html>"));
            assert_eq!(links(&html), 1);
            assert!(html.contains(&format!("<a href=\"{LINK}\"")));
            assert!(html.contains(expiry), "{html}");
            assert!(html.contains("ignore this email"));
            assert!(html.contains("max-width:560px"));
            assert!(
                !html.contains("<img") && !html.contains("<style") && !html.contains("<script")
            );
        }
    }

    #[test]
    fn invitations_say_who_invited_you_to_what_with_one_link() {
        let sender = jana();
        let invite = invite(&sender, "Acme Analytics", OrgRole::Author, LINK);
        assert!(invite.contains("Jana Novak (jana@example.test) invited you to join Acme Analytics as an author, who can publish reports.\n\nAccept the invitation:\n"));
        let share = share(&sender, &report("Acme Analytics", "Q3 revenue", ""), LINK);
        assert!(share.contains(
            "Jana Novak (jana@example.test) shared “Q3 revenue” from Acme Analytics with you, so you can read it.\n\nOpen the report:\n"
        ));
        assert!(share.contains("under “Shared with me”"));
        for message in [invite, share] {
            assert!(message.contains("expires in 24 hours"));
            assert_eq!(links(&message), 1);
        }
        assert_eq!(
            invite_subject(&sender, "Acme Analytics"),
            "Jana Novak invited you to Acme Analytics on RichView"
        );
        assert_eq!(
            share_subject(&sender, "Q3 revenue"),
            "Jana Novak shared “Q3 revenue” with you"
        );
        assert_eq!(
            share_subject(&sender, "  "),
            "Jana Novak shared a RichView report with you"
        );
        let html = invite_html(&sender, "Acme Analytics", OrgRole::Author, LINK);
        assert!(html.contains(">Acme Analytics</p>"));
        assert!(html.contains("Join as an author, who can publish reports."));
    }

    #[test]
    fn a_shared_report_mail_shows_its_title_and_a_bounded_description() {
        let short = report("Acme", "Churn", "Where we lost subscribers.");
        let text = share(&jana(), &short, LINK);
        assert!(
            text.contains("so you can read it.\n\nWhere we lost subscribers.\n\nOpen the report:")
        );
        let html = share_html(&jana(), &short, LINK);
        assert!(html.contains(">Churn</p>"));
        assert!(html.contains(">Where we lost subscribers.</p>"));
        assert!(html.contains(">Acme</p>"));

        let long = "word ".repeat(60);
        let cut = about(&long);
        assert!(cut.ends_with("word…"), "{cut}");
        assert!(cut.chars().count() <= 201);

        // A description a mail client could link is left out, not replaced.
        let linked = report("Acme", "Churn", "Details at evil.example/churn");
        assert!(!share(&jana(), &linked, LINK).contains("evil"));
        assert!(share(&jana(), &linked, LINK).contains("read it.\n\nOpen the report:"));
        assert!(!share_html(&jana(), &linked, LINK).contains("evil"));
    }

    #[test]
    fn an_organization_carrying_the_senders_name_is_not_named_again() {
        let own = report("Jana Novak", "Churn", "");
        let text = share(&jana(), &own, LINK);
        assert!(
            text.contains("shared “Churn” with you, so you can read it."),
            "{text}"
        );
        assert_eq!(text.matches("Jana Novak (").count(), 1);
        let html = share_html(&jana(), &own, LINK);
        assert!(!html.contains(">Jana Novak</p>"));
        assert!(!html.contains("text-transform:uppercase"));
        let team = report("Acme", "Churn", "");
        assert!(share(&jana(), &team, LINK).contains("shared “Churn” from Acme with you"));
    }

    #[test]
    fn typed_names_cannot_add_a_line_markup_or_a_second_link() {
        let sender = Sender {
            name: "Eve\r\nBcc: victim@example.test",
            email: "eve@example.test",
        };
        assert!(!invite_subject(&sender, "Acme").contains(['\r', '\n']));
        assert!(!share_subject(&sender, "Q3\r\nBcc: x").contains(['\r', '\n']));
        let shared = Shared {
            organization: "Visit https://evil.test now",
            title: "www.evil.test",
            description: "<img src=x onerror=alert(1)>",
            editor: true,
        };
        for message in [
            share(&sender, &shared, LINK),
            share_html(&sender, &shared, LINK),
        ] {
            assert_eq!(links(&message), 1);
            assert!(!message.contains("evil.test"));
        }
        assert!(share_html(&sender, &shared, LINK).contains("&lt;img src=x"));
        assert!(!share_html(&sender, &shared, LINK).contains("<img"));
        let marked = report("<b>Acme</b>", "<script>x</script>", "a < b & c");
        let html = share_html(&jana(), &marked, LINK);
        assert!(html.contains("&lt;b&gt;Acme&lt;/b&gt;"));
        assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
        assert!(html.contains("a &lt; b &amp; c"));
        assert!(!html.contains("<b>") && !html.contains("<script>"));
        assert_eq!(plain("  Acme \t Analytics ", "x"), "Acme Analytics");
    }

    #[test]
    fn a_name_a_mail_client_could_link_is_replaced_in_subject_and_body() {
        // Kept as typed: a dot that ends a word or sits between digits names no host.
        for kept in [
            "Acme Inc.",
            "Acme Inc. Analytics",
            "Q3 v1.2 results",
            "St. Mary's",
        ] {
            assert_eq!(plain(kept, "x"), kept);
        }
        for dropped in [
            "Verify your account at login.evil.com",
            "evil.com/path",
            "https://evil.example/x",
            "www.evil.example",
            "WWW.EVIL.EXAMPLE",
            "see evil。com",
            "mail me at eve@evil.example",
        ] {
            assert_eq!(plain(dropped, "x"), "x", "{dropped}");
        }
        // The reviewer's organization name reaches neither the subject nor the body.
        let sender = Sender {
            name: "login.evil.com",
            email: "eve@example.test",
        };
        let organization = "Verify your account at login.evil.com";
        let subject = invite_subject(&sender, organization);
        let body = invite(&sender, organization, OrgRole::Reader, LINK);
        let html = invite_html(&sender, organization, OrgRole::Reader, LINK);
        let shared = report(organization, "evil.com/path", "");
        let share_text = share(&sender, &shared, LINK);
        let share_page = share_html(&sender, &shared, LINK);
        let share_subject = share_subject(&sender, "evil.com/path");
        for text in [
            &subject,
            &share_subject,
            &body,
            &html,
            &share_text,
            &share_page,
        ] {
            assert!(!text.contains("evil.com"), "{text}");
            assert!(!text.contains("Verify your account"), "{text}");
        }
        assert_eq!(
            subject,
            "A colleague invited you to their organization on RichView"
        );
        assert_eq!(
            share_subject,
            "A colleague shared a RichView report with you"
        );
    }

    #[test]
    fn the_notice_mail_carries_one_link_and_no_provider_supplied_text() {
        let account = "https://app.test/account";
        let told = connected("Microsoft", "11 Nov 2026, 15:25 UTC", account);
        assert!(told.contains("At 11 Nov 2026, 15:25 UTC, a Microsoft account was connected"));
        assert!(told.contains("disconnect it on your Account page"));
        assert!(told.ends_with("RichView · operated by Daniel Vecera\n"));
        assert_eq!(
            connected_subject("GitHub"),
            "GitHub was connected to your RichView account"
        );
        let html_told = connected_html("Microsoft", "11 Nov 2026, 15:25 UTC", account);
        for message in [&told, &html_told] {
            assert_eq!(links(message), 1, "{message}");
            assert!(message.contains(account));
        }
        assert!(html_told.starts_with("<!doctype html>") && html_told.ends_with("</html>"));
        assert!(html_told.contains("max-width:560px"));
        assert!(
            !html_told.contains("<img")
                && !html_told.contains("<style")
                && !html_told.contains("<script")
        );
        assert!(html_told.contains("At 11 Nov 2026, 15:25 UTC."));
    }
}
