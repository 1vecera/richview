import { readFile, writeFile, mkdir, cp } from "node:fs/promises";
import { resolve, dirname } from "node:path";
const output = process.argv[2];
const count = Number(process.argv[3] || 300);
if (!output || !Number.isInteger(count) || count < 1 || count > 900)
	throw new Error("Usage: fixtures.ts output-directory [1..900]");
await mkdir(dirname(resolve(output)), { recursive: true });
await mkdir(output);
const site = await readFile("publications/news/site.yaml", "utf8");
await writeFile(
	resolve(output, "site.yaml"),
	site.replace(
		"lead: [measurement-gap, slower-is-clearer]",
		count > 1 ? "lead: [fixture-0000, fixture-0001]" : "lead: [fixture-0000]",
	),
);
const templates = ["small-samples", "reading-a-receipt", "correction-as-a-feature", "release-not-a-folder"];
for (let i = 0; i < count; i++) {
	const id = "fixture-" + String(i).padStart(4, "0");
	const dir = resolve(output, "items", id);
	const template = templates[i % templates.length];
	await cp(resolve("publications/news/items", template), dir, { recursive: true });
	const source = await readFile(
		resolve(dir, "report.rmd"),
		"utf8",
	);
	await writeFile(
		resolve(dir, "report.rmd"),
		source
			.replaceAll(template, id)
			.replace(
				/^title:.*$/m,
				'title: "Representative fixture ' + i + '"',
			)
			.replace(
				'  audience: "public"',
				`  audience: "${i % 10 === 0 ? "members" : "public"}"`,
			)
			.replace(
				'  state: "published"',
				`  state: "${i % 13 === 0 ? "draft" : "published"}"`,
			),
	);
}
console.log(
	JSON.stringify({ fixtures: count, output: resolve(output), generated: true }),
);
