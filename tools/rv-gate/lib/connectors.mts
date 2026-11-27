// Connector examples in the gate (lead, 28 Oct: seeding in test tooling, MetricFlow unrun with
// reason; FEA-BR-14, FEA-BR-15, ARC-59, DEL-60, ARC-54).
//
// An example folder with `report.connectors.yaml` names connectors; the gate decides per example
// whether it can render them:
//   seed         every connector is DuckDB or a Parquet file and the folder ships `data/seed.sql`:
//                the gate seeds a temporary copy with the dev-only `seed_duckdb` example and
//                renders the copy. For DuckDB the script fills a fresh database; for Parquet it
//                writes the files itself (`copy ... to 'data/<name>.parquet'`, relative to the
//                report folder). No product feature, no network, no committed binary.
//   runtime      a MetricFlow connector: it needs the runtime from the server's configuration, so it
//                runs for real only when the gate's environment names one (MF_ENV); otherwise it is
//                validated only and reported `unrun`, never as a pass.
//   unsupported  anything else: the gate cannot prepare it, which is a finding.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";

export const CONNECTORS_YAML = "report.connectors.yaml";
export const SEED_SQL = "data/seed.sql";
/** Where the build step leaves the dev-only seeding example, relative to the checkout. */
export const SEED_BIN = "target/release/examples/seed_duckdb";
/** What a MetricFlow example needs from the gate's environment (FEA-BR-15). */
export const MF_ENV = ["RICHVIEW_MF_OKURKA_DIR", "DBT_PROFILES_DIR", "OKURKA_DBT_PATH", "RICHVIEW_CONNECTORS"];
export const MF_REASON = "needs the MetricFlow runtime from the server's configuration (FEA-BR-15)";
/** The Rust test that needs the same runtime; rust-slow skips it when MF_ENV is incomplete. */
export const MF_TEST = "metricflow_e2e::okurka_metricflow_runtime_returns_real_rows_through_the_connector";
/**
 * Folders copied with an example are never the report: frozen research, earlier output, and the
 * connector cache, whose rows are keyed by connector and SQL rather than by the database, so a
 * cache a test left in the checkout would stand in for the fresh seed (review 1i note 9).
 */
/** The connector kinds the gate seeds, and the key that names each one's report-relative source. */
const SOURCE_KEY: Record<string, string> = { duckdb: "database", parquet: "file" };
export const SKIP_COPY = new Set(["research", "Rendered", "node_modules", ".git", ".richview-cache"]);

/** The names in `vars` that `env` leaves unset or empty. */
export function missingEnv(vars: readonly string[], env = process.env) {
	return vars.filter((name: string|number) => !env[name]);
}

/**
 * Read `report.connectors.yaml`, a two-level mapping: `connectors:` then one block per connector
 * with scalar fields. Returns `{ name: { kind, database?, runtime?, ... } }`; throws on other shapes.
 */
