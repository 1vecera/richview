//! Unsplash search and attributed placement for `richview image` and the MCP image tools.
//!
//! Follows the Unsplash API guidelines: images are hotlinked from `photo.urls` and never
//! downloaded or embedded, `links.download_location` is requested once when a photo is chosen
//! (`use`, never `search`), and the photographer and Unsplash are linked with
//! `utm_source=<app>&utm_medium=referral`. The access key comes from `UNSPLASH_ACCESS_KEY` only
//! and is sent as a header; it is never printed, logged or placed in an error or JSON result.
use reqwest::{Client, StatusCode, Url, header, redirect};
use serde_json::{Value, json};
use std::time::Duration;

const DEFAULT_API: &str = "https://api.unsplash.com";
const DEFAULT_APP: &str = "richview";
const DEFAULT_WIDTH: u32 = 1600;
const MAX_WIDTH: u32 = 8192;
/// Longest search description, in characters.
const MAX_DESCRIPTION: usize = 200;
/// The only hosts a response may point the author at (the image host and the website).
const IMAGE_HOST: &str = "images.unsplash.com";
const SITE_HOST: &str = "unsplash.com";

/// Failure with a stable machine-readable code. Messages never contain the access key.
#[derive(Debug)]
pub struct UnsplashError {
    pub code: &'static str,
    /// Process exit code: 2 for missing or invalid input and configuration, 3 for API failures.
    pub exit: i32,
    pub message: String,
    pub rate_limit_remaining: Option<u64>,
}

impl UnsplashError {
    fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            exit: 2,
            message: message.into(),
            rate_limit_remaining: None,
        }
    }
    fn api(code: &'static str, message: impl Into<String>, remaining: Option<u64>) -> Self {
        Self {
            code,
            exit: 3,
            message: message.into(),
            rate_limit_remaining: remaining,
        }
    }
    pub fn to_json(&self) -> Value {
        let mut value = json!({"ok": false, "code": self.code, "error": self.message});
        if let Some(remaining) = self.rate_limit_remaining {
            value["rate_limit_remaining"] = json!(remaining);
        }
        value
    }
}

/// Where a chosen photo goes in the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Placement {
    /// A fenced `image` block
    Block,
    /// A `title_image` frontmatter mapping
    TitleImage,
}

impl Placement {
    fn name(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::TitleImage => "title-image",
        }
    }
    fn parse(name: &str) -> Option<Self> {
        match name {
            "block" => Some(Self::Block),
            "title-image" | "title_image" => Some(Self::TitleImage),
            _ => None,
        }
    }
}

/// API endpoint, application name and access key. The key has no `Debug` or `Display` path.
pub struct Config {
    key: String,
    api: String,
    app: String,
}

impl Config {
    /// Reads `UNSPLASH_ACCESS_KEY`, `RICHVIEW_UNSPLASH_API` and `RICHVIEW_UNSPLASH_APP`.
    pub fn from_env() -> Result<Self, UnsplashError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, UnsplashError> {
        let get = |name: &str| {
            lookup(name)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let key = get("UNSPLASH_ACCESS_KEY").ok_or_else(|| {
            UnsplashError::invalid(
                "unsplash.key-missing",
                "Set UNSPLASH_ACCESS_KEY to an Unsplash application access key; RichView reads it from the environment only.",
            )
        })?;
        let api = get("RICHVIEW_UNSPLASH_API").unwrap_or_else(|| DEFAULT_API.into());
        let parsed = Url::parse(&api).ok().filter(|url| {
            (url.scheme() == "https" || (url.scheme() == "http" && is_loopback(url)))
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
        });
        let Some(parsed) = parsed else {
            return Err(UnsplashError::invalid(
                "unsplash.config",
                "RICHVIEW_UNSPLASH_API must be an https base URL (http only for localhost, 127.0.0.1 or ::1) without credentials or a query.",
            ));
        };
        let app = get("RICHVIEW_UNSPLASH_APP").unwrap_or_else(|| DEFAULT_APP.into());
        if !app
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(UnsplashError::invalid(
                "unsplash.config",
                "RICHVIEW_UNSPLASH_APP may contain only letters, digits, '-', '_' and '.'.",
            ));
        }
        Ok(Self {
            key,
            api: parsed.as_str().trim_end_matches('/').to_owned(),
            app,
        })
    }

