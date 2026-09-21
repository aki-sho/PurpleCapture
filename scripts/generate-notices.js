import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { root } from "./metadata.js";

const result = spawnSync("cargo", ["metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc", "--manifest-path", "src-tauri/Cargo.toml"], {
  cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024
});
if (result.status !== 0) throw new Error(result.stderr || String(result.error));
const metadata = JSON.parse(result.stdout);
const dependencies = metadata.packages.filter((p) => p.name !== "purplecapture").map((p) => ({
  ecosystem: "Cargo", name: p.name, version: p.version, license: p.license,
  directory: path.dirname(p.manifest_path), licenseFile: p.license_file
}));
const lock = JSON.parse(fs.readFileSync(path.join(root, "package-lock.json"), "utf8"));
for (const [relative, entry] of Object.entries(lock.packages)) {
  if (!relative) continue;
  const directory = path.join(root, relative);
  if (!fs.existsSync(path.join(directory, "package.json"))) continue; // other-platform optional packages
  const pkg = JSON.parse(fs.readFileSync(path.join(directory, "package.json"), "utf8"));
  dependencies.push({ ecosystem: "npm", name: pkg.name, version: pkg.version,
    license: pkg.license || entry.license, directory });
}

function licenseFiles(directory, depth = 0) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    if (entry.isFile() && /^(licen[cs]e|copying|copyright|notice)([._-]|$)/i.test(entry.name)) return [file];
    if (entry.isDirectory() && depth < 2 && /^(licen[cs]es?|legal)$/i.test(entry.name)) return licenseFiles(file, depth + 1);
    return [];
  });
}
const sections = ["Purple Capture — Third-party license texts\n\nGenerated from Cargo.lock (Windows target) and installed package-lock.json dependencies.\nThis inventory also includes build-time components; listing does not imply every component is shipped at runtime.\nEach component remains under its own license.\n"];
const missing = [];
for (const entry of dependencies.sort((a, b) => `${a.ecosystem}/${a.name}/${a.version}`.localeCompare(`${b.ecosystem}/${b.name}/${b.version}`))) {
  const files = licenseFiles(entry.directory);
  if (entry.licenseFile) {
    const explicit = path.resolve(entry.directory, entry.licenseFile);
    if (fs.existsSync(explicit) && !files.includes(explicit)) files.push(explicit);
  }
  // esbuild platform binaries use the same MIT license as their parent package.
  if (!files.length && entry.name.startsWith("@esbuild/")) files.push(path.join(root, "node_modules/esbuild/LICENSE.md"));
  if (!files.length && entry.name.startsWith("@tauri-apps/cli-")) files.push(...licenseFiles(path.join(root, "node_modules/@tauri-apps/cli")));
  const supplement = path.join(root, "third-party", "licenses", `${entry.name}-${entry.version}`);
  if (fs.existsSync(supplement)) files.push(...licenseFiles(supplement));
  if (!files.length) missing.push(`${entry.ecosystem}: ${entry.name} ${entry.version} (${entry.license})`);
  sections.push(`\n${"=".repeat(78)}\n${entry.ecosystem}: ${entry.name} ${entry.version}\nDeclared license: ${entry.license || "See license text"}\n`);
  if (entry.ecosystem === "Cargo") sections.push(`Corresponding unmodified source: https://crates.io/api/v1/crates/${entry.name}/${entry.version}/download\n`);
  for (const file of files.sort()) sections.push(`\n--- ${path.basename(file)} ---\n${fs.readFileSync(file, "utf8").replaceAll("\r\n", "\n")}\n`);
}
if (missing.length) throw new Error(`Missing license texts (release blocked):\n${missing.join("\n")}`);
const installerLicense = path.join(root, "third-party/licenses/NSIS/COPYING");
if (!fs.existsSync(installerLicense)) throw new Error("Missing NSIS license text");
sections.push(`\n${"=".repeat(78)}\nNSIS installer runtime (zlib compression)\nSource: https://nsis.sourceforge.io/License\n\n${fs.readFileSync(installerLicense, "utf8")}\n`);
fs.writeFileSync(path.join(root, "THIRD_PARTY_LICENSES.txt"), sections.join(""));
console.log(`Collected license texts for ${dependencies.length} components.`);
