//! Complete source retention with bounded native textarea windows.
//!
//! The DOM holds only visible(). Rendering, copying and export use full().
//! Byte ranges stay on UTF-8 boundaries and never split a CRLF pair. Large
//! single-line JSON remains editable through multiple byte-bounded windows.

pub const PAGING_THRESHOLD_BYTES: usize = 128 * 1024;
pub const PAGE_LINE_LIMIT: usize = 512;
pub const PAGE_BYTE_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct SourceBuffer {
    source: String,
    line_starts: Vec<usize>,
    page_starts: Vec<usize>,
    page: usize,
}

impl SourceBuffer {
    pub fn new(source: String) -> Self {
        let line_starts = line_starts(&source);
        let mut buffer = Self {
            source,
            line_starts,
            page_starts: Vec::new(),
            page: 0,
        };
        buffer.rebuild_pages(0);
        buffer
    }

    pub fn full(&self) -> &str {
        &self.source
    }

    pub fn visible(&self) -> &str {
        let (start, end, _) = self.visible_bytes();
        &self.source[start..end]
    }

    /// Native textareas expose LF even when the retained source uses CRLF/CR.
    pub fn visible_textarea(&self) -> String {
        normalize_newlines(self.visible())
    }

    pub fn is_paged(&self) -> bool {
        self.source.len() > PAGING_THRESHOLD_BYTES || self.line_starts.len() > PAGE_LINE_LIMIT
    }

    /// One-based first/last visible source line and total source lines.
    /// A final newline creates a final empty line, as it does in a textarea.
    pub fn visible_lines(&self) -> (usize, usize, usize) {
        let (start, end, total_bytes) = self.visible_bytes();
        let first = self.line_at(start) + 1;
        let last = if end == total_bytes {
            self.line_starts.len()
        } else {
            self.line_at(end.saturating_sub(1)) + 1
        };
        (first, last.max(first), self.line_starts.len())
    }

    /// Zero-based UTF-8 byte range and complete source byte length.
    pub fn visible_bytes(&self) -> (usize, usize, usize) {
        (
            self.page_starts[self.page],
            self.page_starts
                .get(self.page + 1)
                .copied()
                .unwrap_or(self.source.len()),
            self.source.len(),
        )
    }

    pub fn has_previous(&self) -> bool {
        self.page > 0
    }

    pub fn has_next(&self) -> bool {
        self.page + 1 < self.page_starts.len()
    }

    /// Replace the complete source, such as an import or example selection.
    pub fn replace_full(&mut self, source: String) {
        self.line_starts = line_starts(&source);
        self.source = source;
        self.rebuild_pages(0);
    }

    /// Apply a textarea edit to its displayed range, preserving hidden bytes.
    /// Only replacement newline offsets are scanned; retained offsets shift
    /// incrementally rather than rescanning the complete source on each edit.
    pub fn update_visible(&mut self, replacement: String) {
        let (start, end, _) = self.visible_bytes();
        let removed = end - start;
        let inserted = replacement.len();
        // Include one byte on either side: edits can join or split a CRLF pair.
        let scan_start = start.saturating_sub(1);
        let old_scan_end = end.saturating_add(1).min(self.source.len());
        let prefix_end = self
            .line_starts
            .partition_point(|&offset| offset <= scan_start);
        let suffix_start = self
            .line_starts
            .partition_point(|&offset| offset <= old_scan_end);
        self.source.replace_range(start..end, &replacement);
        let new_scan_end = old_scan_end - removed + inserted;
        let inserted_starts = newline_offsets(&self.source, scan_start, new_scan_end);
        let shifted_suffix = prefix_end + inserted_starts.len();
        self.line_starts
            .splice(prefix_end..suffix_start, inserted_starts);
        for offset in &mut self.line_starts[shifted_suffix..] {
            if inserted >= removed {
                *offset += inserted - removed;
            } else {
                *offset -= removed - inserted;
            }
        }
        self.rebuild_pages(start.min(self.source.len()));
    }