    fn client() -> Result<Client, UnsplashError> {
        Client::builder()
            .timeout(Duration::from_secs(20))
            // The key must never follow a redirect to another host.
            .redirect(redirect::Policy::none())
            .user_agent(concat!("richview/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| {
                UnsplashError::api("unsplash.network", "Cannot start an HTTP client", None)
            })
    }

    /// One authenticated GET; returns the JSON body and `X-Ratelimit-Remaining` when sent.
    async fn get(&self, url: Url) -> Result<(Value, Option<u64>), UnsplashError> {
        let mut key =
            header::HeaderValue::from_str(&format!("Client-ID {}", self.key)).map_err(|_| {
                UnsplashError::invalid(
                    "unsplash.config",
                    "UNSPLASH_ACCESS_KEY contains characters that cannot be sent in a header.",
                )
            })?;
        key.set_sensitive(true);
        let response = Self::client()?
            .get(url)
            .header(header::AUTHORIZATION, key)
            .header("Accept-Version", "v1")
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| {
                UnsplashError::api(
                    "unsplash.network",
                    format!("Cannot reach the Unsplash API: {}", error.without_url()),
                    None,
                )
            })?;
        let remaining = response
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok());
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status, remaining));
        }
        let body = response.json::<Value>().await.map_err(|_| {
            UnsplashError::api(
                "unsplash.response",
                "The Unsplash API returned a response that is not JSON.",
                remaining,
            )
        })?;
        Ok((body, remaining))
    }

    fn endpoint(&self, path: &str) -> Result<Url, UnsplashError> {
        Url::parse(&format!("{}/{path}", self.api)).map_err(|_| {
            UnsplashError::invalid(
                "unsplash.config",
                "RICHVIEW_UNSPLASH_API is not a valid URL.",
            )
        })
    }

    /// `url` with `utm_source=<app>&utm_medium=referral`, as the API guidelines require.
    fn with_utm(&self, url: &str) -> Option<String> {
        let mut url = Url::parse(url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https"))?;
        url.query_pairs_mut()
            .append_pair("utm_source", &self.app)
            .append_pair("utm_medium", "referral");
        Some(url.into())
    }

    fn unsplash_home(&self) -> String {
        self.with_utm("https://unsplash.com/")
            .unwrap_or_else(|| "https://unsplash.com/".into())
    }

    /// One `GET /search/photos`. Browsing is not use, so the download endpoint is not called.
    pub async fn search(
        &self,
        query: &str,
        orientation: Option<&str>,
        per_page: u8,
    ) -> Result<Value, UnsplashError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(UnsplashError::invalid(
                "unsplash.input",
                "Give a search query.",
            ));
        }
        if !(1..=10).contains(&per_page) {
            return Err(UnsplashError::invalid(
                "unsplash.input",
                "per_page must be from 1 to 10.",
            ));
        }
        if let Some(orientation) = orientation
            && !matches!(orientation, "landscape" | "portrait" | "squarish")
        {
            return Err(UnsplashError::invalid(
                "unsplash.input",
                "orientation must be landscape, portrait or squarish.",
            ));
        }
        let mut url = self.endpoint("search/photos")?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("query", query)
                .append_pair("per_page", &per_page.to_string());
            if let Some(orientation) = orientation {
                pairs.append_pair("orientation", orientation);
            }
        }
        let (body, remaining) = self.get(url).await?;
        let results: Vec<Value> = body["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|photo| self.candidate(photo))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let mut value = json!({
            "ok": true,
            "query": query,
            "total": body["total"],
            "total_pages": body["total_pages"],
            "count": results.len(),
            "results": results,
        });
        if let Some(remaining) = remaining {
            value["rate_limit_remaining"] = json!(remaining);
        }
        Ok(value)
    }

    /// A search result reduced to what an author needs to choose, with attributed links.
    /// `None` when the photo lacks an id or photographer; an unpinned link is an error.
    fn candidate(&self, photo: &Value) -> Result<Option<Value>, UnsplashError> {
        let Some(id) = photo["id"].as_str() else {
            return Ok(None);
        };
        let Some(author) = self.author(photo)? else {
            return Ok(None);
        };
        Ok(Some(json!({
            "id": id,
            "description": description(photo).map(|text| truncate(&text, MAX_DESCRIPTION)),
            "author": author.0,
            "author_url": author.1,
            "width": photo["width"],
            "height": photo["height"],
            "color": photo["color"].as_str().filter(|c| richview_core::image_color(c)),
            "thumb": photo.pointer("/urls/thumb"),
            "regular": photo.pointer("/urls/regular"),
            "unsplash_url": self.photo_page(photo, id)?,
        })))
    }

    /// The photographer's name and profile address with referral parameters. `None` when the
    /// name is missing; a profile link off `unsplash.com` is an error.
    fn author(&self, photo: &Value) -> Result<Option<(String, String)>, UnsplashError> {
        let Some(user) = photo.get("user") else {
            return Ok(None);
        };
        let Some(name) = ["name", "username"]
            .iter()
            .find_map(|key| user[*key].as_str().map(clean).filter(|n| !n.is_empty()))
        else {
            return Ok(None);
        };
        let profile = match user.pointer("/links/html").and_then(Value::as_str) {
            Some(link) => pinned(link, SITE_HOST, "user.links.html")?,
            None => match user["username"].as_str() {
                Some(username) if username_is_plain(username) => {
                    format!("https://{SITE_HOST}/@{username}")
                }
                _ => return Ok(None),
            },
        };
        Ok(self.with_utm(&profile).map(|url| (name, url)))
    }

    fn photo_page(&self, photo: &Value, id: &str) -> Result<Option<String>, UnsplashError> {
        let page = match photo.pointer("/links/html").and_then(Value::as_str) {
            Some(link) => pinned(link, SITE_HOST, "links.html")?,
            None => format!("https://{SITE_HOST}/photos/{id}"),
        };
        Ok(self.with_utm(&page))
    }

    /// Fetches the photo, requests its `download_location` once and returns ready-to-paste YAML.
    pub async fn use_photo(
        &self,
        id: &str,
        width: u32,
        alt: Option<&str>,
        placement: Placement,
    ) -> Result<Value, UnsplashError> {
        let id = id.trim();
        if id.is_empty()
            || id.len() > 64
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(UnsplashError::invalid(
                "unsplash.input",
                "photo id must be the Unsplash id (letters, digits, '-' and '_').",
            ));
        }
        if !(1..=MAX_WIDTH).contains(&width) {
            return Err(UnsplashError::invalid(
                "unsplash.input",
                format!("width must be from 1 to {MAX_WIDTH}."),
            ));
        }
        let (photo, _) = self.get(self.endpoint(&format!("photos/{id}"))?).await?;
        let malformed = |what: &str| {
            UnsplashError::api(
                "unsplash.response",
                format!("The Unsplash photo has no usable {what}."),
                None,
            )
        };
        let raw = photo
            .pointer("/urls/raw")
            .and_then(Value::as_str)
            .ok_or_else(|| malformed("urls.raw"))?;
        let raw = pinned(raw, IMAGE_HOST, "urls.raw")?;
        let raw = raw.as_str();
        let (original_width, original_height) = photo["width"]
            .as_u64()
            .zip(photo["height"].as_u64())
            .filter(|(w, h)| *w > 0 && *h > 0)
            .ok_or_else(|| malformed("width and height"))?;
        let (author, author_url) = self
            .author(&photo)?
            .ok_or_else(|| malformed("photographer"))?;
        let unsplash_url = self.photo_page(&photo, id)?;
        let download = photo
            .pointer("/links/download_location")
            .and_then(Value::as_str)
            .and_then(|location| Url::parse(location).ok())
            .ok_or_else(|| malformed("links.download_location"))?;
        if download.path() != format!("/photos/{id}/download") {
            return Err(UnsplashError::api(
                "unsplash.response",
                format!("The Unsplash links.download_location is not /photos/{id}/download."),
                None,
            ));
        }

        // The tracking request goes to the configured API with the location's path and query, so
        // the key is only ever sent to that API, never to an address taken from a response.
        let mut tracking = self.api.clone();
        tracking.push_str(download.path());
        if let Some(query) = download.query() {
            tracking.push('?');
            tracking.push_str(query);
        }
        let tracking = Url::parse(&tracking).map_err(|_| malformed("links.download_location"))?;
        let (_, remaining) = self.get(tracking).await?;

        let effective = u64::from(width).min(original_width);
        let height = (original_height * effective + original_width / 2) / original_width;
        let src = format!(
            "{raw}{}w={width}&q=80&fm=jpg&fit=max",
            if raw.contains('?') { '&' } else { '?' }
        );
        let alt = alt
            .map(clean)
            .filter(|alt| !alt.is_empty())
            .or_else(|| description(&photo))
            .unwrap_or_else(|| format!("Photo by {author} on Unsplash"));
        let credit = json!({
            "author": author,
            "author_url": author_url,
            "source": "Unsplash",
            "source_url": self.unsplash_home(),
        });
        // The dominant colour fills the photo's box while it loads and when it is unavailable
        // offline. A value that is not `#rrggbb` is left out rather than passed on.
        let color = photo["color"]
            .as_str()
            .filter(|color| richview_core::image_color(color));
        let size = (effective, height);
        let yaml = placement_yaml(placement, &src, &alt, size, color, &credit);
        let mut value = json!({
            "ok": true,
            "id": id,
            "placement": placement.name(),
            "src": src,
            "alt": alt,
            "width": effective,
            "height": height,
            "color": color,
            "credit": credit,
            "unsplash_url": unsplash_url,
            "yaml": yaml,
            "download_tracked": true,
        });
        if let Some(remaining) = remaining {
            value["rate_limit_remaining"] = json!(remaining);
        }
        Ok(value)
    }
}

