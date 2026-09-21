import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { artifactNames, readMetadata, root } from "./metadata.js";

const names = artifactNames(readMetadata());
const license = fs.readFileSync(path.join(root, "LICENSE"), "utf8");
const cargo = fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf8");
if (/未決定|未適用|Undecided/.test(license + cargo)) {
  throw new Error("本体ライセンスが未確定のため公開できません。");
}
for (const [asset, hashName] of [[names.exe, names.exeHash], [names.zip, names.zipHash], [names.setup, names.setupHash]]) {
  const bytes = fs.readFileSync(path.join(root, "dist", asset));
  const actual = crypto.createHash("sha256").update(bytes).digest("hex");
  const expected = fs.readFileSync(path.join(root, "dist", hashName), "utf8").trim();
  if (expected !== `${actual}  ${asset}`) throw new Error(`Hash mismatch: ${asset}`);
}
const exe = fs.readFileSync(path.join(root, "dist", names.exe));
for (const name of ["LICENSE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_LICENSES.txt"]) {
  const content = fs.readFileSync(path.join(root, name));
  if (!exe.includes(content)) throw new Error(`The executable does not embed the current ${name}`);
}
console.log("Release verified: finalized license, embedded notices and all SHA-256 hashes match.");
