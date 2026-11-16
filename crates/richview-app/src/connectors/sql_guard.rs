pub(in crate::connectors) const METADATA_REFUSAL: &str =
    "SQL host metadata and dynamic queries are not available to report sources";

fn check_identifier(name: &str, call: bool) -> Result<(), super::ConnectorError> {
    let name = name.to_ascii_lowercase();
    if name.starts_with("duckdb_")
        || name.starts_with("pragma_")
        || name.starts_with("pg_")
        || name.starts_with("sqlite_")
        || matches!(
            name.as_str(),
            "current_database"
                | "current_catalog"
                | "current_schema"
                | "current_schemas"
                | "information_schema"
        )
        || (call
            && (name.starts_with("json_execute")
                || matches!(
                    name.as_str(),
                    "current_setting" | "getenv" | "getvariable" | "query" | "query_table"
                )))
    {
        return Err(super::ConnectorError::QueryRejected {
            reason: METADATA_REFUSAL.into(),
        });
    }
    Ok(())
}

fn followed_by_call(sql: &str, mut i: usize) -> bool {
    let bytes = sql.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            byte if byte.is_ascii_whitespace() => i += 1,
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && !matches!(bytes[i], b'\r' | b'\n') {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    if bytes.get(i..i + 2) == Some(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if bytes.get(i..i + 2) == Some(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            byte => return byte == b'(',
        }
    }
    false
}

/// Accept one SELECT/WITH query without host metadata, allowing SQL quotes and comments.
/// DuckDB's prepare executes intermediate statements, so the guard runs before preparation.
pub(super) fn single_query(sql: &str) -> Result<&str, super::ConnectorError> {
    scan(sql, true)
}

/// Stored view, macro and generated-column definitions must not hide metadata access.
pub(super) fn metadata_free(sql: &str) -> Result<(), super::ConnectorError> {
    scan(sql, false).map(|_| ())
}

fn scan(sql: &str, query: bool) -> Result<&str, super::ConnectorError> {
    let reject = || super::ConnectorError::QueryRejected {
        reason: "SQL must contain one SELECT or WITH query".into(),
    };
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut first = true;
    let mut end = None;
    // Catalog DDL includes the object's name, which is not a function invocation.
    let prefix = sql.trim_start();
    let mut table_header = !query
        && prefix
            .get(..13)
            .is_some_and(|s| s.eq_ignore_ascii_case("CREATE TABLE "));
    let mut view_header = !query
        && prefix
            .get(..12)
            .is_some_and(|s| s.eq_ignore_ascii_case("CREATE VIEW "));
    while i < bytes.len() {
        match bytes[i] {
            byte if byte.is_ascii_whitespace() => i += 1,
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    if bytes.get(i..i + 2) == Some(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if bytes.get(i..i + 2) == Some(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if depth != 0 {
                    return Err(reject());
                }
            }
            _ if end.is_some() => return Err(reject()),
            _ if first => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                if query
                    && !matches!(
                        sql[start..i].to_ascii_lowercase().as_str(),
                        "select" | "with"
                    )
                {
                    return Err(reject());
                }
                if !table_header && !view_header {
                    check_identifier(&sql[start..i], followed_by_call(sql, i))?;
                }
                first = false;
            }
            b';' => {
                end = Some(i);
                i += 1;
            }
            b'\'' | b'"' => {
                let quote = bytes[i];
                if quote == b'"'
                    && i >= 2
                    && bytes[i - 1] == b'&'
                    && matches!(bytes[i - 2], b'u' | b'U')
                {
                    return Err(super::ConnectorError::QueryRejected {
                        reason: METADATA_REFUSAL.into(),
                    });
                }
                let start = i + 1;
                let escaped = quote == b'\'' && i > 0 && matches!(bytes[i - 1], b'e' | b'E');
                i += 1;
                loop {
                    let Some(&byte) = bytes.get(i) else {
                        return Err(reject());
                    };
                    i += 1;
                    if escaped && byte == b'\\' {
                        if i == bytes.len() {
                            return Err(reject());
                        }
                        i += 1;
                        continue;
                    }
                    if byte == quote {
                        if bytes.get(i) == Some(&quote) {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                }
                if quote == b'"' && !table_header && !view_header {
                    check_identifier(
                        &sql[start..i - 1].replace("\"\"", "\""),
                        followed_by_call(sql, i),
                    )?;
                }
            }
            b'$' => {
                let start = i;
                let mut stop = i + 1;
                while stop < bytes.len()
                    && (bytes[stop].is_ascii_alphanumeric() || bytes[stop] == b'_')
                {
                    stop += 1;
                }
                if bytes.get(stop) == Some(&b'$') {
                    let delimiter = &sql[start..=stop];
                    let rest = &sql[stop + 1..];
                    let Some(close) = rest.find(delimiter) else {
                        return Err(reject());
                    };
                    i = stop + 1 + close + delimiter.len();
                } else {
                    i += 1;
                    // Named parameter identifiers are values, not engine metadata names.
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                }
            }
            0 => return Err(reject()),
            b'(' if table_header => {
                table_header = false;
                i += 1;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' || byte >= 128 => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] >= 128)
                {
                    i += 1;
                }
                if view_header && sql[start..i].eq_ignore_ascii_case("as") {
                    view_header = false;
                }
                if !table_header && !view_header {
                    check_identifier(&sql[start..i], followed_by_call(sql, i))?;
                }
            }
            _ => i += 1,
        }
    }
    if first {
        return Err(reject());
    }
    Ok(&sql[..end.unwrap_or(sql.len())])
}
