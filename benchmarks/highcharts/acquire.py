# /// script
# requires-python = ">=3.13"
# dependencies = ["beautifulsoup4==4.14.3", "httpx==0.28.1", "pillow==12.1.1"]
# ///
"""Download and verify the official gallery's chart thumbnails for local visual review."""

import argparse
import hashlib
import html
import io
import json
from concurrent.futures import ThreadPoolExecutor
from datetime import UTC, datetime
from pathlib import Path

import httpx
from bs4 import BeautifulSoup
from PIL import Image, ImageStat

GALLERY = "https://www.highcharts.com/demo"


def acquire(evidence: Path, count: int) -> None:
    """Keep downloaded bytes, provenance and an inspectable, hash-deduplicated contact sheet together."""
    reference = evidence / "reference"
    pictures = reference / "pictures"
    pictures.mkdir(parents=True, exist_ok=True)
    with httpx.Client(follow_redirects=True, timeout=30) as client:
        response = client.get(GALLERY)
        response.raise_for_status()
        (reference / "gallery.html").write_text(response.text)
        soup = BeautifulSoup(response.text, "html.parser")
        listing = json.loads(soup.find("script", id="__NEXT_DATA__").string)
        demos = {
            demo["thumbnailURL"]: demo
            for section in listing["props"]["pageProps"]["sectionCategoryDemos"]
            for demo in section["demos"]
            if "/samples/highcharts/demo/" in demo["thumbnailURL"]
        }

        def download(demo: dict) -> dict:
            """Verify both the official demo page and nonempty decoded chart picture."""
            slug = demo["location"].split("/")[-1]
            demo_url = f"{GALLERY}/highcharts/{slug}"
            page = client.get(demo_url)
            page.raise_for_status()
            if slug not in page.text or "__NEXT_DATA__" not in page.text:
                raise ValueError(f"Unverified demo: {demo_url}")
            image_url = demo["thumbnailURL"]
            picture = client.get(image_url)
            picture.raise_for_status()
            raster = Image.open(io.BytesIO(picture.content)).convert("RGB")
            if min(raster.size) < 120 or max(ImageStat.Stat(raster).stddev) < 8:
                raise ValueError(f"Empty or decorative picture: {image_url}")
            file = pictures / f"{slug}.webp"
            file.write_bytes(picture.content)
            return {
                "chart": demo["name"], "state": "official gallery thumbnail, resting",
                "demo_url": demo_url, "verified_demo_url": str(page.url),
                "image_url": image_url, "local_image_path": str(file.resolve()),
                "width": raster.width, "height": raster.height,
                "sha256": hashlib.sha256(picture.content).hexdigest(),
                "pixel_sha256": hashlib.sha256(raster.tobytes()).hexdigest(),
                "bytes": len(picture.content), "captured_at": datetime.now(UTC).isoformat(),
                "provenance": "HTTP 200 download of chart thumbnail from official gallery JSON; demo HTTP 200 verified; Pillow decoded RGB pixels, dimensions and nonempty variance checked",
                "pixel_stddev": ImageStat.Stat(raster).stddev,
            }

        records, failures, hashes = [], [], set()
        # Four bounded requests, no site crawl or decorative assets. Download a buffer
        # above the target so duplicate/error exclusions cannot inflate the count.
        with ThreadPoolExecutor(max_workers=4) as executor:
            futures = [(demo, executor.submit(download, demo)) for demo in list(demos.values())[: count + 15]]
            for demo, future in futures:
                try:
                    item = future.result()
                    if item["pixel_sha256"] in hashes:
                        failures.append({"chart": demo["name"], "reason": "duplicate decoded pixels"})
                        continue
                    hashes.add(item["pixel_sha256"])
                    records.append(item)
                    print(f"verified {len(records)}: {item['chart']}", flush=True)
                except (httpx.HTTPError, ValueError, OSError) as error:
                    failures.append({"chart": demo["name"], "reason": str(error)})
        index = {"gallery": GALLERY, "distinct_verified_pictures": len(records), "pictures": records, "excluded": failures}
        (reference / "index.json").write_text(json.dumps(index, indent=2) + "\n")
        cards = "\n".join(
            f'<figure><a href="{html.escape(item["demo_url"])}"><img loading="lazy" src="pictures/{Path(item["local_image_path"]).name}" alt="{html.escape(item["chart"])}"></a><figcaption>{html.escape(item["chart"])} · {item["width"]} × {item["height"]}<br><a href="pictures/{Path(item["local_image_path"]).name}">Local picture</a> · <a href="{html.escape(item["demo_url"])}">Official demo</a></figcaption></figure>'
            for item in records
        )
        (reference / "index.html").write_text(
            '<!doctype html><html lang="en"><meta charset="utf-8"><title>Official Highcharts reference corpus</title>'
            '<style>body{font:15px system-ui;margin:24px;background:#f4f4f6}main{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:16px}figure{margin:0;background:white;padding:12px}img{width:100%;height:190px;object-fit:contain}figcaption{margin-top:12px}a{color:#2452a0}</style>'
            f'<h1>{len(records)} verified gallery pictures</h1><p>Local reference review only. Highcharts/Highsoft retain rights. <a href="index.json">Provenance, dimensions and hashes</a>.</p><main>{cards}</main></html>'
        )
        if len(records) < count:
            raise ValueError(f"Only {len(records)} verified pictures; required {count}. See excluded entries.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--count", type=int, default=110)
    args = parser.parse_args()
    acquire(args.evidence.resolve(), args.count)
