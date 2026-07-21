# Purple Capture

Purple Capture はWindows 10・11（64ビット）向けのポータブル画面録画アプリです。
モニター、開いているアプリウィンドウ、外部ブラウザで共有したタブを選び、
H.264/MP4で録画できます。Tauri 2、Rust、Vanilla JavaScript、WebView2で構成し、
FFmpegなどの外部EXEには依存しません。

## 主な機能

- Windows.Graphics.Captureによるモニター／ウィンドウ録画
- Media FoundationによるH.264/MP4エンコード
- 30/60 FPS、標準／高画質
- WASAPIによるシステム音声と選択マイクの録音
- 一時停止、再開、安全な停止、対象終了検出
- 外部Chrome／Edgeの`getDisplayMedia()`によるブラウザタブの明示的な共有
- Purple Capture内では共有選択を出さず、外部ブラウザで選択したタブ映像を受信
- 共有映像をCPU Canvasへ合成した専用プレビューをWindows.Graphics.Captureで録画
- 保存先設定、日時ファイル名、録画履歴
- EXE横のPortableData、単一起動、異常終了後の一時ファイル整理


内蔵ブラウザや専用のブラウザメニューはありません。Webサイトの閲覧は普段利用している
Chrome／Edgeなどで行い、録画したいタブだけをブラウザ標準の共有画面で選びます。

外部ブラウザで取得した映像は、共有中だけ起動する127.0.0.1のシグナリングと
ローカルWebRTC接続で専用プレビューへ送ります。ランダムトークンを持たない
接続は共有セッションへアクセスできません。映像フレームをJavaScriptからRustへ
逐次送信せず、専用プレビューをWindows.Graphics.Captureで取得します。音声は
共有ストリームからではなく、メイン画面の「システム音声」設定に従いWASAPIで
録音します。

## 使い方

1. Purple Captureを起動します。
2. 「設定」から録画の保存先、画質、FPS、使用するマイクを設定します。
3. 「録画」画面で、録画対象を選択します。
   - モニター
   - ウィンドウ
   - 共有タブ
4. 必要に応じて「システム音声」と「マイク」をオンにします。
5. 「録画を開始」を押すと録画が始まります。
6. 録画中は、一時停止、再開、停止を操作できます。
7. 録画を停止すると、設定した保存先へMP4ファイルが保存されます。
8. 保存した録画は「録画履歴」から確認できます。

### ブラウザタブを録画する場合

1. 「共有タブ」を選択します。
2. ChromeまたはEdgeに表示される共有画面から、録画したいタブを選択します。
3. Purple Captureへ戻り、「録画を開始」を押します。

ブラウザタブの音声を録音する場合は、「システム音声」をオンにしてください。

## 開発

必要環境はWindows 10/11、Node.js 20以降、Rust stable（MSVC toolchain）、
Microsoft Edge WebView2 Runtime、Visual Studio Build ToolsのC++構成です。

```powershell
npm install
npm start
```

開発時のデータは `.devdata/PurpleCapture-PortableData/` に保存されます。

## 検査とビルド

```powershell
npm run check
npm run package:portable
npm run release:portable
```

`release:portable` はポータブル版と現在のユーザー向けNSISインストール版を作り、
`dist/` に次の6ファイルを生成します。

- `PurpleCapture-Portable-1.0.0.exe`
- `PurpleCapture-Portable-1.0.0.exe.sha256`
- `PurpleCapture-Portable-1.0.0.zip`
- `PurpleCapture-Portable-1.0.0.zip.sha256`
- `PurpleCapture-Setup-1.0.0.exe`
- `PurpleCapture-Setup-1.0.0.exe.sha256`

Setup版はWindowsのアンインストール情報とスタートメニュー項目を登録します。
レジストリ登録を避けたい場合はポータブル版を使用してください。

SHA-256はPowerShellの
`Get-FileHash .\dist\PurpleCapture-Portable-1.0.0.zip -Algorithm SHA256`
で確認できます。

## ポータブルデータ

配布版ではEXEと同じフォルダに `PurpleCapture-PortableData/` を作り、設定、
録画、ログ、WebView2データ、Cookie、localStorage、IndexedDB、一時ファイルを
保存します。`Program Files`など書き込みできない場所には置かないでください。

```text
PurpleCapture-PortableData/
├─ settings/
├─ data/
│  └─ recordings/
├─ logs/
├─ cache/
│  └─ webview/
└─ temp/
   ├─ working/
   └─ bin/
```

## ソース構成

- `src/`: メインUI、外部ブラウザ共有、録画、履歴、設定
- `src-tauri/src/`: Rustコマンド、録画、音声、パス、終了処理
- `scripts/`: メタデータ同期、検査、ポータブル配布
- `.github/workflows/`: Windows手動検査

## 外部ライブラリとバイナリ

依存ライブラリは `package-lock.json` と `src-tauri/Cargo.lock` に固定します。
外部メディアバイナリやsidecarは使用しません。Windows標準のMedia Foundation
H.264 encoder availabilityはPC構成に依存します。

## 公開前

- 本体ライセンスを決定し、`LICENSE`を差し替える
- npm/Cargo依存ライセンスとH.264の配布条件を法務確認する
- 署名用証明書でEXEへコード署名する
- 実機で複数GPU、複数モニター、各種音声デバイス、長時間録画を検証する
- `npm audit`、Rust依存監査、マルウェアスキャンを実行する
