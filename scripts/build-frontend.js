import fs from "node:fs";
import path from "node:path";
import { build } from "esbuild";
import { root } from "./metadata.js";

const output = path.join(root, "build", "frontend");
fs.mkdirSync(output, { recursive: true });
await build({
  entryPoints: {
    main: path.join(root, "src", "main.js"),
    share: path.join(root, "src", "share.js")
  },
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  outdir: output,
  minify: false,
  sourcemap: false,
  logLevel: "info"
});
for (const file of ["index.html", "style.css", "share.html", "share.css"]) {
  fs.copyFileSync(path.join(root, "src", file), path.join(output, file));
}
console.log(`Frontend built: ${output}`);
