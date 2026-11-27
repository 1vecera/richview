//! Input bytes for one report version, before encoding or rendering (TYP-105).
use crate::resolver::{contained_path, linked_paths};
use richview_core::document::{Diagnostic, Document};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
    sync::Arc,
};

const FILE_LIMIT: u64 = 5_000_000;
const REPORT_LIMIT: u64 = 50_000_000;
/// The most one publish request may hold, as the server reads it (TYP-43).
pub(crate) const REQUEST_LIMIT: usize = crate::MAX_INPUT_BYTES;
// The finding below states this number.
const _: () = assert!(REQUEST_LIMIT == 33_554_432);

/// The finding for a publish request over [`REQUEST_LIMIT`], raised before anything is sent.
/// Linked files travel as base64, a third larger, so fewer than 50,000,000 bytes of them fit.
pub(crate) fn request_too_large(bytes: usize) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(
        "error",
        "report.input-limit",
        format!(
            "This report's publish request is {bytes} bytes once encoded. A publish request can hold up to 32 MiB (33,554,432 bytes), and linked files travel as base64, a third larger, so about 25,000,000 bytes of linked files is the most that can be published. Reduce the inputs or split the report."
        ),
        "source",
    ))
}

/// The most files a request may list in its folder inventory, carry, or send as data.
pub(crate) const MAX_REQUEST_FILES: usize = 10_000;
/// The longest a file's path in a request, or the report's own file name, may be. Real ones are
/// short; a longer one only makes every check of a path cost more.
pub(crate) const MAX_REQUEST_PATH_BYTES: usize = 512;

/// The shape of a request, checked before it is read any further and before anything is locked:
/// how many files it lists and how long their names are. A request outside these costs the server
/// time in proportion to their product, whatever its size in bytes, so it is refused as it is.
pub(crate) fn request_shape(
    folder: Option<&richview_core::document::ReportFolder>,
    files: &BTreeMap<String, String>,
    data: &BTreeMap<String, String>,
    styles: &[String],
) -> Result<(), Box<Diagnostic>> {
    let refuse = |what: String, path: &str| {
        Box::new(
            Diagnostic::new(
                "error",
                "report.input-limit",
                format!("{what} Reduce the report folder, or split the report."),
                path,
            )
            .fix("Publish a report folder with fewer files and shorter names"),
        )
    };
    let too_long = |name: &str| name.len() > MAX_REQUEST_PATH_BYTES;
    if let Some(folder) = folder {
        if folder.files.len() > MAX_REQUEST_FILES {
            return Err(refuse(
                format!(
                    "The report folder lists more than {MAX_REQUEST_FILES} files; a request may list at most that many."
                ),
                "report_folder",
            ));
        }
        if too_long(&folder.source) || too_long(&folder.root) {
            return Err(refuse(
                format!(
                    "The report's file name or folder name is over {MAX_REQUEST_PATH_BYTES} bytes."
                ),
                "report_folder",
            ));
        }
        if let Some(long) = folder.files.iter().find(|name| too_long(name)) {
            return Err(refuse(
                format!(
                    "A file in the report folder has a path of {} bytes; a path may have at most {MAX_REQUEST_PATH_BYTES}.",
                    long.len()
                ),
                "report_folder",
            ));
        }
    }
    if files.len() > MAX_REQUEST_FILES || data.len() > MAX_REQUEST_FILES {
        return Err(refuse(
            format!("The request carries more than {MAX_REQUEST_FILES} files."),
            "report_folder",
        ));
    }
    if let Some(long) = files.keys().chain(data.keys()).find(|name| too_long(name)) {
        return Err(refuse(
            format!(
                "A file or a data name has a path of {} bytes; a path may have at most {MAX_REQUEST_PATH_BYTES}.",
                long.len()
            ),
            "report_folder",
        ));
    }
    if styles.len() > MAX_REQUEST_STYLES {
        return Err(refuse(
            format!("The request carries more than {MAX_REQUEST_STYLES} styling files."),
            "styles",
        ));
    }
    Ok(())
}

