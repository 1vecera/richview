// Deployed entry (ARC-03 host reason): wires the Wasm Publish server into Cloudflare, and serves the bundled fonts.
// Fonts are the official OFL Fontsource subsets (latin + latin-ext); licences are in ./fonts/licenses.
// Build the Wasm first: `npm run build:wasm` (cargo build for wasm32, then wasm-bindgen into ./pkg).
import wasmModule from "./pkg/richview_publish_server_bg.wasm";
import * as wasm from "./pkg/richview_publish_server.js";
import { PublicationBase, gateway, fontResponse } from "./adapter.ts";
import type { PublicationBindings, PublicationContext, GatewayBindings } from "./adapter.ts";
import newsreaderLatin500 from "./fonts/newsreader-latin-500-normal.woff2";
import newsreaderLatinExt500 from "./fonts/newsreader-latin-ext-500-normal.woff2";
import sourceSerif4Latin400 from "./fonts/source-serif-4-latin-400-normal.woff2";
import sourceSerif4LatinExt400 from "./fonts/source-serif-4-latin-ext-400-normal.woff2";
import geistLatin400 from "./fonts/geist-latin-400-normal.woff2";
import geistLatinExt400 from "./fonts/geist-latin-ext-400-normal.woff2";
import geistLatin600 from "./fonts/geist-latin-600-normal.woff2";
import geistLatinExt600 from "./fonts/geist-latin-ext-600-normal.woff2";
import ibmPlexMonoLatin400 from "./fonts/ibm-plex-mono-latin-400-normal.woff2";
import ibmPlexMonoLatinExt400 from "./fonts/ibm-plex-mono-latin-ext-400-normal.woff2";

wasm.initSync({ module: wasmModule });
const fonts = {
	"newsreader-latin-500-normal.woff2": newsreaderLatin500,
	"newsreader-latin-ext-500-normal.woff2": newsreaderLatinExt500,
	"source-serif-4-latin-400-normal.woff2": sourceSerif4Latin400,
	"source-serif-4-latin-ext-400-normal.woff2": sourceSerif4LatinExt400,
	"geist-latin-400-normal.woff2": geistLatin400,
	"geist-latin-ext-400-normal.woff2": geistLatinExt400,
	"geist-latin-600-normal.woff2": geistLatin600,
	"geist-latin-ext-600-normal.woff2": geistLatinExt600,
	"ibm-plex-mono-latin-400-normal.woff2": ibmPlexMonoLatin400,
	"ibm-plex-mono-latin-ext-400-normal.woff2": ibmPlexMonoLatinExt400,
};

export class Publication extends PublicationBase {
	constructor(ctx: PublicationContext, env: PublicationBindings) {
		super(ctx, env, wasm);
	}
}
const forward = gateway();
export default {
	fetch(request: Request, env: GatewayBindings<unknown>, _ctx: unknown) {
		const { pathname } = new URL(request.url);
		if (pathname.startsWith("/_fonts/")) return fontResponse(request, pathname.slice(8), fonts);
		return forward(request, env);
	},
};
