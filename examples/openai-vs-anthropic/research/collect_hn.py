"""Measure Hacker News discussion of both companies from the complete public item table.

Source: ClickHouse's public playground copy of Hacker News (hackernews.hackernews, live, every story and comment).
Hacker News is a technology-industry population (developers, founders, investors; English-speaking, US-heavy). It is
complete for that site, which makes counts exact, but it represents nobody beyond its own readers.

Brand matching is case-insensitive whole-word RE2 matching on title + URL + text:
OpenAI side = openai | chatgpt, Anthropic side = anthropic | claude. The 2022 baseline (before Claude existed)
measures name collisions such as Claude Shannon.
"""

import json

import common

OPENAI_RE = r"\\b(openai|chatgpt)\\b"
ANTHROPIC_RE = r"\\b(anthropic|claude)\\b"
LIVE_ITEMS = "deleted = 0 AND dead = 0 AND type IN ('story', 'comment')"

MONTHLY_SQL = f"""
SELECT toStartOfMonth(time) AS month, type,
  count() AS items,
  countIf(match(doc, '{OPENAI_RE}')) AS openai_side,
  countIf(match(doc, '{ANTHROPIC_RE}')) AS anthropic_side,
  countIf(match(doc, '{OPENAI_RE}') AND match(doc, '{ANTHROPIC_RE}')) AS both_sides,
  countIf(match(doc, '\\\\bopenai\\\\b')) AS openai,
  countIf(match(doc, '\\\\bchatgpt\\\\b')) AS chatgpt,
  countIf(match(doc, '\\\\banthropic\\\\b')) AS anthropic,
  countIf(match(doc, '\\\\bclaude\\\\b')) AS claude,
  countIf(match(doc, '\\\\bclaude (shannon|monet|debussy|l[eé]vi|van damme)')) AS claude_people,
  countIf(match(doc, '\\\\b(gemini|deepmind)\\\\b')) AS google_side,
  countIf(match(doc, '\\\\bclaude code\\\\b')) AS claude_code,
  countIf(match(doc, '\\\\bcodex\\\\b')) AS codex,
  uniqExactIf(by, match(doc, '{OPENAI_RE}')) AS openai_authors,
  uniqExactIf(by, match(doc, '{ANTHROPIC_RE}')) AS anthropic_authors,
  uniqExact(by) AS authors,
  countIf(type = 'story' AND match(lower(title), '{OPENAI_RE}')) AS openai_title_stories,
  countIf(type = 'story' AND match(lower(title), '{ANTHROPIC_RE}')) AS anthropic_title_stories,
  sumIf(score, type = 'story' AND match(lower(title), '{OPENAI_RE}')) AS openai_title_points,
  sumIf(score, type = 'story' AND match(lower(title), '{ANTHROPIC_RE}')) AS anthropic_title_points
FROM (
  SELECT time, type, score, title, by, lower(concat(title, ' ', url, ' ', text)) AS doc
  FROM hackernews.hackernews
  WHERE time >= '{{start}}' AND time < '{{end}}' AND {LIVE_ITEMS}
)
GROUP BY month, type ORDER BY month, type
"""

SWITCH_VERBS = (
    "(switch|switched|switching|moved|moving|migrated|migrating|went back|going back|cancel+ed|cancel+ing|unsubscribed"
    "|ditched|dropped)"
)
SWITCH_SQL = f"""
SELECT id, toString(time) AS time, text
FROM hackernews.hackernews
WHERE type = 'comment' AND deleted = 0 AND dead = 0 AND time >= '2023-01-01' AND time < '2026-11-01'
  AND match(lower(text),
    '{SWITCH_VERBS}[^.?!]{{0,40}}\\\\b(openai|chatgpt|anthropic|claude|codex|gpt)')
"""