/// The most styling files a request may carry. A report names a few.
pub(crate) const MAX_REQUEST_STYLES: usize = 256;

/// The most chart widths a check may name. The renderer lays figures out at the first two.
pub(crate) const MAX_REQUEST_WIDTHS: usize = 16;

/// Refuses a list of widths longer than a check takes.
pub(crate) fn widths_shape(widths: &[f64]) -> Result<(), Box<Diagnostic>> {
    if widths.len() <= MAX_REQUEST_WIDTHS {
        return Ok(());
    }
    Err(Box::new(
        Diagnostic::new(
            "error",
            "report.input-limit",
            format!(
                "The request names more than {MAX_REQUEST_WIDTHS} chart widths; a check takes at most that many."
            ),
            "widths",
        )
        .fix("Name the widths figures occupy for readers, a few at most"),
    ))
}

/// Readers for the collections of a request that keep at most one more than the limit and skip the
/// rest as they read it, so that a request of the permitted size in bytes cannot be made to hold
/// millions of entries in memory before anything counts them. The one over the limit stays for
/// `request_shape` to refuse; the others are not kept.
pub(crate) mod bounded {
    use super::{MAX_REQUEST_FILES, MAX_REQUEST_STYLES};
    use serde::Deserialize;
    use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
    use std::collections::BTreeMap;
    use std::fmt;

    struct Strings(usize);

    impl<'de> Visitor<'de> for Strings {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a list of strings")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut kept = Vec::new();
            while kept.len() <= self.0 {
                match seq.next_element::<String>()? {
                    Some(next) => kept.push(next),
                    None => return Ok(kept),
                }
            }
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(kept)
        }
    }

    struct Floats(usize);

    impl<'de> Visitor<'de> for Floats {
        type Value = Vec<f64>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a list of numbers")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<f64>, A::Error> {
            let mut kept = Vec::new();
            while kept.len() <= self.0 {
                match seq.next_element::<f64>()? {
                    Some(next) => kept.push(next),
                    None => return Ok(kept),
                }
            }
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(kept)
        }
    }

    struct Strung(usize);

    impl<'de> Visitor<'de> for Strung {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a map of strings")
        }

        fn visit_map<A: MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<BTreeMap<String, String>, A::Error> {
            let mut kept = BTreeMap::new();
            while kept.len() <= self.0 {
                match map.next_entry::<String, String>()? {
                    Some((key, value)) => {
                        kept.insert(key, value);
                    }
                    None => return Ok(kept),
                }
            }
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(kept)
        }
    }

    /// A list of file names.
    pub fn names<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
        d.deserialize_seq(Strings(MAX_REQUEST_FILES))
    }

    /// The chart widths of a check.
    pub fn widths<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f64>, D::Error> {
        d.deserialize_seq(Floats(super::MAX_REQUEST_WIDTHS))
    }

    /// The styling files of a request.
    pub fn styles<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
        d.deserialize_seq(Strings(MAX_REQUEST_STYLES))
    }

    /// Files or data by name.
    pub fn files<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
        d.deserialize_map(Strung(MAX_REQUEST_FILES))
    }

    /// The report folder of a request, with its list of files read as `names` reads one.
    pub fn folder<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<richview_core::document::ReportFolder>, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            root: String,
            source: String,
            #[serde(deserialize_with = "names")]
            files: Vec<String>,
        }
        Ok(
            Option::<Wire>::deserialize(d)?.map(|wire| richview_core::document::ReportFolder {
                root: wire.root,
                source: wire.source,
                files: wire.files,
            }),
        )
    }
}

fn file_limit(path: &str, bytes: u64) -> Result<(), Box<Diagnostic>> {
    if bytes <= FILE_LIMIT {
        return Ok(());
    }
    Err(Box::new(Diagnostic::new("error", "report.file-limit", format!("File ‘{path}’ is {bytes} bytes. The limit is 5,000,000 bytes. Reduce or split this file."), path).in_file(path)))
}