/// Free text from the API or the caller, as one line: every run of whitespace and control
/// characters (newlines included) becomes a single space, and the ends are trimmed.
fn clean(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn description(photo: &Value) -> Option<String> {
    ["description", "alt_description"].iter().find_map(|key| {
        photo[*key]
            .as_str()
            .map(clean)
            .filter(|text| !text.is_empty())
    })
}

fn is_loopback(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn username_is_plain(username: &str) -> bool {
    !username.is_empty()
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `link` if it is https on exactly `host` (default port, no credentials); otherwise an error
/// naming the response field. Response links are never trusted to point elsewhere.
fn pinned(link: &str, host: &str, field: &str) -> Result<String, UnsplashError> {
    Url::parse(link)
        .ok()
        .filter(|url| {
            url.scheme() == "https"
                && url.host_str() == Some(host)
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
        })
        .map(String::from)
        .ok_or_else(|| {
            UnsplashError::api(
                "unsplash.untrusted-host",
                format!("The Unsplash response has {field} outside https://{host}; refusing it."),
                None,
            )
        })
}

fn status_error(status: StatusCode, remaining: Option<u64>) -> UnsplashError {
    match status.as_u16() {
        401 => UnsplashError::api(
            "unsplash.unauthorized",
            "Unsplash rejected the access key (HTTP 401). Check UNSPLASH_ACCESS_KEY.",
            remaining,
        ),
        404 => UnsplashError::api(
            "unsplash.not-found",
            "Unsplash has no such photo (HTTP 404). Check the photo id.",
            remaining,
        ),
        300..=399 => UnsplashError::api(
            "unsplash.redirect",
            format!(
                "The Unsplash API answered HTTP {} with a redirect; RichView does not follow redirects.",
                status.as_u16()
            ),
            remaining,
        ),
        429 => rate_limited(remaining),
        403 if remaining == Some(0) => rate_limited(remaining),
        403 => UnsplashError::api(
            "unsplash.forbidden",
            "Unsplash refused the request (HTTP 403): the key lacks permission or its hourly limit is used up.",
            remaining,
        ),
        code => UnsplashError::api(
            "unsplash.http",
            format!("The Unsplash API answered HTTP {code}."),
            remaining,
        ),
    }
}

fn rate_limited(remaining: Option<u64>) -> UnsplashError {
    UnsplashError::api(
        "unsplash.rate-limited",
        "Unsplash rate limit reached. Demo applications get 50 requests per hour; wait for the hour to reset or ask Unsplash for production access.",
        remaining,
    )
}

/// The image mapping, as an `image` fence or a `title_image` frontmatter entry.
fn placement_yaml(
    placement: Placement,
    src: &str,
    alt: &str,
    (width, height): (u64, u64),
    color: Option<&str>,
    credit: &Value,
) -> String {
    use richview_core::yaml::{Node, to_string};
    let text =
        |value: &Value| Node::Scalar(Value::String(clean(value.as_str().unwrap_or_default())));
    let credit = Node::Map(
        ["author", "author_url", "source", "source_url"]
            .into_iter()
            .map(|key| (key.to_owned(), text(&credit[key])))
            .collect(),
    );
    let mut image = vec![
        ("src".to_owned(), Node::Scalar(src.into())),
        ("alt".to_owned(), Node::Scalar(clean(alt).into())),
        ("width".to_owned(), Node::Scalar(width.into())),
        ("height".to_owned(), Node::Scalar(height.into())),
    ];
    if let Some(color) = color {
        image.push(("color".to_owned(), Node::Scalar(color.into())));
    }
    image.push(("credit".to_owned(), credit));
    match placement {
        Placement::Block => {
            let body = to_string(&Node::Map(image));
            // Longer than any backtick run in the body, so nothing in it can close the fence.
            let longest = body
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or_default();
            let fence = "`".repeat(longest.max(2) + 1);
            format!("{fence}image\n{body}{fence}\n")
        }
        Placement::TitleImage => to_string(&Node::Map(vec![(
            "title_image".to_owned(),
            Node::Map(image),
        )])),
    }
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum ImageFormat {
    Text,
    Json,
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum Orientation {
    Landscape,
    Portrait,
    Squarish,
}

impl Orientation {
    fn name(self) -> &'static str {
        match self {
            Self::Landscape => "landscape",
            Self::Portrait => "portrait",
            Self::Squarish => "squarish",
        }
    }
}

#[derive(clap::Subcommand)]
pub enum ImageCommand {
    /// Search Unsplash and list candidate photos (does not register a download)
    ///
    /// Reads UNSPLASH_ACCESS_KEY; RICHVIEW_UNSPLASH_API and RICHVIEW_UNSPLASH_APP are optional.
    Search {
        query: String,
        #[arg(long, value_enum)]
        orientation: Option<Orientation>,
        /// Results to return, from 1 to 10
        #[arg(long, default_value_t = 6, value_parser = clap::value_parser!(u8).range(1..=10))]
        per_page: u8,
        #[arg(long, value_enum, default_value = "text")]
        format: ImageFormat,
    },
    /// Choose a photo: register the download once and print the YAML with its credit
    ///
    /// If the download-tracking call fails, the command prints an error and no YAML, so retrying
    /// is safe, but Unsplash may then count the download twice.
    Use {
        photo_id: String,
        /// Requested image width in pixels; the file is never enlarged past the original
        #[arg(long, default_value_t = DEFAULT_WIDTH)]
        width: u32,
        /// Alt text; defaults to the photo's own description
        #[arg(long)]
        alt: Option<String>,
        /// A fenced image block or a title_image frontmatter mapping
        #[arg(long = "as", value_enum, default_value = "block")]
        placement: Placement,
        #[arg(long, value_enum, default_value = "text")]
        format: ImageFormat,
    },
}

/// Runs an image command and returns the process exit code.
pub async fn run(command: ImageCommand) -> i32 {
    let (format, result) = match &command {
        ImageCommand::Search {
            query,
            orientation,
            per_page,
            format,
        } => (
            *format,
            search_with_env(query, orientation.map(Orientation::name), *per_page).await,
        ),
        ImageCommand::Use {
            photo_id,
            width,
            alt,
            placement,
            format,
        } => (
            *format,
            use_with_env(photo_id, *width, alt.as_deref(), *placement).await,
        ),
    };
    match (result, format) {
        (Ok(value), ImageFormat::Json) => {
            println!("{value}");
            0
        }
        (Ok(value), ImageFormat::Text) => {
            print!("{}", text_output(&command, &value));
            if value["download_tracked"] == true {
                eprintln!("Download registered with Unsplash.");
            }
            0
        }
        (Err(error), ImageFormat::Json) => {
            println!("{}", error.to_json());
            error.exit
        }
        (Err(error), ImageFormat::Text) => {
            eprintln!("richview: {}", error.message);
            error.exit
        }
    }
}

async fn search_with_env(
    query: &str,
    orientation: Option<&str>,
    per_page: u8,
) -> Result<Value, UnsplashError> {
    Config::from_env()?
        .search(query, orientation, per_page)
        .await
}

async fn use_with_env(
    id: &str,
    width: u32,
    alt: Option<&str>,
    placement: Placement,
) -> Result<Value, UnsplashError> {
    Config::from_env()?
        .use_photo(id, width, alt, placement)
        .await
}

fn text_output(command: &ImageCommand, value: &Value) -> String {
    match command {
        ImageCommand::Use { .. } => value["yaml"].as_str().unwrap_or_default().to_owned(),
        ImageCommand::Search { .. } => {
            let results = value["results"].as_array().cloned().unwrap_or_default();
            if results.is_empty() {
                return "No photos found.\n".into();
            }
            let mut out = String::new();
            for photo in &results {
                let field = |key: &str| photo[key].as_str().unwrap_or_default();
                out.push_str(&format!(
                    "{}  {}x{}  {}\n  by {} ({})\n  {}\n  thumb {}\n",
                    field("id"),
                    photo["width"],
                    photo["height"],
                    photo["description"].as_str().unwrap_or("(no description)"),
                    field("author"),
                    field("author_url"),
                    field("unsplash_url"),
                    field("thumb"),
                ));
            }
            out.push_str("\nChoose one with: richview image use <id>\n");
            out
        }
    }
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "richview_image_search",
            "description": "Search Unsplash for photos. Returns candidates {id, description, author, author_url, width, height, color, thumb, regular, unsplash_url}. Show the user the photographer for each candidate and let them choose; searching does not register a download. Needs UNSPLASH_ACCESS_KEY in the server environment.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string", "description": "What the photo should show"},
                "orientation": {"type": "string", "enum": ["landscape", "portrait", "squarish"]},
                "per_page": {"type": "integer", "minimum": 1, "maximum": 10, "description": "Results to return (default 6)"}
            }, "required": ["query"]},
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true}
        }),
        json!({
            "name": "richview_image_use",
            "description": "Use a chosen Unsplash photo: registers the download with Unsplash once (as the API guidelines require) and returns ready-to-paste YAML in `yaml`, an `image` block or a `title_image` frontmatter mapping, with a hotlinked src, width, height, the photo's dominant `color` (#rrggbb, omitted if the API gives none) and a visible credit (Photo by <name> on Unsplash). Reports may hotlink only credited images.unsplash.com photos. Paste the YAML into the .rmd unchanged and never download or rehost the image. Call it only for a photo that will appear in the report, once per photo. If it fails during download tracking it returns an error without YAML; retrying is safe but may count the download twice.",
            "inputSchema": {"type": "object", "properties": {
                "photo_id": {"type": "string", "description": "id from richview_image_search"},
                "width": {"type": "integer", "minimum": 1, "maximum": MAX_WIDTH, "description": "Image width in pixels (default 1600)"},
                "alt": {"type": "string", "description": "Alt text; defaults to the photo's description"},
                "as": {"type": "string", "enum": ["block", "title-image"], "description": "image fence (default) or title_image frontmatter"}
            }, "required": ["photo_id"]},
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true}
        }),
    ]
}