    /// Preserve untouched original newline bytes while applying a textarea edit.
    pub fn update_textarea(&mut self, value: String) {
        let value = if value.contains('\r') {
            normalize_newlines(&value)
        } else {
            value
        };
        let normalized = self.visible_textarea();
        if normalized == value {
            return;
        }
        let mut prefix = normalized
            .bytes()
            .zip(value.bytes())
            .take_while(|(left, right)| left == right)
            .count();
        while !normalized.is_char_boundary(prefix) || !value.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let mut suffix = normalized[prefix..]
            .bytes()
            .rev()
            .zip(value[prefix..].bytes().rev())
            .take_while(|(left, right)| left == right)
            .count();
        while !normalized.is_char_boundary(normalized.len() - suffix)
            || !value.is_char_boundary(value.len() - suffix)
        {
            suffix -= 1;
        }
        let raw = self.visible();
        let raw_start = raw_offset(raw, prefix);
        let raw_end = raw_offset(raw, normalized.len() - suffix);
        let inserted = &value[prefix..value.len() - suffix];
        let style = newline_style(raw, raw_start);
        let inserted = if style == "\n" {
            inserted.to_string()
        } else {
            inserted.replace('\n', style)
        };
        let replacement = format!("{}{}{}", &raw[..raw_start], inserted, &raw[raw_end..]);
        self.update_visible(replacement);
    }

    /// Start a window at the requested line, clamped to the complete document.
    /// Reanchoring preserves a continuous partition on both sides of the jump.
    pub fn go_to_line(&mut self, line: usize) {
        let index = line.saturating_sub(1).min(self.line_starts.len() - 1);
        self.rebuild_pages(self.line_starts[index]);
    }

