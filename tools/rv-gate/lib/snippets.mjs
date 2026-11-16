// The committed VS Code snippet file comes from each chart page's worked example (ARC-46).
// The builder checks drift without rewriting it; the editor's freshness test regenerates it
// only with UPDATE_SNIPPETS=1. Fast runs that test when the change can alter a snippet.

export const SNIPPET_FILE = "packages/vscode/snippets/rmd.json";
export const REGENERATE = "UPDATE_SNIPPETS=1 cargo test -p richview-editor --test snippets committed_snippets_match_the_generator";

/** Chart pages whose first worked example is a snippet (FEA-SF-26). */
const CHART_PAGE = /^docs\/public\/charts\/[^/]+\.md$/;
/** The generator's own files: the example extractor, and the files that shape the catalogue. */
export const GENERATOR_FILES = ["crates/richview-editor/build.rs", "crates/richview-editor/src/lib.rs", "crates/richview-build/src/main.rs"];
/** The chart type registry the generator walks: a change there can add or drop a snippet (review 1g note 8). */
export const REGISTRY_FILES = ["crates/richview-core/src/registry.rs", "crates/richview-core/src/authoring.rs"];
/** Only the extractor changes every snippet; the other generator files count when the change names snippets. */
const ALWAYS = "crates/richview-editor/build.rs";

/** The first chart fence inside a page's four-backtick `rmd` source block: `build.rs`'s `example_of`. */
export function exampleOf(page) {
	if (page === undefined) return undefined;
	const open = page.indexOf("````rmd\n");
	if (open < 0) return undefined;
	let block = page.slice(open + "````rmd\n".length);
	const close = block.indexOf("\n````");
	if (close < 0) return undefined;
	block = block.slice(0, close);
	const from = block.indexOf("```chart\n");
	if (from < 0) return undefined;
	const rest = block.slice(from);
	const end = rest.indexOf("\n```", 3);
	if (end < 0) return undefined;
	return `${rest.slice(0, end + 4)}\n`;
}

/**
 * The editor's test that runs the generator and compares its output with the committed file. Where
 * it exists, fast runs it for a change that may alter a snippet instead of judging the diff, so a
 * generator change whose output is already committed passes.
 */
export const FRESHNESS_TEST = { crate: "richview-editor", test: "snippets", file: "crates/richview-editor/tests/snippets.rs", name: "committed_snippets_match_the_generator" };

/**
 * Judge a change that may alter a snippet: a changed chart page whose snippet source differs from
 * its base version, or a changed generator file. With the freshness test in the checkout
 * (`hasFreshnessTest`) the result is `test`: run it. Without it, the change fails unless the
 * committed snippet file changed too. `before(path)` and `after(path)` give a file's text at the
 * base and now (undefined when absent); `mentionsSnippets(path)` says whether the change to a
 * generator file touches snippet code.
 */
export function snippetCheck({ changed, before, after, mentionsSnippets, hasFreshnessTest = false }) {
	// A hand edit of the committed file is judged by the freshness test too (review 1h note 4).
	if (changed.includes(SNIPPET_FILE)) return hasFreshnessTest ? { status: "test", reason: `${SNIPPET_FILE} changed: ${FRESHNESS_TEST.crate} --test ${FRESHNESS_TEST.test} compares it with the generator`, causes: [SNIPPET_FILE] } : { status: "pass" };
	const causes = [];
	for (const file of changed) {
		if (CHART_PAGE.test(file)) {
			if (exampleOf(before(file)) !== exampleOf(after(file))) causes.push(`${file} (its snippet example)`);
		} else if (GENERATOR_FILES.includes(file) && (file === ALWAYS || mentionsSnippets(file))) causes.push(`${file} (the snippet generator)`);
		// The registry changes often and rarely a snippet: only the freshness test, which runs the generator, judges it.
		else if (hasFreshnessTest && REGISTRY_FILES.includes(file)) causes.push(`${file} (the chart type registry)`);
	}
	if (!causes.length) return { status: "pass" };
	if (hasFreshnessTest) return { status: "test", reason: `${causes.join(", ")} changed: ${FRESHNESS_TEST.crate} --test ${FRESHNESS_TEST.test} compares ${SNIPPET_FILE} with the generator`, causes };
	return { status: "fail", reason: `${SNIPPET_FILE} is stale: ${causes.join(", ")} changed without it. Regenerate and commit it: ${REGENERATE}`, causes };
}
