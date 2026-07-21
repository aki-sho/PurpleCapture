import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { root } from "./metadata.js";

const source = path.join(root, "assets", "app-icon.svg");
const output = path.join(root, "src-tauri", "icons");
const required = ["32x32.png", "128x128.png", "128x128@2x.png", "icon.ico"];
if (!fs.existsSync(source)) throw new Error(`Icon source not found: ${source}`);
if (required.every((name) => fs.existsSync(path.join(output, name)))) {
  console.log("Tauri icons already exist.");
  process.exit(0);
}
fs.mkdirSync(output, { recursive: true });
const cli = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const result = spawnSync(process.execPath, [cli, "icon", source, "--output", output], {
  cwd: root,
  stdio: "inherit"
});
if (result.status !== 0) process.exit(result.status ?? 1);

