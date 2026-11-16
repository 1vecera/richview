"""Record a short showcase of the rendered report: a headless, deterministic scroll with section captions.

Usage: uv run --with playwright record_showcase.py <report.html> <output.mp4> [chromium-path]
Frames use a fixed rate and deterministic scroll path (no wall-clock screen recording); browser and font versions
can affect rasterization. The headless capture never touches the user's desktop. Captions quote data/facts.json, so they cannot drift from the report.
ffmpeg assembles the frames into an H.264 MP4.
"""

import json
import math
import subprocess
import sys
import tempfile
from pathlib import Path

from playwright.sync_api import Page, ViewportSize, sync_playwright

FPS = 30
VIEWPORT: ViewportSize = {"width": 1280, "height": 720}
FACTS = json.loads((Path(__file__).resolve().parent.parent / "data" / "facts.json").read_text())
# (heading the stop scrolls to, caption template with {fact_key} placeholders, seconds to hold)
STOPS = [
    ("The verdict", "Anthropic is winning the people who build with AI. OpenAI still has everyone else.", 3.5),
    (
        "One race, many publics",
        "One question, many publics: Anthropic's share runs from {population_share_low} to {population_share_high}",
        3.0,
    ),
    (
        "The shift travelled from builders",
        "Developers moved first; buyers and revenue followed about nine months later",
        3.0,
    ),
    ("The values moment", "February 2026: the Super Bowl ad and the Pentagon split", 2.5),
    (
        "Among developers, Anthropic's halo is gone",
        "Developer net stance: Anthropic {hn_net_anthropic_latest}, OpenAI {hn_net_openai_latest} in Q3 2026",
        3.0,
    ),
    ("The next fight is over coding agents", "Codex now leads Claude Code in developer talk and searches", 3.0),
    (
        "Consumers: OpenAI's moat holds",
        "Probability samples: ChatGPT reaches {survey_reach_ratio_min} to "
        "{survey_reach_ratio_max} times as many people",
        2.5,
    ),
    ("Money: Anthropic's crossover", "Q2 2026 revenue: {q2_2026_revenue_anthropic} vs {q2_2026_revenue_openai}", 2.5),
    (
        "Capability: a tie, with specialties",
        "Composite capability: {eci_anthropic_latest} vs {eci_openai_latest}, inside the error",
        2.5,
    ),
    ("Who is winning, by dimension", "Every claim is dated, sourced and rebuilt from public data", 3.5),
]


def caption_text(template: str) -> str:
    """Fill a caption template with the display values from facts.json."""
    return template.format(**{key: value["display"] for key, value in FACTS.items()})


CAPTION_CSS = """
#showcase-caption { position: fixed; left: 50%; bottom: 28px; transform: translateX(-50%); z-index: 9999;
  max-width: 980px; padding: 12px 20px; border-radius: 10px; font: 600 20px/1.35 system-ui, sans-serif;
  color: #fff; background: rgba(20, 20, 28, 0.82); box-shadow: 0 6px 24px rgba(0,0,0,.25); text-align: center; }
"""


def set_caption(page: Page, text: str) -> None:
    """Show or replace the overlay caption (video only; the report itself is untouched)."""
    page.evaluate(
        """text => { let el = document.getElementById('showcase-caption');
        if (!el) { el = document.createElement('div'); el.id = 'showcase-caption'; document.body.appendChild(el); }
        el.textContent = text; }""",
        text,
    )


def heading_offset(page: Page, text: str) -> int:
    """Document y offset of the first heading whose text starts with the given string."""
    return int(
        page.evaluate(
            """text => { const h = [...document.querySelectorAll('h1, h2, h3')]
              .find(e => e.textContent.trim().startsWith(text));
        return h ? h.getBoundingClientRect().top + window.scrollY - 24 : 0; }""",
            text,
        )
    )


def capture(page: Page, frames_dir: Path, index: int) -> int:
    """Save one frame and return the next frame index."""
    page.screenshot(path=str(frames_dir / f"{index:05d}.png"))
    return index + 1


def main(html: Path, output: Path, chromium: str | None) -> None:
    """Scroll through each stop with eased motion, hold, caption, and encode the frames to MP4."""
    scratch = Path.cwd() / "tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as tmp, sync_playwright() as playwright:
        frames_dir = Path(tmp)
        browser = playwright.chromium.launch(headless=True, executable_path=chromium)
        page = browser.new_page(viewport=VIEWPORT, device_scale_factor=1)
        page.emulate_media(reduced_motion="reduce")
        page.goto(html.resolve().as_uri())
        page.add_style_tag(content=CAPTION_CSS)
        page.wait_for_function("document.querySelectorAll('.rvc-stage').length === 18", timeout=45_000)
        page.evaluate("document.fonts.ready")
        page.evaluate("() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))")
        index = 0
        current = 0
        for text, caption, hold in STOPS:
            target = heading_offset(page, text)
            steps = int(FPS * 1.2)
            for step in range(1, steps + 1):
                eased = 0.5 - 0.5 * math.cos(math.pi * step / steps)
                page.evaluate("y => window.scrollTo(0, y)", int(current + (target - current) * eased))
                index = capture(page, frames_dir, index)
            current = target
            set_caption(page, caption_text(caption))
            for _ in range(int(FPS * hold)):
                index = capture(page, frames_dir, index)
        browser.close()
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-loglevel",
                "error",
                "-framerate",
                str(FPS),
                "-i",
                str(frames_dir / "%05d.png"),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                "20",
                "-movflags",
                "+faststart",
                str(output),
            ],
            check=True,
        )
    print(f"{output} ({index} frames, {index / FPS:.1f} s)")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3] if len(sys.argv) > 3 else None)
