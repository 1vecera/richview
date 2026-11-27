// Host reason (ARC-03): declare the inputs Wrangler loads as Worker modules.
declare module "*.woff2" {
	const bytes: ArrayBuffer;
	export default bytes;
}

declare module "*pkg/richview_publish_server_bg.wasm" {
	const module: WebAssembly.Module;
	export default module;
}

// A clean checkout can type-check before wasm-bindgen generates its .d.ts file.
declare module "*pkg/richview_publish_server.js" {
	export function initSync(input: { module: WebAssembly.Module | BufferSource }): unknown;
	export function publish_handle(input: string): string;
	export function publish_migrate(input: string): string;
}

// The local platform tools use the Worker's own installed JOSE package.
declare module "*cloudflare/publish/node_modules/jose/dist/webapi/index.js" {
	export { exportJWK, generateKeyPair, SignJWT } from "jose";
}
