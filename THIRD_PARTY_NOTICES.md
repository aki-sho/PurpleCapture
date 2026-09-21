# Third-party notices

Purple Capture本体の利用条件は同梱のLICENSEをご覧ください。第三者のコンポーネントはそれぞれのライセンスに従います。本体の条件で第三者ライセンスの権利を制限するものではありません。

THIRD_PARTY_LICENSES.txtに、Cargo.lockのWindows向け依存およびインストール済みnpm依存のライセンス・著作権表示を収録しています。ビルド時だけ使用するコンポーネントも含みます。

MPL-2.0のコンポーネント（cssparser、cssparser-macros、dtoa-short、option-ext、selectors）は変更せず使用しています。対応するソースはTHIRD_PARTY_LICENSES.txtの各項目の固定バージョンのダウンロードリンクから取得できます。

原文がパッケージに不足していたライセンスはthird-party/license-sources.jsonに記録した公式ソースから取得しました。npm run licensesで再生成でき、原文が不足した場合は配布物の生成を停止します。

主な直接依存:

- Tauri 2 / MIT or Apache-2.0
- `@tauri-apps/api`, Tauri CLI and official plugins / MIT or Apache-2.0
- `windows-capture` / MIT
- `wasapi` / MIT
- `windows` (windows-rs) / MIT or Apache-2.0
- `serde`, `serde_json` / MIT or Apache-2.0
- `chrono` / MIT or Apache-2.0
- `parking_lot` / MIT or Apache-2.0
- `uuid` / MIT or Apache-2.0
- NSIS installer runtime / zlib/libpng license

Microsoft Edge WebView2 RuntimeはポータブルZIPに同梱していません。Setup版は必要に応じてMicrosoftから取得します。Windows Media Foundation、WASAPI、Windows.Graphics.CaptureはWindowsのコンポーネントです。FFmpegその他の外部メディアEXEは同梱していません。

H.264に関する権利処理は、このライセンス一覧によって完了したとするものではありません。