/// Handles the image tools; `None` for any other tool name.
pub async fn call_tool(name: &str, args: &Value) -> Option<(Value, bool)> {
    let result = match name {
        "richview_image_search" => tool_search(args).await,
        "richview_image_use" => tool_use(args).await,
        _ => return None,
    };
    Some(match result {
        Ok(value) => (value, false),
        Err(error) => (error.to_json(), true),
    })
}

async fn tool_search(args: &Value) -> Result<Value, UnsplashError> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let orientation = optional_str(args, "orientation")?;
    let per_page = match args.get("per_page") {
        None | Some(Value::Null) => 6,
        Some(value) => value
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .ok_or_else(|| {
                UnsplashError::invalid("unsplash.input", "per_page must be from 1 to 10.")
            })?,
    };
    Config::from_env()?
        .search(query, orientation, per_page)
        .await
}

async fn tool_use(args: &Value) -> Result<Value, UnsplashError> {
    let id = args
        .get("photo_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let width = match args.get("width") {
        None | Some(Value::Null) => DEFAULT_WIDTH,
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                UnsplashError::invalid(
                    "unsplash.input",
                    format!("width must be from 1 to {MAX_WIDTH}."),
                )
            })?,
    };
    let placement = match optional_str(args, "as")? {
        None => Placement::Block,
        Some(name) => Placement::parse(name).ok_or_else(|| {
            UnsplashError::invalid("unsplash.input", "as must be block or title-image.")
        })?,
    };
    Config::from_env()?
        .use_photo(id, width, optional_str(args, "alt")?, placement)
        .await
}

