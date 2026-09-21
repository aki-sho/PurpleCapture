import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { artifactNames, readMetadata, root } from "./metadata.js";
import { preparePortableLayout } from "./prepare-portable-release.js";

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit", shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function sha256(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function writeHash(file, output) {
  fs.writeFileSync(output, `${sha256(file)}  ${path.basename(file)}\n`, "ascii");
}

const meta = readMetadata();
const names = artifactNames(meta);
const distDirectory = path.join(root, "dist");
fs.mkdirSync(distDirectory, { recursive: true });
function clearBuildDirectory(target, allowedRoot) {
  const resolved = path.resolve(target);
  const allowed = path.resolve(allowedRoot) + path.sep;
  if (!resolved.startsWith(allowed)) throw new Error(`Unsafe build cleanup path: ${resolved}`);
  fs.rmSync(resolved, { recursive: true, force: true });
}
clearBuildDirectory(path.join(root, ".tmp", names.folder), path.join(root, ".tmp"));
clearBuildDirectory(path.join(root, "src-tauri", "target", "release", "bundle", "nsis"), path.join(root, "src-tauri", "target"));
run(process.execPath, [path.join(root, "scripts", "sync-app-metadata.js")]);
run(process.execPath, [path.join(root, "scripts", "generate-notices.js")]);
run(process.execPath, [path.join(root, "scripts", "check-project.js")]);
run(process.execPath, [path.join(root, "scripts", "prepare-icon.js")]);
run(process.execPath, [path.join(root, "scripts", "build-frontend.js")]);
run(process.execPath, [
  path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js"),
  "build",
  "--no-bundle"
]);

const builtExe = path.join(root, "src-tauri", "target", "release", `${meta.executableBaseName}.exe`);
if (!fs.existsSync(builtExe)) throw new Error(`Release EXE not found: ${builtExe}`);
const layout = preparePortableLayout(builtExe);
const exe = path.join(layout.dist, names.exe);
const zip = path.join(layout.dist, names.zip);
writeHash(exe, path.join(layout.dist, names.exeHash));

const ps = [
  "Compress-Archive",
  "-LiteralPath",
  `'${layout.stage.replaceAll("'", "''")}'`,
  "-DestinationPath",
  `'${zip.replaceAll("'", "''")}'`,
  "-CompressionLevel",
  "Optimal",
  "-Force"
].join(" ");
run("powershell.exe", ["-NoProfile", "-Command", ps]);
writeHash(zip, path.join(layout.dist, names.zipHash));

run(process.execPath, [
  path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js"),
  "build",
  "--bundles",
  "nsis",
  "--config",
  path.join(root, "src-tauri", "tauri.setup.conf.json")
]);
const nsisDirectory = path.join(root, "src-tauri", "target", "release", "bundle", "nsis");
const generatedInstallers = fs.existsSync(nsisDirectory)
  ? fs.readdirSync(nsisDirectory)
    .filter((name) => name.toLowerCase().endsWith(".exe"))
    .map((name) => path.join(nsisDirectory, name))
  : [];
if (generatedInstallers.length !== 1) {
  throw new Error(`Expected one NSIS installer, found ${generatedInstallers.length}`);
}
const setup = path.join(layout.dist, names.setup);
fs.copyFileSync(generatedInstallers[0], setup);
writeHash(setup, path.join(layout.dist, names.setupHash));

const entries = spawnSync("powershell.exe", [
  "-NoProfile",
  "-Command",
  `Add-Type -AssemblyName System.IO.Compression.FileSystem; [IO.Compression.ZipFile]::OpenRead('${zip.replaceAll("'", "''")}').Entries.FullName`
], { cwd: root, encoding: "utf8" });
if (entries.status !== 0) throw new Error(entries.stderr);
const normalized = entries.stdout.trim().split(/\r?\n/).filter(Boolean).map((v) => v.replaceAll("\\", "/"));
const expected = [names.exe, "README.txt", "LICENSE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_LICENSES.txt"].map((file) => `${names.folder}/${file}`);
if (JSON.stringify(normalized.sort()) !== JSON.stringify(expected.sort())) {
  throw new Error(`ZIP content mismatch:\n${normalized.join("\n")}`);
}
for (const name of [
  names.exe,
  names.exeHash,
  names.zip,
  names.zipHash,
  names.setup,
  names.setupHash
]) {
  if (!fs.existsSync(path.join(layout.dist, name))) throw new Error(`Missing artifact: ${name}`);
}
console.log(`Portable and installer release completed: ${layout.dist}`);
