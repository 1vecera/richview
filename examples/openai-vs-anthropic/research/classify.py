"""Label texts with large language models on Amazon Bedrock, batched, cached and reproducible.

Because the subject is two AI labs, the primary judge is made by neither (Qwen3-235B by Alibaba). A shared validation
subset is also labelled by a second neutral judge (Mistral's Devstral 2) and by one judge from each lab (Claude Haiku
4.5, OpenAI gpt-oss-120b), which measures agreement and tests whether a lab's own model rates that lab more kindly.
Labels are model estimates of what each text says, not facts about the companies; every prompt is stored here.
"""

import hashlib
import json
import os
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import boto3
from botocore.config import Config

import common

JUDGES = {
    "qwen3-235b": "qwen.qwen3-235b-a22b-2507-v1:0",
    "devstral-2": "mistral.devstral-2-123b",
    "claude-haiku-4-5": "eu.anthropic.claude-haiku-4-5-20251001-v1:0",
    "gpt-oss-120b": "openai.gpt-oss-120b-1:0",
}
PRIMARY_JUDGE = "qwen3-235b"
# Parallel requests per judge, sized to each model's Bedrock on-demand quota in eu-central-1.
JUDGE_WORKERS = {"qwen3-235b": 8, "devstral-2": 6, "claude-haiku-4-5": 4, "gpt-oss-120b": 6}
BATCH_SIZE = 10
MAX_CHARS = 1800

STANCES = ["positive", "negative", "mixed", "neutral", "absent"]
PREFERENCES = ["openai", "anthropic", "neither", "no_comparison"]
DISCUSSION_TOPICS = [
    "model_quality",
    "coding_tools",
    "pricing_limits",
    "reliability_outages",
    "safety_ethics",
    "business_finance",
    "legal_copyright",
    "government_military",
    "privacy_data",
    "leadership_governance",
    "ads_monetization",
    "open_source",
    "hype_skepticism",
    "product_ux",
    "other",
]
SWITCH_DIRECTIONS = ["to_anthropic", "to_openai", "left_openai", "left_anthropic", "other_switch", "not_a_switch"]
REVIEW_TOPICS = [
    "usage_limits",
    "pricing_subscription",
    "ads",
    "refusals_censorship",
    "accuracy_hallucination",
    "bugs_crashes",
    "login_account",
    "speed",
    "memory_context",
    "voice",
    "images_files",
    "personality_tone",
    "privacy",
    "helpfulness_quality",
    "other",
]


@dataclass(frozen=True)
class Task:
    """One labelling task: its instructions and the validator that keeps only well-formed labels."""

    name: str
    instructions: str
    validate: Callable[[dict[str, Any]], bool]


def valid_stance_label(label: dict[str, Any]) -> bool:
    """Check a discussion-stance label against the allowed values."""
    return (
        label["openai"] in STANCES
        and label["anthropic"] in STANCES
        and label["prefers"] in PREFERENCES
        and all(topic in DISCUSSION_TOPICS for topic in label["topics"])  # type: ignore[union-attr]
    )


def valid_switch_label(label: dict[str, Any]) -> bool:
    """Check a switching label against the allowed values."""
    return label["direction"] in SWITCH_DIRECTIONS and label["first_person"] in (True, False)


def valid_review_label(label: dict[str, Any]) -> bool:
    """Check an app-review label against the allowed values."""
    return all(topic in REVIEW_TOPICS for topic in label["topics"])  # type: ignore[union-attr]


STANCE_TASK = Task(
    name="hn_stance",
    instructions=f"""You annotate public technology discussion about AI companies. Label each Hacker News comment by
what its text says. Do not use outside knowledge about the companies and do not reward or punish either one.

For each comment return an object with:
- "id": copied from the input.
- "openai": the comment's evaluative stance toward OpenAI or its products (ChatGPT, GPT models, Codex, Sora, o-series):
  one of {STANCES}. "neutral" = mentioned without evaluation; "absent" = not mentioned.
- "anthropic": the same toward Anthropic or its products (Claude, Claude Code, Opus, Sonnet, Haiku).
- "prefers": when the comment compares the two, the one it favours: {PREFERENCES}.
- "topics": one or two of {DISCUSSION_TOPICS} naming what the evaluation is about.
Return only a JSON array of these objects, in input order.""",
    validate=valid_stance_label,
)

