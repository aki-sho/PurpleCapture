// Crate archives occasionally omit workspace-root licenses. Retrieve their
// original texts at the exact source commit, never from an unpinned branch.
import fs from "node:fs";
import path from "node:path";
import { root } from "./metadata.js";
const metadata = JSON.parse(fs.readFileSync(path.join(root, ".tmp/cargo-metadata.json"), "utf8"));
const names = ["alloc-stdlib", "selectors", "tauri-plugin", "unic-char-property", "unic-char-range", "unic-common", "unic-ucd-ident", "unic-ucd-version", "webview2-com", "webview2-com-macros", "webview2-com-sys"];
const cache = new Map();
const sources = [];
for (const pkg of metadata.packages.filter((pkg) => names.includes(pkg.name))) {
  if (pkg.name === "selectors") {
    // The source header explicitly directs recipients to this canonical text.
    const url = "https://www.mozilla.org/media/MPL/2.0/index.txt";
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    const directory = path.join(root, "third-party", "licenses", `${pkg.name}-${pkg.version}`);
    fs.mkdirSync(directory, { recursive: true });
    fs.writeFileSync(path.join(directory, "LICENSE-MPL-2.0"), await response.text());
    sources.push({ package: pkg.name, version: pkg.version, file: "LICENSE-MPL-2.0", url });
    continue;
  }
  const vcs = JSON.parse(fs.readFileSync(path.join(path.dirname(pkg.manifest_path), ".cargo_vcs_info.json"), "utf8"));
  const repo = pkg.repository.replace(/^https:\/\/github.com\//, "").replace(/\/$/, "");
  const key = `${repo}/${vcs.git.sha1}`;
  if (!cache.has(key)) {
    const response = await fetch(`https://api.github.com/repos/${repo}/git/trees/${vcs.git.sha1}`);
    if (!response.ok) throw new Error(`${key}: ${response.status}`);
    const tree = await response.json();
    const files = tree.tree.filter((entry) => entry.type === "blob" && /^(license|licence|copying|copyright|notice)([._-]|$)/i.test(entry.path));
    if (!files.length) throw new Error(`No license at ${key}`);
    const texts = [];
    for (const file of files) {
      const url = `https://raw.githubusercontent.com/${key}/${file.path}`;
      const body = await fetch(url);
      if (!body.ok) throw new Error(`${url}: ${body.status}`);
      texts.push({ name: file.path, text: await body.text(), url });
    }
    cache.set(key, texts);
  }
  const directory = path.join(root, "third-party", "licenses", `${pkg.name}-${pkg.version}`);
  fs.mkdirSync(directory, { recursive: true });
  for (const file of cache.get(key)) {
    fs.writeFileSync(path.join(directory, file.name), file.text);
    sources.push({ package: pkg.name, version: pkg.version, file: file.name, url: file.url });
  }
}
fs.writeFileSync(path.join(root, "third-party/license-sources.json"), `${JSON.stringify(sources, null, 2)}\n`);
console.log(`Retrieved ${sources.length} license texts from pinned upstream commits.`);