pub(crate) fn encoded(error: Box<Diagnostic>) -> String {
    serde_json::to_string(&[error]).unwrap()
}

/// Read through the same open handle used to measure the file. A growing file cannot
/// turn a checked size into an unbounded allocation.
pub fn read_input(path: &Path, name: &str) -> Result<Vec<u8>, String> {
    // The source of a report whose folder the command holds open is read from that folder.
    let file = crate::resolver::pinned_source(path)
        .unwrap_or_else(|| fs::File::open(path))
        .map_err(|e| format!("{name}: {e}"))?;
    read_open_file(file, name).map(|(bytes, _)| bytes)
}

/// `read_input` for the file `name` of a report folder, opened beneath the folder, so the file read
/// is one inside it.
pub(crate) fn read_input_beneath(root: &Path, name: &str) -> Result<Vec<u8>, String> {
    read_open_file(crate::resolver::open_beneath(root, name)?, name).map(|(bytes, _)| bytes)
}

/// The bytes of an open file, and its permission bits taken from the same handle (none where the
/// system has no such bits).
fn read_open_file(file: fs::File, name: &str) -> Result<(Vec<u8>, Option<Perm>), String> {
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err(format!("{name} is not a regular file"));
    }
    file_limit(name, meta.len()).map_err(encoded)?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    (&file)
        .take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > FILE_LIMIT {
        let measured = file
            .metadata()
            .map_err(|e| e.to_string())?
            .len()
            .max(bytes.len() as u64);
        file_limit(name, measured).map_err(encoded)?;
    }
    Ok((bytes, file_mode(&meta)))
}

/// The permission bits of a file and the group that owns it, as an open handle gave them. A copy
/// of the file that is to be as private as the file is needs both: group bits mean a group, and a
/// new file may be in another one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) struct Perm {
    pub mode: u32,
    pub group: Option<u32>,
}

impl Perm {
    /// Permission bits whose group is not known, which is none the copy is moved into.
    #[cfg(test)]
    pub(crate) fn bits(mode: u32) -> Self {
        Self { mode, group: None }
    }
}

#[cfg(unix)]
fn file_mode(meta: &fs::Metadata) -> Option<Perm> {
    use std::os::unix::fs::MetadataExt;
    Some(Perm {
        mode: meta.mode(),
        group: Some(meta.gid()),
    })
}

#[cfg(not(unix))]
fn file_mode(_: &fs::Metadata) -> Option<Perm> {
    None
}

#[derive(Default, Debug, Clone)]
pub struct InputSizes {
    source: String,
    pub(crate) links: BTreeSet<String>,
    // A download or cover also embedded in the report must use the same bytes in both roles.
    shared: BTreeMap<String, Option<Arc<Vec<u8>>>>,
    /// The permission bits of each file read, as its open handle gave them with its bytes.
    modes: BTreeMap<String, Option<Perm>>,
    local_connectors: BTreeSet<String>,
    files: BTreeMap<String, u64>,
    results: BTreeMap<String, u64>,
    total: u64,
}

impl InputSizes {
    pub(crate) fn new(source: &str, bytes: usize) -> Result<Self, Box<Diagnostic>> {
        let mut sizes = Self {
            source: source.into(),
            ..Default::default()
        };
        sizes.record(source, bytes as u64)?;
        Ok(sizes)
    }

    pub(crate) fn record(&mut self, path: &str, bytes: u64) -> Result<(), Box<Diagnostic>> {
        file_limit(path, bytes)?;
        let previous = self.files.insert(normalize(path), bytes).unwrap_or(0);
        self.total = self.total - previous + bytes;
        Ok(())
    }

    pub(crate) fn is_shared(&self, name: &str) -> bool {
        self.shared.contains_key(&normalize(name))
    }

