"""Rotate a dedicated Access service token, then exec a publisher without bootstrap credentials.

Use an existing managed Cloudflare credential. The scoped client secret only exists in memory;
no credential is written to argv, a file, stdout or an operator configuration.
"""
import argparse
from datetime import UTC, datetime, timedelta
import json
import os
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--account", required=True)
    parser.add_argument("--service-token", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("Pass the publisher command after --")
    if not args.account.isalnum() or len(args.account) != 32:
        parser.error("Invalid account ID")
    bearer = os.environ.get("CLOUDFLARE_API_TOKEN")
    key = os.environ.get("CLOUDFLARE_GLOBAL_API_KEY") or os.environ.get("CLOUDFLARE_API_KEY")
    email = os.environ.get("CLOUDFLARE_EMAIL") or os.environ.get("DAS_ITEM_CLOUDFLARE_ACCESS_TOKEN__EMAIL")
    if bearer:
        headers = {"Authorization": "Bearer " + bearer}
    elif key and email:
        headers = {"X-Auth-Key": key, "X-Auth-Email": email}
    else:
        raise SystemExit("A managed Cloudflare credential with service-token write permission is required")
    headers["Content-Type"] = "application/json"
    base = f"https://api.cloudflare.com/client/v4/accounts/{args.account}/access/service_tokens"

    def request(url, payload=None, method="GET"):
        req = urllib.request.Request(url, headers=headers, method=method,
                                     data=json.dumps(payload).encode() if payload else None)
        try:
            with urllib.request.urlopen(req, timeout=45) as response:
                result = json.load(response)
        except urllib.error.HTTPError as exc:
            raise SystemExit(f"Access token operation returned HTTP {exc.code}") from None
        if not result.get("success"):
            raise SystemExit("Access token operation failed; no publisher was started")
        return result["result"]

    tokens = request(base)
    current = next((token for token in tokens if token["id"] == args.service_token), None)
    if current is None:
        raise SystemExit("The configured service token does not exist")
    rotated = request(base + "/" + args.service_token + "/rotate", {
        "previous_client_secret_expires_at": (datetime.now(UTC) + timedelta(seconds=60)).isoformat(),
    }, method="POST")
    if not rotated.get("client_secret") or not rotated.get("client_id"):
        raise SystemExit("Provider did not return a rotated credential; no publisher was started")
    child = {name: os.environ[name] for name in ("PATH", "HOME", "USER", "TMPDIR", "LANG", "LC_ALL", "SSL_CERT_FILE")
             if name in os.environ}
    if "PUBLISH_GITHUB_TOKEN" in os.environ:
        child["PUBLISH_GITHUB_TOKEN"] = os.environ["PUBLISH_GITHUB_TOKEN"]
    child["CF_ACCESS_CLIENT_ID"] = rotated["client_id"]
    child["CF_ACCESS_CLIENT_SECRET"] = rotated["client_secret"]
    os.execvpe(command[0], command, child)


if __name__ == "__main__":
    main()