SAMPLE_SQL = f"""
SELECT id, toString(time) AS time, side, text FROM (
  SELECT id, time, text, side,
    row_number() OVER (PARTITION BY toStartOfMonth(time), side ORDER BY cityHash64(id)) AS rank_in_month
  FROM (
    SELECT id, time, text,
      multiIf(oa AND NOT an, 'openai', an AND NOT oa, 'anthropic', 'both') AS side
    FROM (
      SELECT id, time, text, match(lower(text), '{OPENAI_RE}') AS oa, match(lower(text), '{ANTHROPIC_RE}') AS an
      FROM hackernews.hackernews
      WHERE type = 'comment' AND deleted = 0 AND dead = 0 AND time >= '2023-01-01' AND time < '2026-11-01'
        AND length(text) BETWEEN 80 AND 4000
    ) WHERE oa OR an
  )
) WHERE rank_in_month <= {{per_month}}
ORDER BY time
"""

TOP_STORIES_SQL = f"""
SELECT toString(month) AS month, side, id, title, score, descendants FROM (
  SELECT toStartOfMonth(time) AS month, id, title, score, descendants,
    multiIf(an AND NOT oa, 'anthropic', oa AND NOT an, 'openai', 'both') AS side,
    row_number() OVER (PARTITION BY month, side ORDER BY score DESC) AS rank_in_month
  FROM (
    SELECT time, id, title, score, descendants,
      match(lower(title), '{OPENAI_RE}') AS oa, match(lower(title), '{ANTHROPIC_RE}') AS an
    FROM hackernews.hackernews
    WHERE type = 'story' AND deleted = 0 AND dead = 0 AND time >= '2023-01-01' AND time < '2026-11-01'
  ) WHERE oa OR an
) WHERE rank_in_month <= 5
ORDER BY month, side, score DESC
"""


def main() -> None:
    """Run the monthly, switching-candidate, comment-sample and top-story queries and store their outputs."""
    fetcher = common.CachedFetcher("clickhouse", timeout_s=300.0)
    monthly = []
    for year in range(2022, 2027):
        sql = MONTHLY_SQL.replace("{start}", f"{year}-01-01").replace("{end}", min(f"{year + 1}-01-01", "2026-11-01"))
        monthly.extend(common.clickhouse_rows(fetcher, sql))
    common.write_rows(common.DERIVED_DIR / "hn_monthly_mentions.csv", monthly)

    switch_candidates = common.clickhouse_rows(fetcher, SWITCH_SQL)
    (common.CACHE_DIR / "hn_switch_candidates.jsonl").write_text(
        "\n".join(json.dumps(row) for row in switch_candidates)
    )

    sample = common.clickhouse_rows(fetcher, SAMPLE_SQL.replace("{per_month}", "60"))
    (common.CACHE_DIR / "hn_comment_sample.jsonl").write_text("\n".join(json.dumps(row) for row in sample))
    common.write_rows(
        common.DERIVED_DIR / "hn_comment_sample_ids.csv",
        [{"id": row["id"], "time": row["time"], "side": row["side"]} for row in sample],
    )

    top_stories = common.clickhouse_rows(fetcher, TOP_STORIES_SQL)
    common.write_rows(common.DERIVED_DIR / "hn_top_stories.csv", top_stories)

    common.upsert_collection_run(
        {
            "run_id": "hn-clickhouse",
            "source": "Hacker News items via ClickHouse public playground (hackernews.hackernews)",
            "collector": "research/collect_hn.py",
            "finished_at": common.utc_now(),
            "query": "whole-word openai|chatgpt vs anthropic|claude on title+url+text; stories and comments",
            "window_start": "2022-01-01",
            "window_end": common.WINDOW_END.isoformat(),
            "items": len(monthly) + len(switch_candidates) + len(sample) + len(top_stories),
            "requests": fetcher.requests,
            "cache_hits": fetcher.cache_hits,
            "cost_usd": 0,
            "notes": (
                f"monthly rows={len(monthly)}; switching candidates={len(switch_candidates)}; "
                f"comment sample={len(sample)} (<=60 per month and side, hash-ordered); top stories={len(top_stories)}"
            ),
        }
    )
    print(f"monthly={len(monthly)} switch={len(switch_candidates)} sample={len(sample)} top={len(top_stories)}")


if __name__ == "__main__":
    main()