    /// The bytes of the file `name` in the report folder `root`, opened beneath it.
    pub(crate) fn read(&mut self, root: &Path, name: &str) -> Result<Arc<Vec<u8>>, String> {
        let key = normalize(name);
        if let Some(Some(bytes)) = self.shared.get(&key) {
            return Ok(bytes.clone());
        }
        let (bytes, mode) = read_open_file(crate::resolver::open_beneath(root, name)?, name)?;
        let bytes = Arc::new(bytes);
        self.modes.insert(key.clone(), mode);
        self.record(name, bytes.len() as u64).map_err(encoded)?;
        self.check().map_err(encoded)?;
        if let Some(slot) = self.shared.get_mut(&key) {
            *slot = Some(bytes.clone());
        }
        Ok(bytes)
    }

    /// The permission bits of the file `name` was last read from, as the handle that read its bytes
    /// gave them.
    pub(crate) fn mode_read(&self, name: &str) -> Option<Perm> {
        self.modes.get(&normalize(name)).copied().flatten()
    }

    /// Read the exact bytes for a download or cover against the resolved version's total.
    pub fn attachment(&mut self, root: &Path, name: &str) -> Result<Arc<Vec<u8>>, String> {
        self.read(root, name)
    }

    pub(crate) fn encoded_downloads(
        &mut self,
        root: &Path,
        doc: &Document,
    ) -> Result<BTreeMap<String, String>, String> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let mut files = BTreeMap::new();
        // The server's link check wants every spelling of a linked path (`x/a`, `./x/a`), so each
        // is carried; the bytes are counted once, so the copies are counted here, against what a
        // publish request can hold, before the next one is made.
        let mut held = 0usize;
        for path in linked_paths(doc) {
            if !richview_core::document::is_source_file(&path) {
                let bytes = self.attachment(root, &path)?;
                let text = STANDARD.encode(bytes.as_slice());
                held += text.len();
                if held > REQUEST_LIMIT {
                    return Err(encoded(request_too_large(held)));
                }
                files.insert(path, text);
            }
        }
        Ok(files)
    }

    pub(crate) fn result(
        &mut self,
        id: String,
        label: &str,
        bytes: usize,
    ) -> Result<(), Box<Diagnostic>> {
        file_limit(label, bytes as u64)?;
        let previous = self.results.insert(id, bytes as u64).unwrap_or(0);
        self.total = self.total - previous + bytes as u64;
        Ok(())
    }

    /// The table a `data.parquet` block reads. A report carries that text, not the file, so
    /// every surface counts it as the server does and names it in the message (TYP-105).
    pub(crate) fn table(&mut self, path: &str, bytes: usize) -> Result<(), Box<Diagnostic>> {
        self.result(normalize(path), &format!("table read from {path}"), bytes)
            .map_err(|mut found| {
                found.file = Some(path.into());
                found
            })
    }

    pub(crate) fn style(&mut self, index: usize, bytes: usize) -> Result<(), Box<Diagnostic>> {
        file_limit(&format!("styles[{index}]"), bytes as u64)?;
        self.total += bytes as u64;
        Ok(())
    }

    pub(crate) fn local_connector(&self, name: &str) -> bool {
        self.local_connectors.contains(name)
    }

    pub(crate) fn check(&self) -> Result<(), Box<Diagnostic>> {
        let bytes = self.total;
        if bytes <= REPORT_LIMIT {
            return Ok(());
        }
        Err(Box::new(Diagnostic::new("error", "report.input-limit", format!("This report uses {bytes} bytes of inputs. The limit is 50,000,000 bytes. Reduce the inputs or split the report."), "source").in_file(&self.source)))
    }

    pub(crate) fn local(&mut self, root: &Path, reference: &str) -> Result<(), Box<Diagnostic>> {
        // Resolution owns findings for absent and escaping paths. Never stat them here.
        if self
            .shared
            .get(&normalize(reference))
            .is_some_and(Option::is_some)
        {
            return Ok(());
        }
        if let Ok(path) = contained_path(root, reference)
            && let Ok(meta) = fs::metadata(path)
        {
            self.record(reference, meta.len())?;
        }
        Ok(())
    }

    pub(crate) fn referenced(
        &mut self,
        root: &Path,
        doc: &Document,
        styles: &[String],
        declaration: Option<&crate::connectors::config::ReportConnectors>,
    ) -> Result<(), Box<Diagnostic>> {
        for block in &doc.blocks {
            let data = &block.value["data"];
            if data
                .pointer("/sql/inline")
                .and_then(|v| v.as_str())
                .is_none()
                && let Some(query) = data.pointer("/sql/query").and_then(|v| v.as_str())
            {
                self.local(root, query)?;
            }
            if let Some(name) = data.pointer("/sql/connector").and_then(|v| v.as_str())
                && let Some(path) = declaration.and_then(|d| d.local_source(name))
            {
                self.local(root, path)?;
                if self.files.contains_key(&normalize(path)) {
                    self.local_connectors.insert(name.into());
                }
            }
        }
        // A direct Parquet file is counted as the table read from it, when it is read. Only a
        // link to it, which sends the file itself, counts its size on disk.
        let parquet = direct_parquet(doc);
        for resource in richview_core::document::report_resources(doc) {
            if resource.kind == "data" && parquet.contains(&normalize(&resource.path)) {
                continue;
            }
            self.local(root, &resource.path)?;
        }
        let links = linked_paths(doc);
        for path in &links {
            self.local(root, path)?;
            self.shared.entry(normalize(path)).or_default();
        }
        self.links = links;
        for (index, path) in styles.iter().enumerate() {
            if !richview_core::is_style_name(path) {
                if path.contains('\n') {
                    self.style(index, path.len())?;
                } else {
                    self.local(root, path)?;
                }
            }
        }
        if let Some(cover) = doc
            .frontmatter
            .pointer("/publish/cover")
            .and_then(|v| v.as_str())
        {
            self.local(root, cover)?;
            self.shared.entry(normalize(cover)).or_default();
        }
        self.check()
    }
}

