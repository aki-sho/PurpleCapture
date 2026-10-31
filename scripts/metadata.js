import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export function readMetadata() {
  const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
  const extra = pkg.purpleCapture;
  if (!extra || !pkg.productName || !pkg.version) {
    throw new Error("package.json のアプリ情報が不足しています。");
  }
  return {
    packageName: pkg.name,
    productName: pkg.productName,
    version: pkg.version,
    description: pkg.description,
    ...extra
  };
}

export function artifactNames(meta, { unsignedPreview = false } = {}) {
  const suffix = unsignedPreview ? "-Unsigned" : "";
  const prefix = `${meta.executableBaseName}-Portable-${meta.version}${suffix}`;
  return {
    exe: `${prefix}.exe`,
    exeHash: `${prefix}.exe.sha256`,
    zip: `${prefix}.zip`,
    zipHash: `${prefix}.zip.sha256`,
    folder: prefix,
    setup: `${meta.executableBaseName}-Setup-${meta.version}${suffix}.exe`,
    setupHash: `${meta.executableBaseName}-Setup-${meta.version}${suffix}.exe.sha256`
  };
}
