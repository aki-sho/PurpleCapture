import fs from "node:fs";
import path from "node:path";
import { artifactNames, readMetadata, root } from "./metadata.js";

export function preparePortableLayout(sourceExe) {
  const meta = readMetadata();
  const names = artifactNames(meta);
  const dist = path.join(root, "dist");
  const stage = path.join(root, ".tmp", names.folder);
  fs.rmSync(stage, { recursive: true, force: true });
  fs.mkdirSync(stage, { recursive: true });
  fs.copyFileSync(sourceExe, path.join(stage, names.exe));
  const readme = `${meta.productName} ポータブル版 ${meta.version}

本配布物はインストーラーを使用しないポータブル版です。

1. このフォルダを、書き込み可能な場所へ移動してください。
2. ${names.exe}を起動してください。
3. アプリのデータは${meta.portableDataName}へ保存されます。
4. 移動する場合はフォルダごと移動してください。
5. 削除する場合はフォルダごと削除してください。
6. Program Filesには配置しないでください。
7. 実行にはMicrosoft Edge WebView2 Runtimeが必要です。

インストール版:
同じリリースの${names.setup}を使用すると、現在のWindowsユーザー向けに
インストールできます。ポータブル版とは保存場所とアンインストール方法が異なります。
`;
  fs.writeFileSync(path.join(stage, "README.txt"), readme, "utf8");
  fs.mkdirSync(dist, { recursive: true });
  fs.copyFileSync(sourceExe, path.join(dist, names.exe));
  return { meta, names, dist, stage };
}
