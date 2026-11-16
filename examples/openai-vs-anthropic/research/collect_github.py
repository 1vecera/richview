"""Count public GitHub artifacts that coding agents sign, per month, through the GitHub search API.

Each agent leaves a different trace, so the counts measure visible output under each product's default workflow, not
usage: Codex cloud opens pull requests from codex/ branches, Claude Code on the web and its GitHub app use claude/
branches, Claude Code signs PR bodies and commit trailers by default, and Copilot's coding agent uses copilot/ branches.
Compare each agent's trend with itself; compare levels across agents only with that caveat. The query set mirrors and
extends PRarena (github.com/aavetis/PRarena), which tracks the branch- and bot-based agents but not Claude Code.
"""

import subprocess
from datetime import date, timedelta

import common

PR_QUERIES = {
    "codex_prs": "is:pr head:codex/",
    "codex_prs_merged": "is:pr head:codex/ is:merged",
    "claude_branch_prs": "is:pr head:claude/",
    "claude_branch_prs_merged": "is:pr head:claude/ is:merged",
    "claude_signed_prs": 'is:pr "Generated with Claude Code"',
    "claude_signed_prs_merged": 'is:pr "Generated with Claude Code" is:merged',
    "copilot_prs": "is:pr head:copilot/",
    "copilot_prs_merged": "is:pr head:copilot/ is:merged",
    "cursor_prs": "is:pr head:cursor/",
    "jules_prs": "is:pr author:google-labs-jules[bot]",
}
COMMIT_QUERIES = {
    "claude_coauthored_commits": '"Co-Authored-By: Claude"',
    "codex_coauthored_commits": '"Co-authored-by: Codex"',
}
FIRST_MONTH = date(2025, 1, 1)


def month_end(month: date) -> date:
    """Return the last day of the month that starts on the given date."""
    return common.next_month(month) - timedelta(days=1)


def main() -> None:
    """Query monthly total counts for every agent signature and write one long table."""
    token = subprocess.run(["gh", "auth", "token"], capture_output=True, text=True, check=True).stdout.strip()
    headers = {"Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json"}
    # GitHub signals secondary rate limits with 403; back off for minutes, not seconds.
    retry = (403, 429, 500, 502, 503, 504)
    issues = common.CachedFetcher(
        "github-search-issues", min_interval_s=7.0, headers=headers, retry_statuses=retry, backoff_s=30.0
    )
    commits = common.CachedFetcher(
        "github-search-commits", min_interval_s=13.0, headers=headers, retry_statuses=retry, backoff_s=30.0
    )
    rows = []
    for month in common.month_starts(FIRST_MONTH, common.WINDOW_END):
        window = f"{month}..{month_end(month)}"
        for metric, query in PR_QUERIES.items():
            body = issues.get_json(
                "https://api.github.com/search/issues", {"q": f"{query} created:{window}", "per_page": 1}
            )
            rows.append(
                {
                    "month": month.isoformat()[:7],
                    "metric": metric,
                    "count": body["total_count"],
                    "incomplete": body["incomplete_results"],
                }
            )
        for metric, query in COMMIT_QUERIES.items():
            body = commits.get_json(
                "https://api.github.com/search/commits", {"q": f"{query} committer-date:{window}", "per_page": 1}
            )
            rows.append(
                {
                    "month": month.isoformat()[:7],
                    "metric": metric,
                    "count": body["total_count"],
                    "incomplete": body["incomplete_results"],
                }
            )
        print(month.isoformat()[:7], flush=True)
    common.write_rows(common.DERIVED_DIR / "github_agent_artifacts_monthly.csv", rows)
    common.upsert_collection_run(
        {
            "run_id": "github-agent-artifacts",
            "source": "GitHub REST search API (issues/PRs and commits), total_count per month",
            "collector": "research/collect_github.py",
            "finished_at": common.utc_now(),
            "query": "; ".join(f"{name}={query}" for name, query in (PR_QUERIES | COMMIT_QUERIES).items()),
            "window_start": FIRST_MONTH.isoformat(),
            "window_end": common.WINDOW_END.isoformat(),
            "items": len(rows),
            "requests": issues.requests + commits.requests,
            "cache_hits": issues.cache_hits + commits.cache_hits,
            "cost_usd": 0,
            "notes": "Counts visible signed artifacts under default workflows; not comparable as usage across agents.",
        }
    )


if __name__ == "__main__":
    main()