export function parseConnectorsYaml(text: string) {
	const connectors: Record<string, Record<string, string>> = {};
	let inConnectors = false;
	let current: Record<string, string> | undefined;
	let blockIndent = 0;
	for (const raw of text.split("\n")) {
		const line = raw.replace(/(^|\s)#.*$/, "").trimEnd();
		if (!line.trim()) continue;
		const indent = line.length - line.trimStart().length;
		const match = /^([^:\s][^:]*):\s*(.*)$/.exec(line.trim());
		if (!match) throw new Error(`${CONNECTORS_YAML}: cannot read "${line.trim()}"`);
		const [, key, value] = match;
		if (indent === 0) {
			inConnectors = key === "connectors";
			current = undefined;
		} else if (!inConnectors) {
			continue;
		} else if (!current || indent <= blockIndent) {
			if (value !== "") throw new Error(`${CONNECTORS_YAML}: connector '${key}' must be a block`);
			current = connectors[key] = {};
			blockIndent = indent;
		} else {
			current[key] = value.replace(/^["']|["']$/g, "");
		}
	}
	return connectors;
}

/**
 * How the gate treats an example folder: `null` when it declares no connectors, else
 * `{ kind: "seed" | "runtime" | "unsupported", connectors, reason? }`.
 */
export function classifyConnectors(dir: string) {
	const file = join(dir, CONNECTORS_YAML);
	if (!existsSync(file)) return null;
	const connectors = parseConnectorsYaml(readFileSync(file, "utf8"));
	const entries = Object.entries(connectors);
	const unsupported = (reason: string) => ({ kind: "unsupported" as const, connectors, reason });
	if (!entries.length) return unsupported(`${CONNECTORS_YAML} declares no connector`);
	const other = entries.find(([, c]) => !SOURCE_KEY[c.kind] && c.kind !== "metricflow");
	if (other) return unsupported(`connector '${other[0]}' has kind '${other[1].kind}', which the gate cannot prepare`);
	if (entries.some(([, c]) => c.kind === "metricflow")) return { kind: "runtime" as const, connectors, reason: MF_REASON };
	if (!existsSync(join(dir, SEED_SQL))) return unsupported(`a ${entries[0][1].kind} connector needs ${SEED_SQL} for the gate to seed it`);
	const bare = entries.find(([, c]) => !c[SOURCE_KEY[c.kind]]);
	if (bare) return unsupported(`${bare[1].kind} connector '${bare[0]}' names no ${SOURCE_KEY[bare[1].kind]}`);
	return { kind: "seed" as const, connectors };
}

/** The trusted connectors file the gate writes beside a seeded copy (FEA-BR-14: never inside the report). */
export const FIXTURE_CONNECTORS = `# Written by rv-gate for a seeded example. DuckDB databases are named by the report's own
# report.connectors.yaml (report-relative, confined to its folder). CLI and MCP use the same
# default snapshot store; each gate entry point gives it a private XDG_STATE_HOME.
`;

/**
 * Copy a seed example to `dest`, seed each of its DuckDB databases from `data/seed.sql` with the
 * dev-only `seed_duckdb <seed.sql> <database>` (it creates a fresh file and refuses to overwrite),
 * let the script write the Parquet files its `parquet` connectors name (run once, in the copy,
 * against an in-memory database), and write the trusted connectors file beside the copy. Returns what a render needs:
 * `{ folder, connectorsFile, args, env }`. Throws when seeding fails.
 */
export function prepareSeedExample({ source, dest, seedBin, run = spawnSync }: { source: string; dest: string; seedBin?: string; run?: typeof spawnSync }) {
	const classified = classifyConnectors(source);
	if (classified?.kind !== "seed") throw new Error(`${source} is not a seed example (${classified?.kind ?? "no connectors"})`);
	if (!existsSync(seedBin!)) throw new Error(`The seed binary ${seedBin} does not exist; the build step makes it (cargo build --release -p richview-app --example seed_duckdb)`);
	// A copy left by an earlier run keeps no cached rows either.
	rmSync(join(dest, ".richview-cache"), { recursive: true, force: true });
	cpSync(source, dest, { recursive: true, filter: (src) => !SKIP_COPY.has(basename(src)) });
	const root = resolve(dest);
	const seed = (name: string, database: string) => {
		// The script names the Parquet files it writes relative to the report folder.
		const seeded = run(seedBin!, [join(root, SEED_SQL), database], { encoding: "utf8", cwd: root });
		if (seeded.error || seeded.status !== 0) throw new Error(`seeding '${name}' failed: ${(seeded.error?.message ?? (seeded.stderr || seeded.stdout || `exit ${seeded.status}`)).trim().slice(0, 300)}`);
	};
	const written: [name: string, source: string, path: string][] = [];
	for (const [name, connector] of Object.entries(classified.connectors)) {
		const key = SOURCE_KEY[connector.kind];
		const source = resolve(root, connector[key]);
		const inside = relative(root, source);
		if (inside === "" || inside.startsWith("..")) throw new Error(`connector '${name}' ${key} '${connector[key]}' escapes the report folder`);
		mkdirSync(dirname(source), { recursive: true });
		// A source the checkout already holds (a test seeded it there) is not the seed: start fresh (review 1h should-fix 6).
		rmSync(source, { force: true });
		if (connector.kind === "parquet") written.push([name, source, inside]);
		else seed(name, source);
	}
	if (written.length) {
		seed(written[0][0], ":memory:");
		const missing = written.find(([, source]) => !existsSync(source));
		if (missing) throw new Error(`seeding '${missing[0]}' failed: ${SEED_SQL} did not write '${missing[2]}'`);
	}
	const connectorsFile = `${root}.connectors.toml`;
	writeFileSync(connectorsFile, FIXTURE_CONNECTORS);
	return { folder: root, connectorsFile, args: ["--connectors", connectorsFile], env: { RICHVIEW_CONNECTORS: connectorsFile } };
}

/** The `--seed-bin` flag, else the build step's output in the checkout. */
export function seedBinPath(cwd: string, flag?: string | null) {
	return resolve(flag ? resolve(flag) : join(cwd, SEED_BIN));
}

/** One line of the human report for unrun inputs, or undefined. */
export function unrunLine(unrun: readonly { input: string; reason: string }[]) {
	return unrun.length ? `unrun: ${unrun.map((u) => `${u.input} (${u.reason})`).join("; ")}` : undefined;
}
