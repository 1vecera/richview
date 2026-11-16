"""Check the dated image revision offline, including source bytes, data and redistribution notices.

Run from research: uv run --frozen verify_images.py. This never fetches source material.
"""

import csv
import hashlib
import json
import xml.etree.ElementTree as ET
from pathlib import Path

from build_cover import content_digest

ROOT = Path(__file__).resolve().parent.parent


def csv_rows(relative: str) -> list[dict[str, str]]:
    with (ROOT / relative).open(newline="") as handle:
        return list(csv.DictReader(handle))


def digest(relative: str) -> str:
    return hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()


def main() -> None:
    rows = csv_rows("ledger/image-sources.csv")
    expected = {"I-COVER", "I-AUDIENCES", "I-RATINGS", "I-CODEX", "I-COMPUTER-USE"}
    # Retired images stay byte-identical for revision history but are no longer in the report.
    retired = {"I-AUDIENCES", "I-RATINGS"}
    assert len(rows) == len(expected) and {row["id"] for row in rows} == expected
    report = (ROOT / "report.rmd").read_text()
    assert "date: '2026-10-02'" in report or "date: 2026-10-02" in report
    charts = json.loads((ROOT / "data/charts/index.json").read_text())
    assert report.count("```chart\n") == len(charts) == 9
    assert "media/" not in report, "The active report still references a replaced stock photograph"
    for row in rows:
        if row["id"] in retired:
            assert row["asset"] not in report and "Retired" in row["changes"], row["id"]
        else:
            assert row["asset"] in report and row["id"] in report, row["id"]
        assert digest(row["asset"]) == row["sha256"], row["id"]
        assert row["creator"] and row["license"] and row["source_date"] and row["source_url"]
        assert row["collected_at"][:10] in {"2026-10-15", "2026-10-23", "2026-11-09"}, row["id"]
        for notice in row["local_notices"].split(";"):
            assert len((ROOT / notice).read_text()) > 100, notice
    assert report.count("reproduced unchanged") == 2
    originals = {
        "I-CODEX": (
            "ab753387ccf54aba94ab8c54da3149c743dbe189",
            "15b86fa9c0790a779ecdd84f1e7dee029ab79bfb093bd3c4876998696925b013",
        ),
        "I-COMPUTER-USE": (
            "b03d42cc109ef2a61c65305ac2fb8b293bbdac71",
            "c79ac003fd72b053805c35cc2daeba48de64d6a2c822cab09f03f9b3e2ac4ac4",
        ),
    }
    for row in rows:
        if row["id"] in originals:
            commit, sha = originals[row["id"]]
            assert commit in row["source_url"] and commit in row["license_url"]
            assert row["sha256"] == sha, "A source screenshot changed"
    cover = ET.parse(ROOT / "assets/cover-two-publics.svg").getroot()
    cover_metadata = cover.findtext("{http://www.w3.org/2000/svg}metadata") or ""
    for source in ("data/charts/population-shares.csv", "data/facts.json"):
        assert content_digest(source) in cover_metadata, f"The cover was not built from the committed {source}"
    manifest = json.loads((ROOT / "assets/editorial-charts.json").read_text())
    assert manifest["snapshot"] == "2026-10-02" and manifest["imageRevision"] == "2026-10-15"
    assert manifest["source_sha256"] == digest(manifest["source"])
    populations = {row["measure"]: row for row in csv_rows(manifest["source"])}
    audience, ratings = manifest["assets"]
    for item, measure in zip(
        audience["chart"]["data"]["inline"], ("US iOS ratings", "Firms paying", "Agent at work"), strict=True
    ):
        assert item["share"] == populations[measure]["anthropic_share_pct"]
        assert item["source"] == populations[measure]["source_id"]
    counts = populations["US iOS ratings"]
    # The shortened report (15 November) quotes the exact counts in the cover's text alternative; its
    # scorecard gives their ratio.
    for count in (counts["openai_value"], counts["anthropic_value"]):
        assert f"{int(count):,}" in report, "The report lost an exact rating count"
    assert ratings["chart"]["data"]["inline"] == [
        {"product": "ChatGPT", "ratings": counts["openai_value"]},
        {"product": "Claude", "ratings": counts["anthropic_value"]},
    ]
    for asset in manifest["assets"]:
        relative = "assets/" + asset["file"]
        assert asset["sha256"] == digest(relative)
        svg = ET.parse(ROOT / relative).getroot()
        assert list(map(float, svg.attrib["viewBox"].split()[2:])) == [asset["width"], asset["height"]]
        namespace = {"s": "http://www.w3.org/2000/svg"}
        metadata = svg.findtext("s:metadata", namespaces=namespace)
        assert metadata and manifest["source_sha256"] in metadata
        assert manifest["runtime_sha256"] in metadata
        for notice in ("richview-MIT.txt", "source-serif-4-OFL.txt", "source-code-pro-OFL.txt"):
            assert (ROOT / "assets/licenses" / notice).read_text() in metadata
        fonts = svg.findtext("s:style", namespaces=namespace)
        assert fonts and fonts.count("@font-face") == 3
        assert fonts.count("data:font/") == 3
        assert all(
            "http" not in value for node in svg.iter() for key, value in node.attrib.items() if key.endswith("href")
        )
        assert not svg.findall(".//s:script", namespaces=namespace)
    runs = [row for row in csv_rows("ledger/collection-runs.csv") if row["run_id"] == "editorial-images-2026-10-15"]
    assert len(runs) == 1 and runs[0]["items"] == "4"
    print(
        json.dumps(
            {
                "status": "passed",
                "assets": [{"id": row["id"], "sha256": row["sha256"]} for row in rows],
                "snapshot": manifest["snapshot"],
                "image_revision": manifest["imageRevision"],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
