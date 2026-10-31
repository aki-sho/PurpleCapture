import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { artifactNames, readMetadata, root } from "./metadata.js";
import { assertReleaseSignature } from "./release-signature.js";

const unsignedPreview = process.argv.includes("--unsigned-preview");
if (process.argv.slice(2).some((arg) => arg !== "--unsigned-preview")) throw new Error("Unknown release argument");
const names = artifactNames(readMetadata(), { unsignedPreview });
const distribution = unsignedPreview ? "dist-unsigned" : "dist";
const license = fs.readFileSync(path.join(root, "LICENSE"), "utf8");
const cargo = fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf8");
if (/未決定|未適用|Undecided/.test(license + cargo)) {
  throw new Error("本体ライセンスが未確定のため公開できません。");
}
for (const [asset, hashName] of [[names.exe, names.exeHash], [names.zip, names.zipHash], [names.setup, names.setupHash]]) {
  const bytes = fs.readFileSync(path.join(root, distribution, asset));
  const actual = crypto.createHash("sha256").update(bytes).digest("hex");
  const expected = fs.readFileSync(path.join(root, distribution, hashName), "utf8").trim();
  if (expected !== `${actual}  ${asset}`) throw new Error(`Hash mismatch: ${asset}`);
}
const exe = fs.readFileSync(path.join(root, distribution, names.exe));
for (const name of ["LICENSE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_LICENSES.txt"]) {
  const content = fs.readFileSync(path.join(root, name));
  if (!exe.includes(content)) throw new Error(`The executable does not embed the current ${name}`);
}
for (const asset of [names.exe, names.setup]) {
  const file = path.join(root, distribution, asset).replaceAll("'", "''");
  const verification = spawnSync("powershell.exe", ["-NoProfile", "-Command",
    `$ErrorActionPreference = 'Stop'; Import-Module "$PSHOME\\Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1"; $s = Get-AuthenticodeSignature -LiteralPath '${file}'; @{status=[string]$s.Status;timestamped=($null -ne $s.TimeStamperCertificate);signer=$s.SignerCertificate.Subject} | ConvertTo-Json -Compress`
  ], { encoding: "utf8" });
  if (verification.error) throw verification.error;
  if (verification.status !== 0) throw new Error(`Signature verification failed: ${asset}\n${verification.stderr}`);
  assertReleaseSignature(JSON.parse(verification.stdout.trim()), { unsignedPreview });
  console.log(`${asset}: ${verification.stdout.trim()}`);
}
console.log(unsignedPreview
  ? "Unsigned preview verified: finalized license, embedded notices, SHA-256 hashes and signature status checked."
  : "Release verified: finalized license, embedded notices, SHA-256 hashes and trusted timestamped signatures match.");
