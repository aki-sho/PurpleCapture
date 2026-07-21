# Third-party notices

Purple Capture 本体のライセンスは未決定です。依存コンポーネントのライセンスは
それぞれの権利者が定める条件に従います。公開前に `cargo metadata`、
`cargo license` 相当の監査と `npm` のライセンス監査を再実行してください。

主な直接依存:

- Tauri 2 / MIT or Apache-2.0
- `@tauri-apps/api`, Tauri CLI and official plugins / MIT or Apache-2.0
- `windows-capture` / MIT
- `wasapi` / MIT
- `windows` (windows-rs) / MIT or Apache-2.0
- `serde`, `serde_json` / MIT or Apache-2.0
- `chrono` / MIT or Apache-2.0
- `parking_lot` / MIT or Apache-2.0
- `url` / MIT or Apache-2.0
- `uuid` / MIT or Apache-2.0
- NSIS installer runtime / zlib/libpng license

Microsoft Edge WebView2 Runtime、Windows Media Foundation、WASAPI、および
Windows.Graphics.Capture はWindowsのコンポーネントであり、本リポジトリには
再配布していません。H.264の利用・配布条件は配布地域と用途に応じて別途確認して
ください。FFmpegその他の外部メディアバイナリは同梱していません。
