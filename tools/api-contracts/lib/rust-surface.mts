// Host reason (ARC-03): inspect explicitly registered Rust facade declarations without building
// a Wasm runtime or teaching the gate about a crate's private implementation.

type Declaration = {head: string[]; body: string[] | null; end: number};
type SurfaceOptions = {select?: string[]; implicitPublic?: boolean; moduleDeclarations?: boolean};
export type PublicItem = {
  name: string; kind: string; declaration: string; attributes?: string; publicFields?: string[];
  tupleFields?: {index: number; declaration: string}[]; privateFields?: boolean; variants?: string[];
  members?: PublicItem[]; default?: boolean; types?: string[];
};

const pairs: Record<string, string> = { "(": ")", "[": "]", "{": "}" };
const canonical = (tokens: string[]) => tokens.join(" ");
function fail(message: string): never { throw new Error(message); }

/** Tokenize declaration syntax, keeping literals intact and discarding nested comments. */
export function rustTokens(source: string) {
  const tokens = [];
  let at = 0;
  while (at < source.length) {
    const rest = source.slice(at);
    if (/^\s/.test(rest)) { at++; continue; }
    if (rest.startsWith("//")) { const end = source.indexOf("\n", at); at = end < 0 ? source.length : end + 1; continue; }
    if (rest.startsWith("/*")) {
      let depth = 1; at += 2;
      while (at < source.length && depth) {
        if (source.startsWith("/*", at)) { depth++; at += 2; }
        else if (source.startsWith("*/", at)) { depth--; at += 2; }
        else at++;
      }
      if (depth) fail("Unclosed Rust block comment");
      continue;
    }
    const raw = /^(?:b|c)?r(#{0,255})"/.exec(rest);
    if (raw) {
      const close = `"${raw[1]}`;
      const end = source.indexOf(close, at + raw[0].length);
      if (end < 0) fail("Unclosed Rust raw string");
      tokens.push(source.slice(at, end + close.length)); at = end + close.length; continue;
    }
    const string = /^(?:b|c)?"/.exec(rest);
    if (string) {
      const start = at; at += string[0].length;
      while (at < source.length && source[at] !== '"') at += source[at] === "\\" ? 2 : 1;
      if (at >= source.length) fail("Unclosed Rust string");
      tokens.push(source.slice(start, ++at)); continue;
    }
    const character = /^(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\])'/u.exec(rest);
    const word = /^(?:r#)?[\p{L}_][\p{L}\p{N}_]*/u.exec(rest);
    const number = /^(?:0[xob][0-9a-fA-F_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?[\d_]+)?)(?:[a-zA-Z][a-zA-Z\d_]*)?/.exec(rest);
    const match = character ?? word ?? number;
    if (match) { tokens.push(match[0]); at += match[0].length; }
    else tokens.push(source[at++]);
  }
  return tokens;
}

function groupEnd(tokens: string[], start: number): number {
  const close = pairs[tokens[start]];
  if (!close) fail(`Expected a Rust group at ${tokens[start]}`);
  for (let at = start + 1; at < tokens.length; at++) {
    if (tokens[at] === close) return at;
    if (pairs[tokens[at]]) at = groupEnd(tokens, at);
    else if (Object.values(pairs).includes(tokens[at])) fail("Mismatched Rust delimiter");
  }
  fail(`Unclosed Rust ${tokens[start]} group`);
}

function attributes(tokens: string[], start: number) {
  const kept = [];
  let at = start;
  while (tokens[at] === "#") {
    if (tokens[at + 1] === "!") fail("Inner attributes are unsupported in an API facade");
    if (tokens[at + 1] !== "[") fail("Malformed Rust attribute");
    const end = groupEnd(tokens, at + 1);
    // Documentation and lint wording do not change the public interface.
    if (!["doc", "allow", "warn", "deny", "forbid", "expect"].includes(tokens[at + 2])) kept.push(...tokens.slice(at, end + 1));
    at = end + 1;
  }
  return { kept, at };
}

function visibility(tokens: string[], start: number) {
  if (tokens[start] !== "pub") return { public: false, at: start };
  if (tokens[start + 1] === "(") return { public: false, at: groupEnd(tokens, start + 1) + 1 };
  return { public: true, at: start + 1 };
}

/** Read one top-level declaration, distinguishing its body from signature groups. */
function declaration(tokens: string[], start: number): Declaration {
  let angle = 0;
  for (let at = start; at < tokens.length; at++) {
    const token = tokens[at];
    if (token === ";" && !angle) return { head: tokens.slice(start, at), body: null, end: at + 1 };
    if (token === "<") angle++;
    if (token === ">" && tokens[at - 1] !== "-" && angle) angle--;
    if (token === "{" && !angle) {
      const end = groupEnd(tokens, at);
      return { head: tokens.slice(start, at), body: tokens.slice(at + 1, end), end: end + 1 + Number(tokens[end + 1] === ";") };
    }
    if (pairs[token]) at = groupEnd(tokens, at);
  }
  fail("Rust declaration has no terminator");
}

function split(tokens: string[], separator = ",") {
  const parts = []; let start = 0, angle = 0;
  for (let at = 0; at < tokens.length; at++) {
    const token = tokens[at];
    if (token === "<") angle++;
    if (token === ">" && tokens[at - 1] !== "-" && angle) angle--;
    if (pairs[token]) at = groupEnd(tokens, at);
    else if (token === separator && !angle) { parts.push(tokens.slice(start, at)); start = at + 1; }
  }
  if (start < tokens.length) parts.push(tokens.slice(start));
  return parts.filter(part => part.length);
}

function fields(tokens: string[]) {
  const publicFields = []; let privateFields = false;
  for (const field of split(tokens)) {
    const attr = attributes(field, 0);
    const vis = visibility(field, attr.at);
    if (vis.public) publicFields.push(canonical([...attr.kept, ...field.slice(attr.at)]));
    else privateFields = true;
  }
  return { publicFields, privateFields };
}

function useItems(tokens: string[], prefix: string[] = []): PublicItem[] {
  const open = tokens.indexOf("{");
  if (tokens.includes("*")) fail("Wildcard reexports cannot define a checked API facade");
  if (open >= 0) {
    const end = groupEnd(tokens, open);
    if (end !== tokens.length - 1) fail("Unsupported Rust use tree");
    return split(tokens.slice(open + 1, end)).flatMap(branch => useItems(branch, [...prefix, ...tokens.slice(0, open)]));
  }
  const alias = tokens.indexOf("as");
  const path = [...prefix, ...tokens.slice(0, alias < 0 ? tokens.length : alias)];
  if (path.at(-1) === "self") path.splice(-3);
  const name = alias < 0 ? path.at(-1) : tokens[alias + 1];
  if (!name || (alias >= 0 && alias !== tokens.length - 2)) fail("Unsupported Rust reexport");
  return [{ name, kind: "use", declaration: canonical(["pub", "use", ...path, ...(alias < 0 ? [] : ["as", name]), ";"]) }];
}

/** Extract the registered file's explicit public declarations. Unsupported public forms fail. */
function surfaceTokens(tokens: string[], { select, implicitPublic = false, moduleDeclarations = false }: SurfaceOptions = {}): PublicItem[] {
  const items: PublicItem[] = [], implementations: (Declaration & {attributes: string[]})[] = [];
  for (let at = 0; at < tokens.length;) {
    if (tokens[at] === ";") { at++; continue; }
    const attr = attributes(tokens, at);
    const vis = visibility(tokens, attr.at);
    let kindAt = vis.at;
    while (["unsafe", "async", "extern"].includes(tokens[kindAt]) || /^"/.test(tokens[kindAt] ?? "")) kindAt++;
    if (tokens[kindAt] === "const" && tokens[kindAt + 1] === "fn") kindAt++;
    const kind = tokens[kindAt];
    if (!kind) break;
    if (tokens[kindAt + 1] === "!") fail("Module-level macros need an explicit expanded public contract");
    const item = declaration(tokens, vis.at);
    if (!select && kind === "{" && item.head.includes("extern") && item.body?.includes("pub")) fail("Public extern blocks need an explicit supported facade contract");
    if (vis.public || implicitPublic) {
      if (select && kind !== "use" && !select.includes(tokens[kindAt + 1])) { at = item.end; continue; }
      if (kind === "use") items.push(...useItems([...item.head.slice(1), ...(item.body ? ["{", ...item.body, "}"] : [])]).map(entry => attr.kept.length ? { ...entry, attributes: canonical(attr.kept) } : entry));
      else if (kind === "mod" && moduleDeclarations) {
        if (item.body || attr.kept.some((token, at) => token === "#" && attr.kept[at + 2] === "path")) fail("Registered facade modules require a direct external module declaration");
        items.push({ name: tokens[kindAt + 1], kind: "mod", declaration: canonical([...attr.kept, "pub", ...item.head, ";"]) });
      }
      else if (["fn", "type", "const", "static", "struct", "enum", "trait", "union"].includes(kind)) {
        const name = tokens[kindAt + 1] === "mut" ? tokens[kindAt + 2] : tokens[kindAt + 1];
        if (select && !select.includes(name)) { at = item.end; continue; }
        const entry: PublicItem = { name, kind, declaration: canonical([...attr.kept, ...(vis.public ? ["pub"] : []), ...item.head]) };
        if (kind === "struct" && !item.body) {
          let angle = 0, open = -1;
          for (let i = 2; i < item.head.length && item.head[i] !== "where"; i++) {
            if (item.head[i] === "<") angle++;
            if (item.head[i] === ">" && item.head[i - 1] !== "-") angle--;
            if (!angle && item.head[i] === "(") { open = i; break; }
          }
          if (open >= 0) {
            const end = groupEnd(item.head, open);
            entry.declaration = canonical([...attr.kept, "pub", ...item.head.slice(0, open), ...item.head.slice(end + 1)]);
            entry.tupleFields = [];
            entry.privateFields = false;
            for (const [index, field] of split(item.head.slice(open + 1, end)).entries()) {
              const attribute = attributes(field, 0), visible = visibility(field, attribute.at);
              if (visible.public) entry.tupleFields.push({ index, declaration: canonical([...attribute.kept, ...field.slice(attribute.at)]) });
              else entry.privateFields = true;
            }
          }
        }
        if (["struct", "union"].includes(kind) && item.body) Object.assign(entry, fields(item.body));
        else if (kind === "enum" && item.body) entry.variants = split(item.body).map(canonical);
        else if (kind === "trait" && item.body) entry.members = surfaceTokens(item.body, { implicitPublic: true });
        else if (item.body && kind !== "fn") fail(`Unsupported ${kind} body in ${name}`);
        if (implicitPublic && kind === "fn") entry.default = Boolean(item.body);
        items.push(entry);
      } else fail(`Unsupported public Rust declaration: ${kind}`);
    } else if (kind === "impl") {
      implementations.push({ ...item, attributes: attr.kept });
    }
    at = item.end;
  }
  const types = items.filter(item => ["struct", "enum", "union", "type"].includes(item.kind)).map(item => item.name);
  for (const implementation of implementations) {
    let head = implementation.head.slice(implementation.head.indexOf("impl") + 1);
    if (head[0] === "<") {
      let depth = 1, at = 1;
      for (; at < head.length && depth; at++) {
        if (head[at] === "<") depth++;
        if (head[at] === ">" && head[at - 1] !== "-") depth--;
      }
      if (depth) fail("Unclosed impl generic parameters");
      head = head.slice(at);
    }
    const where = head.indexOf("where");
    if (where >= 0) head = head.slice(0, where);
    const traitFor = head.findIndex((token, at) => token === "for" && head[at + 1] !== "<");
    const target = traitFor < 0 ? head : head.slice(traitFor + 1);
    const owners = types.filter(name => target.includes(name));
    if (!owners.length) continue;
    const members = surfaceTokens(implementation.body ?? [], { implicitPublic: traitFor >= 0 });
    if (traitFor < 0 && !members.length) continue;
    const name = canonical(implementation.head);
    items.push({ name, kind: "impl", types: owners.sort(), declaration: canonical([...implementation.attributes, ...implementation.head]), members });
  }
  const selected = select ? items.filter(item => select.includes(item.name) || (item.kind === "impl" && item.types!.some(name => select.includes(name)))) : items;
  if (select) for (const name of select) if (!selected.some(item => item.name === name)) fail(`Registered public item ${name} is missing`);
  selected.sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
  for (let i = 1; i < selected.length; i++) if (selected[i - 1].name === selected[i].name) fail(`Ambiguous public item ${selected[i].name}`);
  return selected;
}

/** Extract a facade or explicitly selected public types, including their public capabilities. */
export function publicSurface(source: string, options?: SurfaceOptions) {
  return surfaceTokens(rustTokens(source), options);
}

/** Verify the crate exposes the external facade rather than just leaving its source on disk. */
export function publicModule(source: string, name: string) {
  const items = surfaceTokens(rustTokens(source), { select: [name], moduleDeclarations: true });
  const item = items.find(item => item.name === name);
  if (item?.kind !== "mod") fail(`Expected a direct public module declaration for ${name}`);
  return item.declaration;
}