    pub fn previous_page(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    pub fn next_page(&mut self) {
        if self.has_next() {
            self.page += 1;
        }
    }

    fn line_at(&self, position: usize) -> usize {
        self.line_starts
            .partition_point(|&offset| offset <= position)
            .saturating_sub(1)
    }

    fn forward_end(&self, start: usize) -> usize {
        let line_end = self
            .line_starts
            .get(self.line_at(start) + PAGE_LINE_LIMIT)
            .copied()
            .unwrap_or(self.source.len());
        let byte_end = start.saturating_add(PAGE_BYTE_LIMIT).min(self.source.len());
        let mut end = line_end.min(byte_end);
        if end < self.source.len() {
            while !self.source.is_char_boundary(end) {
                end -= 1;
            }
            if end > start
                && self.source.as_bytes()[end - 1] == b'\r'
                && self.source.as_bytes()[end] == b'\n'
            {
                end -= 1;
            }
            // Prefer whole lines when they fit, but keep long lines navigable.
            let line_boundary = self.line_starts[self.line_at(end)];
            if line_boundary > start {
                end = line_boundary;
            }
        }
        end
    }

    fn backward_start(&self, end: usize) -> usize {
        let last_line = self.line_at(end.saturating_sub(1));
        let line_start = self.line_starts[last_line.saturating_sub(PAGE_LINE_LIMIT - 1)];
        let byte_start = end.saturating_sub(PAGE_BYTE_LIMIT);
        let mut start = line_start.max(byte_start);
        while !self.source.is_char_boundary(start) {
            start += 1;
        }
        if start > 0
            && start < end
            && self.source.as_bytes()[start - 1] == b'\r'
            && self.source.as_bytes()[start] == b'\n'
        {
            start += 1;
        }
        let next_line = self.line_starts.partition_point(|&offset| offset < start);
        if let Some(&line_boundary) = self.line_starts.get(next_line)
            && line_boundary < end
        {
            start = line_boundary;
        }
        start
    }

    fn rebuild_pages(&mut self, anchor: usize) {
        let mut anchor = anchor.min(self.source.len());
        while !self.source.is_char_boundary(anchor) {
            anchor -= 1;
        }
        // An edit can join a retained CR and a newly inserted LF at the anchor.
        if anchor > 0
            && anchor < self.source.len()
            && self.source.as_bytes()[anchor - 1] == b'\r'
            && self.source.as_bytes()[anchor] == b'\n'
        {
            anchor -= 1;
        }
        self.page_starts.clear();
        self.page = 0;
        if !self.is_paged() {
            self.page_starts.push(0);
            return;
        }
        let mut left = Vec::new();
        let mut end = anchor;
        while end > 0 {
            let start = self.backward_start(end);
            debug_assert!(start < end);
            left.push(start);
            end = start;
        }
        left.reverse();
        self.page = left.len();
        self.page_starts = left;
        self.page_starts.push(anchor);
        let mut start = anchor;
        while start < self.source.len() {
            let end = self.forward_end(start);
            debug_assert!(end > start);
            if end == self.source.len() {
                break;
            }
            self.page_starts.push(end);
            start = end;
        }
    }
}

fn normalize_newlines(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

fn raw_offset(raw: &str, normalized_offset: usize) -> usize {
    let bytes = raw.as_bytes();
    let mut raw_index = 0;
    let mut normalized_index = 0;
    while normalized_index < normalized_offset {
        if bytes[raw_index] == b'\r' && bytes.get(raw_index + 1) == Some(&b'\n') {
            raw_index += 2;
        } else {
            raw_index += 1;
        }
        normalized_index += 1;
    }
    raw_index
}

fn newline_style(raw: &str, position: usize) -> &'static str {
    let bytes = raw.as_bytes();
    let style_at = |index: usize| match bytes[index] {
        b'\r' if bytes.get(index + 1) == Some(&b'\n') => Some("\r\n"),
        b'\r' => Some("\r"),
        b'\n' if index > 0 && bytes[index - 1] == b'\r' => Some("\r\n"),
        b'\n' => Some("\n"),
        _ => None,
    };
    for index in position..bytes.len() {
        if let Some(style) = style_at(index) {
            return style;
        }
    }
    for index in (0..position).rev() {
        if let Some(style) = style_at(index) {
            return style;
        }
    }
    "\n"
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    offsets.extend(newline_offsets(source, 0, source.len()));
    offsets
}

fn newline_offsets(source: &str, start: usize, end: usize) -> Vec<usize> {
    let bytes = source.as_bytes();
    (start..end)
        .filter_map(|index| {
            (bytes[index] == b'\n'
                || (bytes[index] == b'\r' && bytes.get(index + 1) != Some(&b'\n')))
            .then_some(index + 1)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn large_lines() -> String {
        (0..4000)
            .map(|index| format!("řádek {index:04}: {}\r\n", "žlutý😀".repeat(6)))
            .collect()
    }

    fn assert_window(buffer: &SourceBuffer) {
        assert_eq!(
            buffer.line_starts,
            line_starts(buffer.full()),
            "Incremental line offsets must match a complete rescan"
        );
        let (start, end, total) = buffer.visible_bytes();
        assert!(buffer.full().is_char_boundary(start));
        assert!(buffer.full().is_char_boundary(end));
        assert_eq!(buffer.visible(), &buffer.full()[start..end]);
        assert_eq!(total, buffer.full().len());
        if buffer.is_paged() {
            assert!(end - start <= PAGE_BYTE_LIMIT);
            assert!(
                buffer
                    .visible()
                    .bytes()
                    .filter(|&byte| byte == b'\n')
                    .count()
                    <= PAGE_LINE_LIMIT
            );
        }
        for boundary in [start, end] {
            if boundary > 0 && boundary < total {
                assert_ne!(&buffer.full().as_bytes()[boundary - 1..=boundary], b"\r\n");
            }
        }
        assert_eq!(
            buffer.visible_lines().2,
            normalize_newlines(buffer.full())
                .bytes()
                .filter(|&byte| byte == b'\n')
                .count()
                + 1
        );
    }

    #[test]
    fn small_documents_and_empty_final_lines_remain_exact() {
        for source in ["", "a", "ž😀\r\nb\n", "\n\n", "one\r\n"] {
            let mut buffer = SourceBuffer::new(source.into());
            assert!(!buffer.is_paged());
            assert_eq!(buffer.full(), source);
            assert_eq!(buffer.visible(), source);
            assert!(!buffer.has_previous() && !buffer.has_next());
            assert_window(&buffer);
            buffer.go_to_line(usize::MAX);
            assert_eq!(buffer.visible(), source);
        }
    }

    #[test]
    fn every_page_round_trip_preserves_all_unicode_crlf_source() {
        let source = large_lines();
        let mut buffer = SourceBuffer::new(source.clone());
        assert!(buffer.is_paged());
        let mut gathered = String::new();
        let mut ranges = Vec::new();
        loop {
            assert_window(&buffer);
            ranges.push(buffer.visible_bytes());
            gathered.push_str(buffer.visible());
            if !buffer.has_next() {
                break;
            }
            buffer.next_page();
        }
        assert_eq!(gathered, source);
        for expected in ranges.into_iter().rev() {
            assert_eq!(buffer.visible_bytes(), expected);
            buffer.previous_page();
        }
        assert_eq!(buffer.full(), source);
    }

    #[test]
    fn middle_page_edits_preserve_hidden_prefix_suffix_and_line_index() {
        let mut buffer = SourceBuffer::new(large_lines());
        buffer.go_to_line(1301);
        assert_eq!(buffer.visible_lines().0, 1301);
        let before = buffer.full().to_string();
        let (start, end, _) = buffer.visible_bytes();
        let replacement = format!(
            "新á😀 line\r\n{}",
            buffer.visible().replacen("řádek", "Edited", 1)
        );
        let expected = format!("{}{}{}", &before[..start], replacement, &before[end..]);
        buffer.update_visible(replacement);
        assert_eq!(buffer.full(), expected);
        assert_window(&buffer);
        buffer.go_to_line(1302);
        assert_eq!(buffer.visible_lines().0, 1302);
        assert!(buffer.visible().starts_with("Edited"));
        buffer.update_visible(String::new());
        assert_window(&buffer);
    }

    #[test]
    fn long_single_line_is_fully_navigable_and_editable_by_byte_segments() {
        let source = format!("{}\r\n{}", "ž😀".repeat(40000), "終".repeat(50000));
        let mut buffer = SourceBuffer::new(source.clone());
        let mut gathered = String::new();
        let mut segments_in_first_line = 0;
        loop {
            assert_window(&buffer);
            if buffer.visible_lines().0 == 1 {
                segments_in_first_line += 1;
            }
            gathered.push_str(buffer.visible());
            if !buffer.has_next() {
                break;
            }
            buffer.next_page();
        }
        assert!(segments_in_first_line > 1);
        assert_eq!(gathered, source);
        buffer.go_to_line(2);
        assert_eq!(buffer.visible_lines().0, 2);
        assert!(buffer.visible().starts_with('終'));
        buffer.previous_page();
        assert!(buffer.visible_bytes().0 > 0);
        let (start, end, _) = buffer.visible_bytes();
        let expected = format!("{}edit😀\n{}", &source[..start], &source[end..]);
        buffer.update_visible("edit😀\n".into());
        assert_eq!(buffer.full(), expected);
        assert_window(&buffer);
    }

    #[test]
    fn byte_cap_never_splits_crlf_and_trailing_empty_line_accepts_edits() {
        let source = format!(
            "{}\r\n{}\n",
            "x".repeat(PAGE_BYTE_LIMIT - 1),
            "y".repeat(160000)
        );
        let mut buffer = SourceBuffer::new(source.clone());
        loop {
            assert_window(&buffer);
            if !buffer.has_next() {
                break;
            }
            buffer.next_page();
        }
        buffer.go_to_line(3);
        assert_eq!(buffer.visible(), "");
        assert_eq!(buffer.visible_lines(), (3, 3, 3));
        buffer.update_visible("last😀\r\n".into());
        assert_eq!(buffer.full(), format!("{source}last😀\r\n"));
        assert_window(&buffer);
    }

    #[test]
    fn complete_replacement_resets_paging_without_retaining_old_hidden_data() {
        let mut buffer = SourceBuffer::new(large_lines());
        buffer.go_to_line(1700);
        buffer.replace_full("new source\r\n😀".into());
        assert_eq!(buffer.full(), "new source\r\n😀");
        assert_eq!(buffer.visible(), buffer.full());
        assert!(!buffer.is_paged());
        assert_window(&buffer);
        buffer.update_visible("entire replacement\n".into());
        assert_eq!(buffer.full(), "entire replacement\n");
    }

    #[test]
    fn many_short_lines_are_bounded_even_when_source_bytes_are_small() {
        let source = "\n".repeat(100000);
        assert!(source.len() < PAGING_THRESHOLD_BYTES);
        let mut buffer = SourceBuffer::new(source.clone());
        assert!(buffer.is_paged());
        assert_eq!(buffer.visible().len(), PAGE_LINE_LIMIT);
        let mut gathered = String::new();
        loop {
            assert_window(&buffer);
            gathered.push_str(buffer.visible());
            if !buffer.has_next() {
                break;
            }
            buffer.next_page();
        }
        assert_eq!(gathered, source);
    }

    #[test]
    fn edit_joining_crlf_reanchors_without_splitting_the_new_pair() {
        let source = format!(
            "{}\r{}",
            "x".repeat(PAGE_BYTE_LIMIT - 1),
            "z".repeat(200000)
        );
        let mut buffer = SourceBuffer::new(source.clone());
        buffer.next_page();
        let (start, end, _) = buffer.visible_bytes();
        assert_eq!(start, PAGE_BYTE_LIMIT);
        let replacement = format!("\n{}", buffer.visible());
        let expected = format!("{}{}{}", &source[..start], replacement, &source[end..]);
        buffer.update_visible(replacement);
        assert_eq!(buffer.full(), expected);
        assert_window(&buffer);
        assert_eq!(buffer.visible_bytes().0, PAGE_BYTE_LIMIT - 1);
    }

    #[test]
    fn edits_join_and_split_crlf_at_both_retained_edges() {
        let source = format!(
            "{}\r{}\n{}",
            "x".repeat(PAGE_BYTE_LIMIT - 1),
            "z".repeat(PAGE_BYTE_LIMIT),
            "q".repeat(200000)
        );
        let mut buffer = SourceBuffer::new(source.clone());
        buffer.next_page();
        let (start, end, _) = buffer.visible_bytes();
        assert_eq!(start, PAGE_BYTE_LIMIT);
        assert_eq!(end, PAGE_BYTE_LIMIT * 2);
        let replacement = format!("\n{}\r", buffer.visible());
        buffer.update_visible(replacement.clone());
        assert_eq!(
            buffer.full(),
            format!("{}{}{}", &source[..start], replacement, &source[end..])
        );
        assert_window(&buffer);
        assert_eq!(buffer.line_starts.len(), 3);

        // The joined prefix pair now occupies a complete small window. Remove
        // its LF while retaining the CR, then its CR while retaining the LF.
        assert_eq!(buffer.visible(), "\r\n");
        buffer.update_visible("\r".into());
        assert_window(&buffer);
        assert_eq!(buffer.line_starts.len(), 3);
        buffer.update_visible("\n".into());
        assert_window(&buffer);
        assert_eq!(buffer.line_starts.len(), 3);
        buffer.update_visible(String::new());
        assert_window(&buffer);
        assert_eq!(buffer.line_starts.len(), 2);
    }

    #[test]
    fn cached_offsets_match_complete_rescan_after_mixed_boundary_edits() {
        let source: String = (0..2400)
            .map(|index| {
                let ending = ["\r", "\n", "\r\n"][index % 3];
                format!("row {index}: {}{ending}", "ž😀".repeat(16))
            })
            .collect();
        let mut buffer = SourceBuffer::new(source);
        let mut seed = 12345_usize;
        for iteration in 0..64 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            buffer.go_to_line(seed % buffer.line_starts.len() + 1);
            let (start, end, _) = buffer.visible_bytes();
            let before = buffer.full().to_string();
            let visible = buffer.visible();
            let body = if iteration % 4 == 0 {
                visible.trim_matches(['\r', '\n'])
            } else {
                visible
            };
            let replacement = format!(
                "{}{body}{}",
                ["", "\n", "\r", "\r\n"][iteration % 4],
                ["\r", "\n", "", "\r\n"][iteration % 4]
            );
            let expected = format!("{}{}{}", &before[..start], replacement, &before[end..]);
            buffer.update_visible(replacement);
            assert_eq!(buffer.full(), expected);
            assert_window(&buffer);
        }
    }

    #[test]
    fn textarea_edits_preserve_untouched_mixed_newlines_and_unicode() {
        let source = format!("first\r\nsecond\rthird\n{}", large_lines());
        let mut buffer = SourceBuffer::new(source.clone());
        let normalized = buffer.visible_textarea();
        buffer.update_textarea(normalized.clone());
        assert_eq!(
            buffer.full(),
            source,
            "Unchanged normalized input preserves exact raw bytes"
        );
        buffer.update_textarea(normalized.replacen("second", "新😀 second", 1));
        assert_eq!(buffer.full(), source.replacen("second", "新😀 second", 1));
        assert_window(&buffer);
        let before = buffer.full().to_string();
        buffer.go_to_line(1400);
        let normalized = buffer.visible_textarea();
        let edited = normalized.replacen("řádek", "改😀", 1);
        buffer.update_textarea(edited);
        let expected = before.replacen("řádek 1396", "改😀 1396", 1);
        assert_eq!(buffer.full(), expected);
        assert_window(&buffer);
    }

    #[test]
    fn inserted_textarea_line_uses_original_line_ending_style() {
        let source = "first\r\nlast😀\r\n".to_string();
        let mut buffer = SourceBuffer::new(source);
        buffer.update_textarea("first\ninserted😀\nlast😀\n".into());
        assert_eq!(buffer.full(), "first\r\ninserted😀\r\nlast😀\r\n");
        assert_window(&buffer);
        buffer.update_textarea("first\ninserted😃\nlast😀\n".into());
        assert_eq!(buffer.full(), "first\r\ninserted😃\r\nlast😀\r\n");
    }
}