fn optional_str<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, UnsplashError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(UnsplashError::invalid(
            "unsplash.input",
            format!("{key} must be a string."),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> Result<Config, UnsplashError> {
        Config::from_lookup(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    #[test]
    fn configuration_requires_a_key_and_validates_the_rest() {
        assert_eq!(config(&[]).err().unwrap().code, "unsplash.key-missing");
        assert_eq!(
            config(&[("UNSPLASH_ACCESS_KEY", "  ")]).err().unwrap().code,
            "unsplash.key-missing"
        );
        let ok = config(&[("UNSPLASH_ACCESS_KEY", "k")]).unwrap();
        assert_eq!(
            (ok.api.as_str(), ok.app.as_str()),
            (DEFAULT_API, "richview")
        );
        for api in [
            "http://api.example.test",
            "ftp://x",
            "not a url",
            "https://user:pw@x.test",
            "https://x.test/?a=1",
        ] {
            assert_eq!(
                config(&[("UNSPLASH_ACCESS_KEY", "k"), ("RICHVIEW_UNSPLASH_API", api)])
                    .err()
                    .unwrap()
                    .code,
                "unsplash.config",
                "{api}"
            );
        }
        for api in [
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
            "http://localhost:3000",
            "https://proxy.example.test/unsplash",
        ] {
            assert!(
                config(&[("UNSPLASH_ACCESS_KEY", "k"), ("RICHVIEW_UNSPLASH_API", api)]).is_ok(),
                "{api}"
            );
        }
        assert!(
            config(&[
                ("UNSPLASH_ACCESS_KEY", "k"),
                ("RICHVIEW_UNSPLASH_APP", "a b")
            ])
            .is_err()
        );
    }

    #[test]
    fn referral_parameters_are_appended_and_keep_existing_queries() {
        let config = config(&[
            ("UNSPLASH_ACCESS_KEY", "k"),
            ("RICHVIEW_UNSPLASH_APP", "my-app"),
        ])
        .unwrap();
        assert_eq!(
            config.with_utm("https://unsplash.com/@a?x=1").unwrap(),
            "https://unsplash.com/@a?x=1&utm_source=my-app&utm_medium=referral"
        );
        assert_eq!(
            config.unsplash_home(),
            "https://unsplash.com/?utm_source=my-app&utm_medium=referral"
        );
        assert!(config.with_utm("javascript:alert(1)").is_none());
    }

    #[test]
    fn hostile_strings_are_collapsed_and_the_fence_outgrows_any_backtick_run() {
        assert_eq!(clean("  a\n```\r\n# x\u{0}\u{2028}y\t "), "a ``` # x y");
        let credit = json!({"author": "E\n````\nve", "author_url": "https://unsplash.com/@e", "source": "Unsplash", "source_url": "https://unsplash.com/"});
        let block = placement_yaml(
            Placement::Block,
            "https://images.unsplash.com/p",
            "a\n```\n# injected",
            (10, 5),
            None,
            &credit,
        );
        assert!(
            block.starts_with("`````image\n") && block.ends_with("\n`````\n"),
            "{block}"
        );
        let source = format!("---\ntitle: T\nversion: 0.0.1\n---\n\n{block}");
        let document = richview_core::document::parse(&source).unwrap();
        assert_eq!(document.blocks.len(), 1);
        assert_eq!(document.blocks[0].value["alt"], "a ``` # injected");
        assert_eq!(document.blocks[0].value["credit"]["author"], "E ```` ve");
        assert_eq!(truncate(&"é".repeat(300), 200).chars().count(), 200);
    }

    #[test]
    fn response_links_must_be_https_on_the_expected_host() {
        let ok = "https://images.unsplash.com/photo-1?ixid=1";
        assert!(pinned(ok, IMAGE_HOST, "urls.raw").is_ok());
        for bad in [
            "http://images.unsplash.com/p",
            "https://images.unsplash.com.evil.test/p",
            "https://evil.test/images.unsplash.com",
            "https://images.unsplash.com:8443/p",
            "https://u:p@images.unsplash.com/p",
            "not a url",
        ] {
            let error = pinned(bad, IMAGE_HOST, "urls.raw").unwrap_err();
            assert_eq!(error.code, "unsplash.untrusted-host", "{bad}");
        }
    }

    #[test]
    fn placement_yaml_round_trips_as_a_valid_image_block() {
        let credit = json!({"author": "Ada: Lovelace", "author_url": "https://unsplash.com/@a?utm_source=richview&utm_medium=referral", "source": "Unsplash", "source_url": "https://unsplash.com/?utm_source=richview&utm_medium=referral"});
        let block = placement_yaml(
            Placement::Block,
            "https://images.unsplash.com/p?ixid=1&w=1600&q=80&fm=jpg&fit=max",
            "A sea: calm",
            (1600, 900),
            Some("#262626"),
            &credit,
        );
        let body = block
            .strip_prefix("```image\n")
            .unwrap()
            .strip_suffix("```\n")
            .unwrap();
        let parsed: Value = richview_core::yaml::from_str(body).unwrap();
        assert_eq!(parsed["alt"], "A sea: calm");
        assert_eq!(parsed["credit"]["author"], "Ada: Lovelace");
        assert_eq!(parsed["width"], 1600);
        assert_eq!(parsed["color"], "#262626");
        let title = placement_yaml(
            Placement::TitleImage,
            "https://x/y",
            "t",
            (10, 5),
            None,
            &credit,
        );
        let parsed: Value = richview_core::yaml::from_str(&title).unwrap();
        assert_eq!(parsed["title_image"]["credit"]["source"], "Unsplash");
    }
}
