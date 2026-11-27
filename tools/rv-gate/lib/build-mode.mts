// Host reason (ARC-03): build the tested checkout in the mode its install switch requires.
import { readFileSync } from "node:fs";
import { join } from "node:path";

export function releaseBuildModeArgs(cwd: string) {
	const { enabled }: { enabled?: unknown } = JSON.parse(readFileSync(join(cwd, "assets/install/release.json"), "utf8"));
	if (typeof enabled !== "boolean") throw new Error("install release must say whether it is enabled");
	return enabled ? ["--", "--repository-public"] : [];
}
