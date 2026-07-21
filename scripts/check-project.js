import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { artifactNames, readMetadata, root } from "./metadata.js";

const meta = readMetadata();
const names = artifactNames(meta);
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const tauri = JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const cargo = fs.readFileSync(path.join(root, "src-tauri", "Cargo.toml"), "utf8");
const lib = fs.readFileSync(path.join(root, "src-tauri", "src", "lib.rs"), "utf8");
const paths = fs.readFileSync(path.join(root, "src-tauri", "src", "portable_paths.rs"), "utf8");
const processManager = fs.readFileSync(path.join(root, "src-tauri", "src", "process_manager.rs"), "utf8");
const capability = JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "capabilities", "main.json"), "utf8"));
const setupConfig = JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "tauri.setup.conf.json"), "utf8"));

const required = [
  "src/index.html", "src/main.js", "src/style.css",
  "src/share.html", "src/share.js", "src/share.css",
  "src/share-sender.html", "src/share-sender.js", "src/share-sender.css",
  "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json",
  "src-tauri/tauri.setup.conf.json",
  "src-tauri/capabilities/main.json", "src-tauri/capabilities/share.json",
  "src-tauri/src/main.rs", "src-tauri/src/lib.rs", "src-tauri/src/share.rs",
  "src-tauri/src/share_server.rs",
  "scripts/smoke-share-window.ps1",
  "scripts/smoke-external-share.js",
  "scripts/smoke-share-relay.js",
  "scripts/smoke-app-share-receiver.js",
  "scripts/inspect-running-share.js",
  "scripts/smoke-shared-recording.js",
  "scripts/fixtures/share-target.html",
  "scripts/fixtures/share-receiver.html",
  "scripts/fixtures/share-app-receiver.html",
  "scripts/fixtures/synthetic-sender.html",
  "README.md", "CHANGELOG.md", "LICENSE", "THIRD_PARTY_NOTICES.md",
  ".github/workflows/windows-check.yml"
];
const failures = [];
for (const file of required) if (!fs.existsSync(path.join(root, file))) failures.push(`Missing: ${file}`);
for (const [name, version] of Object.entries({ ...pkg.dependencies, ...pkg.devDependencies })) {
  if (name.startsWith("@tauri-apps/") && !String(version).startsWith("2.")) {
    failures.push(`${name} must be version 2.x`);
  }
}
if (!/^tauri\s*=\s*\{[^}]*version\s*=\s*"2\./m.test(cargo)) failures.push("Cargo tauri must be 2.x");
if (!/^tauri-plugin-single-instance\s*=\s*"2\./m.test(cargo)) failures.push("single-instance plugin must be 2.x");
if (tauri.productName !== meta.productName || tauri.version !== meta.version || tauri.identifier !== meta.identifier) {
  failures.push("package.json / tauri.conf.json metadata mismatch");
}
if (!new RegExp(`^version = "${meta.version.replaceAll(".", "\\.")}"`, "m").test(cargo)) {
  failures.push("package.json / Cargo.toml version mismatch");
}
if (!pkg.scripts["package:portable"]?.includes("--no-bundle")) failures.push("package:portable must use --no-bundle");
if (!pkg.scripts.start?.includes("build:frontend")) failures.push("npm start must build the frontend");
if (tauri.bundle?.active !== false) failures.push("Tauri bundle.active must be false");
if (setupConfig.bundle?.active !== true || !setupConfig.bundle?.targets?.includes("nsis")) failures.push("NSIS setup configuration is missing");
if (setupConfig.bundle?.windows?.nsis?.installMode !== "currentUser") failures.push("NSIS installer must use currentUser mode");
if (!Array.isArray(tauri.app?.security?.capabilities)) failures.push("Capabilities are not configured");
const perms = capability.permissions ?? [];
if (perms.some((p) => String(p).includes(":default") && String(p).startsWith("fs:"))) failures.push("Broad fs permission found");
if (perms.some((p) => String(p).includes("shell:allow-execute") || String(p).includes("shell:allow-spawn"))) failures.push("Shell execution permission found");
if (!paths.includes("current_exe") || !paths.includes("PortableData") || !paths.includes("cache\")") || !paths.includes("webview")) failures.push("PortableData paths are incomplete");
if (/AppData|app_data_dir/i.test(paths)) failures.push("AppData must not be used");
if (!lib.includes("tauri_plugin_single_instance::init")) failures.push("Single-instance initialization missing");
if (!processManager.includes("stop_all")) failures.push("External process shutdown hook missing");
if (tauri.app.withGlobalTauri !== false) failures.push("withGlobalTauri must be false");
if (!tauri.app.security.csp) failures.push("CSP is missing");
if (!tauri.app.security.csp.includes("media-src") || !tauri.app.security.csp.includes("blob:")) failures.push("Share preview CSP is incomplete");
if (tauri.app.windows?.[0]?.create !== false) failures.push("Main window must be created programmatically with PortableData");
if (!lib.includes("WebviewWindowBuilder::from_config") || !lib.includes(".data_directory(paths.webview.clone())")) failures.push("Main WebView PortableData directory is missing");
const shareReceiver = fs.readFileSync(path.join(root, "src", "share.js"), "utf8");
const shareSender = fs.readFileSync(path.join(root, "src", "share-sender.js"), "utf8");
const shareMarkup = fs.readFileSync(path.join(root, "src", "share.html"), "utf8");
const mainMarkup = fs.readFileSync(path.join(root, "src", "index.html"), "utf8");
const recordingManager = fs.readFileSync(path.join(root, "src-tauri", "src", "recording", "manager.rs"), "utf8");
if (!shareSender.includes("getDisplayMedia") || !shareSender.includes("RTCPeerConnection")) failures.push("External browser tab share flow is missing");
if (!shareReceiver.includes("RTCPeerConnection") || !shareReceiver.includes("share_session_info")) failures.push("Share receiver flow is missing");
if (!shareReceiver.includes("share_browser_options") || !shareReceiver.includes("share_launch_browser")) failures.push("Share browser selection flow is missing");
if (!shareReceiver.includes("willReadFrequently") || !shareReceiver.includes("requestVideoFrameCallback")) failures.push("Share black-screen canvas mitigation is missing");
if (shareReceiver.includes("getDisplayMedia") || shareMarkup.includes("start-direct-share")) failures.push("Display-media selection must only run in the external browser sender");
if (recordingManager.includes("共有映像が黒い") || recordingManager.includes("shared_source")) failures.push("Shared recordings must not be rejected only because frames are dark");
if (!fs.readFileSync(path.join(root, "src", "features", "share.js"), "utf8").includes("withTimeout")) failures.push("Share window open timeout is missing");
if (!mainMarkup.includes('id="share-panel"') || !mainMarkup.includes('id="share-tab"')) failures.push("Recording-page share controls are missing");
if (mainMarkup.includes('data-page="browser"') || mainMarkup.includes('id="page-browser"')) failures.push("Removed embedded browser menu is still present");
if (!fs.readFileSync(path.join(root, "src-tauri", "src", "share.rs"), "utf8").includes(".data_directory(self.paths.webview.clone())")) failures.push("Share WebView PortableData directory is missing");
if (!tauri.app.security.capabilities.includes("share-preview")) failures.push("Share preview capability is not configured");
if (!names.exe.endsWith(`-${meta.version}.exe`)) failures.push("Artifact naming failure");
if (!names.setup.endsWith(`-${meta.version}.exe`)) failures.push("Setup artifact naming failure");

function collectJavaScript(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) return collectJavaScript(target);
    return entry.isFile() && entry.name.endsWith(".js") ? [target] : [];
  });
}
for (const file of [...collectJavaScript(path.join(root, "src")), ...collectJavaScript(path.join(root, "scripts"))]) {
  const result = spawnSync(process.execPath, ["--check", file], { encoding: "utf8" });
  if (result.status !== 0) failures.push(`JavaScript syntax error: ${path.relative(root, file)}\n${result.stderr}`);
}

if (failures.length) {
  console.error(failures.map((v) => `ERROR: ${v}`).join("\n"));
  process.exit(1);
}
console.log(`Project check OK: ${meta.productName} ${meta.version}`);
console.log(`Expected artifacts: ${names.exe}, ${names.exeHash}, ${names.zip}, ${names.zipHash}, ${names.setup}, ${names.setupHash}`);
