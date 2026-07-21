# Purple Capture

![Version](https://img.shields.io/badge/version-1.0.0-7C3AED?style=flat-square)
![Windows](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?style=flat-square&logo=windows11&logoColor=white)
![Tauri](https://img.shields.io/badge/Tauri-2-FFC131?style=flat-square&logo=tauri&logoColor=111827)
![Rust](https://img.shields.io/badge/Rust-stable-CE422B?style=flat-square&logo=rust&logoColor=white)
![JavaScript](https://img.shields.io/badge/JavaScript-Vanilla-F7DF1E?style=flat-square&logo=javascript&logoColor=111827)
![Video](https://img.shields.io/badge/Video-H.264%20%2F%20MP4-22C55E?style=flat-square)
![Portable](https://img.shields.io/badge/Portable-supported-8B5CF6?style=flat-square)
![FFmpeg](https://img.shields.io/badge/FFmpeg-not%20required-4B5563?style=flat-square)

**Purple Capture** は、Windows 10・11（64ビット）向けのポータブル画面録画アプリです。

モニター、開いているアプリウィンドウ、Chrome／Edgeで共有したブラウザタブを選択し、H.264／MP4形式で録画できます。

Tauri 2、Rust、Vanilla JavaScript、WebView2で構成されており、FFmpegなどの外部EXEには依存しません。


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

## 録画対象

Purple Captureでは、次の録画対象を選択できます。

- 接続されているモニター
- 現在開いているアプリウィンドウ
- Chrome／Edgeで共有したブラウザタブ

内蔵ブラウザや専用のWebブラウザ機能はありません。

Webサイトの閲覧は普段利用しているChrome／Edgeなどで行い、録画したいタブだけをブラウザ標準の共有画面から選択します。

## ブラウザタブ録画の仕組み

ブラウザタブの共有には、Chrome／Edgeの`getDisplayMedia()`を使用します。

Purple Capture内には共有対象の選択画面を表示せず、外部ブラウザで選択されたタブ映像を受信します。

共有映像は、共有中のみ起動する`127.0.0.1`のシグナリングサーバーと、ローカルWebRTC接続を通じて専用プレビューへ送信されます。

ランダムトークンを持たない接続は、共有セッションへアクセスできません。

映像フレームをJavaScriptからRustへ逐次送信する方式ではなく、共有映像をCPU Canvasへ合成した専用プレビューをWindows.Graphics.Captureで録画します。

音声は共有ストリームから取得せず、メイン画面の「システム音声」設定に従ってWASAPIで録音します。

## 技術構成

| 項目 | 使用技術 |
|---|---|
| デスクトップ基盤 | Tauri 2 |
| バックエンド | Rust |
| フロントエンド | Vanilla JavaScript |
| WebView | Microsoft Edge WebView2 |
| 画面取得 | Windows.Graphics.Capture |
| 動画エンコード | Media Foundation |
| 音声取得 | WASAPI |
| ブラウザ共有 | WebRTC／getDisplayMedia |
| 出力形式 | H.264／MP4 |

## 必要環境

開発には次の環境が必要です。

- Windows 10／11（64ビット）
- Node.js 20以降
- Rust stable
- MSVC toolchain
- Microsoft Edge WebView2 Runtime
- Visual Studio Build ToolsのC++構成

## 開発

依存パッケージをインストールします。

```powershell
npm install
```

開発環境を起動します。

```powershell
npm start
```

開発時のデータは、次のフォルダに保存されます。

```text
.devdata/PurpleCapture-PortableData/
```

## 検査とビルド

コードと設定を検査します。

```powershell
npm run check
```

ポータブル版を作成します。

```powershell
npm run package:portable
```

ポータブル版とインストール版をまとめて作成します。

```powershell
npm run release:portable
```

## 配布ファイル

`npm run release:portable`を実行すると、`dist/`に次のファイルが生成されます。

```text
dist/
├─ PurpleCapture-Portable-1.0.0.exe
├─ PurpleCapture-Portable-1.0.0.exe.sha256
├─ PurpleCapture-Portable-1.0.0.zip
├─ PurpleCapture-Portable-1.0.0.zip.sha256
├─ PurpleCapture-Setup-1.0.0.exe
└─ PurpleCapture-Setup-1.0.0.exe.sha256
```

### ポータブル版

レジストリ登録やインストールを行わず、展開したフォルダから起動できます。

### Setup版

現在のWindowsユーザー向けにインストールされ、次の情報を登録します。

- Windowsのアンインストール情報
- スタートメニュー項目

レジストリ登録を避けたい場合は、ポータブル版を使用してください。

## SHA-256の確認

PowerShellから次のコマンドを実行します。

```powershell
Get-FileHash .\dist\PurpleCapture-Portable-1.0.0.zip -Algorithm SHA256
```

表示されたハッシュ値と、配布されている`.sha256`ファイルの内容を比較してください。

## ポータブルデータ

配布版では、実行ファイルと同じ場所に`PurpleCapture-PortableData/`を作成します。

このフォルダには、次のデータが保存されます。

- アプリ設定
- 録画ファイル
- ログ
- WebView2データ
- Cookie
- localStorage
- IndexedDB
- 一時ファイル

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

`Program Files`など、通常ユーザーが書き込めない場所には配置しないでください。

## ソース構成

```text
PurpleCapture/
├─ src/
├─ src-tauri/
│  └─ src/
├─ scripts/
├─ assets/
├─ build/
├─ dist/
└─ .github/
   └─ workflows/
```

| フォルダ | 内容 |
|---|---|
| `src/` | メインUI、外部ブラウザ共有、録画、履歴、設定 |
| `src-tauri/src/` | Rustコマンド、録画、音声、パス、終了処理 |
| `scripts/` | メタデータ同期、検査、ポータブル配布 |
| `assets/` | アプリで使用する画像など |
| `build/` | ビルド関連ファイル |
| `dist/` | 生成された配布ファイル |
| `.github/workflows/` | Windows環境での手動検査 |

## 外部ライブラリとバイナリ

JavaScriptの依存ライブラリは`package-lock.json`、Rustの依存ライブラリは`src-tauri/Cargo.lock`に固定されています。

外部メディアバイナリやsidecarは使用していません。

Windows標準のMedia Foundation H.264 Encoderが使用可能かどうかは、PCのWindowsエディションや構成によって異なる場合があります。

## 公開前チェック

- [ ] 本体ライセンスを決定し、`LICENSE`を差し替える
- [ ] npm依存ライブラリのライセンスを確認する
- [ ] Cargo依存ライブラリのライセンスを確認する
- [ ] H.264に関する配布条件を確認する
- [ ] 署名用証明書でEXEへコード署名する
- [ ] 複数GPU環境で録画を検証する
- [ ] 複数モニター環境で録画を検証する
- [ ] 各種音声デバイスで録音を検証する
- [ ] 長時間録画を検証する
- [ ] `npm audit`を実行する
- [ ] Rust依存ライブラリの監査を実行する
- [ ] 配布ファイルのマルウェアスキャンを実行する