/// The report-relative paths blocks read as `data.parquet`.
pub(crate) fn direct_parquet(doc: &Document) -> BTreeSet<String> {
    doc.blocks
        .iter()
        .flat_map(|b| {
            [
                b.value.pointer("/data/parquet"),
                b.value.pointer("/chart/data/parquet"),
            ]
        })
        .flatten()
        .filter_map(|v| v.as_str())
        .map(normalize)
        .collect()
}

pub(crate) fn normalize(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut first = true;
    for part in path.split('/').filter(|part| *part != ".") {
        if !first {
            out.push('/');
        }
        first = false;
        out.push_str(part);
    }
    out
}

/// Point the size findings of an encoded resolution error at the block that names their file,
/// as the server does for what it receives.
pub(crate) fn locate_encoded(error: String, doc: &Document) -> String {
    match serde_json::from_str::<Vec<Diagnostic>>(&error) {
        Ok(found) => serde_json::to_string(
            &found
                .into_iter()
                .map(|d| *locate(Box::new(d), doc))
                .collect::<Vec<_>>(),
        )
        .unwrap_or(error),
        Err(_) => error,
    }
}

pub(crate) fn locate(mut error: Box<Diagnostic>, doc: &Document) -> Box<Diagnostic> {
    if error.code == "report.file-limit" {
        richview_core::document::locate_report_input(doc, &mut error);
    }
    error
}

#[cfg(test)]
mod tests {
    const HEAD: &str = "---\ntitle: T\ndescription: D\nversion: 1.0.0\n---\n\n# T\n\n";

    fn carried(
        root: &std::path::Path,
        links: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        let source = format!("{HEAD}{links}\n");
        let Ok(document) = richview_core::document::parse(&source) else {
            panic!("the source parses");
        };
        crate::resolver::read_linked_files(root, &document)
    }

