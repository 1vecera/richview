// Host reason (ARC-03): run the signed-in editor controls in the reader's browser.
// The Worker serves this compiled function as a script. Keep all runtime references inside it.
export function startDesk() {
	type Item = { id: string; title: string; state: string; private: boolean };
	type Change = { number: number; head: string; state: string };
	type Button = [label: string, handler: () => void | Promise<void>];
	type Failure = { error?: string; diagnostics?: { line?: number; message: string; code: string }[] };
	type Result<Data> =
		| { ok: true; status: number; data: Data }
		| { ok: false; status: number; data: Failure };
	type Source = { path: string; revision: string; source: string };

	// Method shorthand keeps Wrangler's name-preservation helpers outside the served function.
	const { element, api, li, lines, load, open } = {
		element<T extends HTMLElement = HTMLElement>(id: string): T {
			return document.getElementById(id)! as T;
		},
		async api<Data>(path: string, body?: object): Promise<Result<Data>> {
			const response = await fetch("/_editor/api/" + path, body ? {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify(body),
			} : {});
			const type = response.headers.get("content-type") || "";
			const data: unknown = type.includes("json") ? await response.json() : { error: response.statusText };
			return { ok: response.ok, status: response.status, data } as Result<Data>;
		},
		li(label: string, ...buttons: Button[]) {
			const row = document.createElement("li");
			row.append(label + " ");
			for (const [name, handler] of buttons) {
				const button = document.createElement("button");
				button.textContent = name;
				button.onclick = handler;
				row.append(button);
			}
			return row;
		},
		lines() {
			element("nums").textContent = text.value.split("\n").map((_, i) => i + 1).join("\n");
		},
		async load() {
			const session = await api<{ actor: string; role: { name: string } }>("session");
			element("who").textContent = session.ok ? session.data.actor + " · " + session.data.role.name : "Not signed in";
			const list = await api<{ changes: Change[]; items: Item[] }>("items");
			if (!list.ok) return;
			element("changes").replaceChildren(...list.data.changes
				.filter((change) => change.state === "pending review" || change.state === "approved")
				.map((change) => li("#" + change.number + " " + change.state + " " + change.head.slice(0, 8) + " ",
					["Approve", async () => {
						const response = await api("approve", { number: change.number, head: change.head });
						alert(response.ok ? "Approved" : response.data.error || response.status);
						load();
					}],
					["Reject", async () => {
						const reason = prompt("Reason") || "";
						const response = await api("reject", { number: change.number, head: change.head, reason });
						alert(response.ok ? "Closed" : response.data.error || response.status);
						load();
					}])));
			element("items").replaceChildren(...list.data.items.map((item) => {
				const buttons: Button[] = [["Edit source", () => open(item)]];
				if (item.private) buttons.push(["Make public", async () => {
					const response = await api("approve", { id: item.id });
					alert(response.ok ? "Public" : response.data.error || response.status);
					load();
				}]);
				return li(item.title + " (" + item.state + (item.private ? ", private" : "") + ") ", ...buttons);
			}));
		},
		async open(item: Item) {
			const response = await api<Source>("source?id=" + encodeURIComponent(item.id));
			if (!response.ok) {
				alert("Unavailable: " + (response.data.error || response.status));
				return;
			}
			current = { item, path: response.data.path, revision: response.data.revision };
			element("title").textContent = item.title;
			text.value = response.data.source;
			element("edit").hidden = false;
			element("diag").replaceChildren();
			element("msg").textContent = "";
			lines();
		},
	};
	const text = element<HTMLTextAreaElement>("text");
	let current: { item: Item; path: string; revision: string } | null = null;
	text.oninput = lines;
	element("save").onclick = async () => {
		const message = element("msg");
		message.textContent = "Validating…";
		element("diag").replaceChildren();
		const response = await api<{ number: number }>("edit", {
			path: current!.path, revision: current!.revision, source: text.value,
		});
		if (response.ok) {
			message.className = "ok";
			message.textContent = "Saved as pull request #" + response.data.number;
			load();
			return;
		}
		message.className = "err";
		message.textContent = response.data.error || ("Refused (" + response.status + "). Your text is kept.");
		for (const diagnostic of response.data.diagnostics || [])
			element("diag").append(li((diagnostic.line ? "line " + diagnostic.line + ": " : "") + diagnostic.message + " [" + diagnostic.code + "]"));
	};
	load();
}
