//! Reader provenance supplied by a trusted host after authored input has been validated.
use richview_core::document::{
    Document, Output, RenderOptions, ReportFolder, parse, render_document,
};
use serde_json::{Value, json};

fn options(output: Output) -> RenderOptions {
    RenderOptions {
        output,
        report_folder: Some(ReportFolder {
            root: "provenance".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    }
}

fn table() -> Value {
    json!({"title":"Ledger","data":{"inline":[{"name":"Alpha","amount":12}]}})
}

fn chart() -> Value {
    json!({"type":"bar","title":"Sales","encoding":{"x":"name","y":"amount"},"data":{"inline":[{"name":"Alpha","amount":12}]}})
}

fn document() -> Document {
    parse(&format!(
        "```table\n{}\n```\n\n```chart\n{}\n```\n",
        table(),
        chart()
    ))
    .unwrap()
}

fn provenance() -> Value {
    json!({
        "connector":"Sales \" <img src=x onerror=alert(1)> & operations",
        "source":"sql",
        "fetched_at":"2026-10-29T15:00:00+01:00",
        "fingerprint":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "row_count":1,
        "stale":false,
        // These are deliberately outside the reader carrier and must not escape through it.
        "query":"SELECT private_query_marker FROM secrets",
        "params":{"private_parameter_marker":"private_parameter_value_marker"},
        "path":"/private/host/cache_path_marker",
        "credential":"private_credential_marker"
    })
}

#[test]
fn trusted_provenance_survives_the_worker_boundary_as_a_safe_reader_disclosure() {
    for output in [Output::Report, Output::Pdf] {
        let mut document = document();
        assert_eq!(document.blocks.len(), 2);
        for block in &mut document.blocks {
            block.value["data"]["_provenance"] = provenance();
        }
        let document = serde_json::from_slice(&serde_json::to_vec(&document).unwrap()).unwrap();
        let report = render_document(document, &options(output)).unwrap();
        assert!(
            !report.diagnostics.iter().any(|d| d.level == "error"),
            "{:?}",
            report.diagnostics
        );
        let notes: Vec<_> = report
            .html
            .split("<details class=\"rv-provenance\"")
            .skip(1)
            .map(|rest| rest.split_once("</details>").unwrap().0)
            .collect();
        assert_eq!(notes.len(), 2, "one disclosure for each actual chart/table");
        assert!(report.html.contains("Alpha") && report.html.contains("<table"));
        for note in notes {
            assert!(note.contains("<summary>Data source</summary>"));
            assert!(
                note.contains("Sales &quot; &lt;img src=x onerror=alert(1)&gt; &amp; operations")
            );
            for field in [
                "<dt>Source</dt><dd>sql</dd>",
                "<code>0123456789ab</code>",
                "datetime=\"2026-10-29T15:00:00+01:00\"",
                "<dt>Rows</dt><dd>1</dd>",
            ] {
                assert!(note.contains(field), "missing {field}: {note}");
            }
            assert_eq!(note.starts_with(" open>"), output == Output::Pdf);
        }
        for private in [
            "private_query_marker",
            "private_parameter_marker",
            "private_parameter_value_marker",
            "cache_path_marker",
            "private_credential_marker",
            "<img src=x onerror=alert(1)>",
        ] {
            assert!(!report.html.contains(private), "leaked {private}");
        }
    }
}

#[test]
fn stale_provenance_is_named_in_the_closed_summary_and_missing_stale_means_current() {
    for (source, stale) in [
        ("sql", Some(false)),
        ("metricflow", None),
        ("cache", Some(true)),
    ] {
        let mut document = document();
        document.blocks.truncate(1);
        let mut provenance = provenance();
        provenance["source"] = json!(source);
        match stale {
            Some(stale) => provenance["stale"] = json!(stale),
            None => {
                provenance.as_object_mut().unwrap().remove("stale");
            }
        }
        document.blocks[0].value["data"]["_provenance"] = provenance;
        let report = render_document(document, &options(Output::Report)).unwrap();
        let note = report
            .html
            .split_once("<details class=\"rv-provenance\"")
            .unwrap()
            .1
            .split_once("</details>")
            .unwrap()
            .0;
        let summary = note
            .split_once("<summary>")
            .unwrap()
            .1
            .split_once("</summary>")
            .unwrap()
            .0;
        assert_eq!(summary.contains("stale"), stale == Some(true));
        assert_eq!(
            note.contains("This snapshot is stale."),
            stale == Some(true)
        );
        assert!(note.contains(&format!("<dt>Source</dt><dd>{source}</dd>")));
    }
}

#[test]
fn authored_internal_provenance_cannot_claim_a_trusted_source() {
    for (tag, mut value) in [("table", table()), ("chart", chart())] {
        value["data"]["_provenance"] = provenance();
        let document = parse(&format!("```{tag}\n{value}\n```\n")).unwrap();
        assert_eq!(document.blocks.len(), 1, "{:?}", document.diagnostics);
        assert!(
            document.blocks[0].value["data"]
                .get("_provenance")
                .is_none()
        );
        assert!(
            document
                .diagnostics
                .iter()
                .any(|d| d.code == "block.unknown-key"
                    && d.path == format!("{tag}.data._provenance"))
        );
        let report = render_document(document, &options(Output::Report)).unwrap();
        assert!(!report.html.contains("class=\"rv-provenance\""));
        assert!(!report.html.contains("private_query_marker"));
    }
}