    #[test]
    fn a_request_is_read_into_at_most_one_entry_over_what_it_may_hold() {
        use super::{MAX_REQUEST_FILES, MAX_REQUEST_STYLES, bounded};
        let many = |n: usize| {
            (0..n)
                .map(|i| format!("\"f{i}\""))
                .collect::<Vec<_>>()
                .join(",")
        };
        let list = format!("[{}]", many(MAX_REQUEST_FILES * 5));
        let mut de = serde_json::Deserializer::from_str(&list);
        let names = bounded::names(&mut de).unwrap();
        assert_eq!(names.len(), MAX_REQUEST_FILES + 1);
        // Under the limit it is every entry, as it was written.
        let mut de = serde_json::Deserializer::from_str("[\"a\",\"b\"]");
        assert_eq!(bounded::names(&mut de).unwrap(), ["a", "b"]);
        let styles = format!("[{}]", many(MAX_REQUEST_STYLES * 3));
        let mut de = serde_json::Deserializer::from_str(&styles);
        assert_eq!(
            bounded::styles(&mut de).unwrap().len(),
            MAX_REQUEST_STYLES + 1
        );
        let map = format!(
            "{{{}}}",
            (0..MAX_REQUEST_FILES * 3)
                .map(|i| format!("\"k{i}\":\"v\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut de = serde_json::Deserializer::from_str(&map);
        assert_eq!(
            bounded::files(&mut de).unwrap().len(),
            MAX_REQUEST_FILES + 1
        );
        // The shape check refuses what was kept, whatever it lists, and names no more than that.
        let folder = richview_core::document::ReportFolder {
            root: "t".into(),
            source: "report.rmd".into(),
            files: names,
        };
        let refused =
            super::request_shape(Some(&folder), &Default::default(), &Default::default(), &[]);
        assert!(refused.is_err());
    }

    #[test]
    fn a_list_of_widths_is_read_into_at_most_one_entry_over_what_a_check_takes() {
        use super::{MAX_REQUEST_WIDTHS, bounded, widths_shape};
        let list = format!(
            "[{}]",
            (0..MAX_REQUEST_WIDTHS * 5_000)
                .map(|i| (300 + i % 1000).to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut de = serde_json::Deserializer::from_str(&list);
        let widths = bounded::widths(&mut de).unwrap();
        assert_eq!(widths.len(), MAX_REQUEST_WIDTHS + 1);
        let refused = widths_shape(&widths).unwrap_err();
        assert_eq!(
            (refused.code.as_str(), refused.path.as_str()),
            ("report.input-limit", "widths")
        );
        // A few widths, as a figure's layout uses, are read as written.
        let mut de = serde_json::Deserializer::from_str("[480, 960.5]");
        let few = bounded::widths(&mut de).unwrap();
        assert_eq!(few, [480.0, 960.5]);
        assert!(widths_shape(&few).is_ok());
    }

    #[test]
    fn a_data_name_that_is_a_path_of_a_megabyte_is_refused_before_it_is_normalized() {
        let long = "d/".repeat(300_000);
        let data = std::iter::once((long, "x".to_owned())).collect();
        let refused = super::request_shape(None, &Default::default(), &data, &[]).unwrap_err();
        assert_eq!(refused.code, "report.input-limit");
        assert!(!refused.message.contains("d/d/d/d/"), "{}", refused.message);
        let styles = vec![String::new(); super::MAX_REQUEST_STYLES + 1];
        let refused = super::request_shape(None, &Default::default(), &Default::default(), &styles)
            .unwrap_err();
        assert_eq!(refused.path, "styles");
    }

    #[test]
    fn many_spellings_of_one_file_are_refused_once_their_copies_pass_what_a_request_holds() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("x")).unwrap();
        std::fs::write(dir.path().join("x/a.csv"), vec![b'7'; 1_000_000]).unwrap();
        // One megabyte is counted once, but each of forty spellings is a copy a third larger.
        let links: String = (0..40)
            .map(|n| format!("[l{n}]({}x/a.csv) ", "./".repeat(n + 1)))
            .collect();
        let refused = carried(dir.path(), &links).unwrap_err();
        assert!(refused.contains("report.input-limit"), "{refused}");
        // A few spellings are carried as before.
        let few = carried(dir.path(), "[a](x/a.csv) [b](./x/a.csv)").unwrap();
        assert_eq!(few.len(), 2);
    }
}
