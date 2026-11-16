// Host reason (ARC-03): Cloudflare Worker adapter; verifies the Access token and hands the principal to the Rust server.
import { createRemoteJWKSet, jwtVerify } from "jose";
let keySet, issuer;

/** Access owns login and sessions. No header, cookie or email is trusted without verification. */
export async function identity(request, env) {
	if (!env.ACCESS_ISSUER || !env.ACCESS_AUD) return null;
	const cookie = request.headers
		.get("cookie")
		?.split(";")
		.map((x) => x.trim())
		.find((x) => x.startsWith("CF_Authorization="))
		?.slice(17);
	const token = request.headers.get("Cf-Access-Jwt-Assertion") || cookie;
	if (!token) return null;
	try {
		if (issuer !== env.ACCESS_ISSUER) {
			issuer = env.ACCESS_ISSUER;
			if (!/^https:\/\/[a-z0-9-]+\.cloudflareaccess\.com$/.test(issuer))
				return null;
			keySet = createRemoteJWKSet(new URL(`${issuer}/cdn-cgi/access/certs`));
		}
		const { payload } = await jwtVerify(token, keySet, {
			issuer,
			audience: env.ACCESS_AUD,
			algorithms: ["RS256"],
			requiredClaims: ["exp", "iat", "sub"],
		});
		if (typeof payload.email === "string" && payload.email)
			return {
				key: `email:${payload.email.toLowerCase()}`,
				name: payload.email,
				email: payload.email,
			};
		if (typeof payload.common_name === "string" && payload.common_name)
			return {
				key: `service:${payload.common_name}`,
				name: "Publication service",
				service: true,
			};
		return null;
	} catch {
		return null;
	}
}