SWITCH_TASK = Task(
    name="hn_switch",
    instructions=f"""You annotate Hacker News comments that may describe switching between AI providers.
For each comment return an object with:
- "id": copied from the input.
- "direction": one of {SWITCH_DIRECTIONS}.
  to_anthropic = someone moved (or is moving) their use from OpenAI/ChatGPT/Codex/GPT to Anthropic/Claude;
  to_openai = moved from Anthropic/Claude to OpenAI/ChatGPT/Codex/GPT;
  left_openai = cancelled or stopped using OpenAI/ChatGPT without naming Anthropic as the destination;
  left_anthropic = cancelled or stopped using Anthropic/Claude without naming OpenAI as the destination;
  other_switch = a switch involving other providers only, or ambiguous direction;
  not_a_switch = hypothetical, advice, a question, someone else's company policy, or no switch at all.
- "first_person": true if the writer reports their own (or their team's) switch, else false.
Return only a JSON array of these objects, in input order.""",
    validate=valid_switch_label,
)

REVIEW_TASK = Task(
    name="app_review",
    instructions=f"""You annotate app-store reviews of AI assistant apps. For each review return an object with:
- "id": copied from the input.
- "topics": one to three of {REVIEW_TOPICS} naming what drives the reviewer's rating
  (helpfulness_quality = general usefulness or answer quality; usage_limits = message caps, quotas, rate limits).
Return only a JSON array of these objects, in input order.""",
    validate=valid_review_label,
)


def bedrock_client() -> object:
    """Create a Bedrock runtime client from the managed Bedrock credentials (never printed)."""
    return boto3.client(
        "bedrock-runtime",
        region_name="eu-central-1",
        aws_access_key_id=os.environ["DAS_ITEM_AWS_BEDROCK__AWS_ACCESS_KEY_ID"],
        aws_secret_access_key=os.environ["DAS_ITEM_AWS_BEDROCK__AWS_SECRET_ACCESS_KEY"],
        config=Config(retries={"max_attempts": 12, "mode": "adaptive"}, read_timeout=300),
    )


def label_batch(client: object, judge: str, task: Task, batch: list[dict[str, str]]) -> dict[str, dict[str, Any]]:
    """Label one batch with one judge, caching the raw reply; return id → label for well-formed items only."""
    payload = json.dumps([{"id": item["id"], "text": item["text"][:MAX_CHARS]} for item in batch], ensure_ascii=False)
    key = hashlib.sha256(f"{task.instructions}\n{payload}".encode()).hexdigest()[:32]
    cache_path = common.CACHE_DIR / "llm" / judge / task.name / f"{key}.json"
    if cache_path.exists():
        reply = json.loads(cache_path.read_text())["reply"]
    else:
        response = client.converse(  # type: ignore[attr-defined]
            modelId=JUDGES[judge],
            system=[{"text": task.instructions}],
            messages=[{"role": "user", "content": [{"text": payload}]}],
            inferenceConfig={"maxTokens": 4000, "temperature": 0},
        )
        blocks = response["output"]["message"]["content"]
        reply = "".join(block["text"] for block in blocks if "text" in block)
        cache_path.parent.mkdir(parents=True, exist_ok=True)
        cache_path.write_text(
            json.dumps({"judge": judge, "model": JUDGES[judge], "usage": response["usage"], "reply": reply})
        )
    start = reply.find("[")
    try:
        labels = json.JSONDecoder().raw_decode(reply[start:])[0] if start >= 0 else []
    except json.JSONDecodeError:
        labels = []
    wanted = {item["id"] for item in batch}
    result = {}
    for label in labels:
        try:
            if str(label["id"]) in wanted and task.validate(label):
                result[str(label["id"])] = label
        except (KeyError, TypeError):
            continue
    return result


def label_items(judge: str, task: Task, items: list[dict[str, str]]) -> dict[str, dict[str, Any]]:
    """Label every item with one judge in parallel batches and return id → label (malformed replies dropped)."""
    client = bedrock_client()
    batches = [items[start : start + BATCH_SIZE] for start in range(0, len(items), BATCH_SIZE)]
    labels: dict[str, dict[str, Any]] = {}
    with ThreadPoolExecutor(max_workers=JUDGE_WORKERS[judge]) as pool:
        for batch_labels in pool.map(lambda batch: label_batch(client, judge, task, batch), batches):
            labels.update(batch_labels)
    return labels


def usage_totals(judge: str, task: Task) -> dict[str, int]:
    """Sum cached token usage for one judge and task (for the cost ledger)."""
    totals = {"inputTokens": 0, "outputTokens": 0, "calls": 0}
    for path in Path(common.CACHE_DIR / "llm" / judge / task.name).glob("*.json"):
        usage = json.loads(path.read_text())["usage"]
        totals["inputTokens"] += usage["inputTokens"]
        totals["outputTokens"] += usage["outputTokens"]
        totals["calls"] += 1
    return totals
