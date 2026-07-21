import fs from "node:fs";
import path from "node:path";
import { readMetadata, root } from "./metadata.js";

const meta = readMetadata();
const tauriPath = path.join(root, "src-tauri", "tauri.conf.json");
const cargoPath = path.join(root, "src-tauri", "Cargo.toml");
const versionPath = path.join(root, "src", "shared", "version.js");

const tauri = JSON.parse(fs.readFileSync(tauriPath, "utf8"));
tauri.productName = meta.productName;
tauri.version = meta.version;
tauri.identifier = meta.identifier;
tauri.app.windows[0].title = meta.productName;
tauri.app.windows[0].width = meta.initialWidth;
tauri.app.windows[0].height = meta.initialHeight;
tauri.app.windows[0].minWidth = meta.minWidth;
tauri.app.windows[0].minHeight = meta.minHeight;
fs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);

let cargo = fs.readFileSync(cargoPath, "utf8");
cargo = cargo.replace(/^version = "[^"]+"/m, `version = "${meta.version}"`);
cargo = cargo.replace(/^description = "[^"]+"/m, `description = "${meta.description}"`);
fs.writeFileSync(cargoPath, cargo);

fs.mkdirSync(path.dirname(versionPath), { recursive: true });
fs.writeFileSync(
  versionPath,
  `// package.json から自動生成。直接編集しないでください。\nexport const APP_VERSION = "${meta.version}";\n`
);
console.log(`Metadata synchronized: ${meta.productName} ${meta.version}`);

