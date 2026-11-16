// Host reason (ARC-03): keep release evidence bound to this checkout and its pinned browser.

/** Ambient probe overrides must not redirect a release check to different inputs or an engine. */
export const SOURCE_OVERRIDES = Object.freeze([
	"RICHVIEW_CLI", "RICHVIEW_BIN", "RICHVIEW_DIST", "RICHVIEW_VERIFY_URL", "RICHVIEW_VERIFY_PUBLIC_DIR",
	"RICHVIEW_PUBLIC_DIR", "PUBLIC_DIR", "RICHVIEW_VERIFY_NO_HEADERS", "CHROMIUM_PATH", "RICHVIEW_CHROMIUM",
	"RICHVIEW_BROWSER", "RICHVIEW_VERIFY_BROWSER", "RICHVIEW_VERIFY_WORKERS",
]);

/** Clear inherited source overrides during release evidence execution, then restore them. */
export async function withGateSourceEnvironment(mode, run) {
	if (!["release", "full", "prod"].includes(mode)) return run();
	const saved = new Map(SOURCE_OVERRIDES.map(name => [name, process.env[name]]));
	for (const name of SOURCE_OVERRIDES) delete process.env[name];
	try { return await run(); }
	finally { for (const [name, value] of saved) if (value === undefined) delete process.env[name]; else process.env[name] = value; }
}
