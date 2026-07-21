use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSource {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub save_directory: String,
    pub quality: String,
    pub fps: u32,
    pub system_audio: bool,
    pub microphone: bool,
    pub microphone_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingRequest {
    pub source: CaptureSource,
    pub quality: String,
    pub fps: u32,
    pub system_audio: bool,
    pub microphone: bool,
    pub microphone_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub state: String,
    pub elapsed_seconds: u64,
    pub target_name: String,
    pub error: Option<String>,
}

impl Default for RecordingStatus {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            elapsed_seconds: 0,
            target_name: String::new(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub path: String,
    pub file_name: String,
    pub target_name: String,
    pub created_at: String,
    pub duration_seconds: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareState {
    pub window_open: bool,
    pub active: bool,
    pub title: String,
    pub display_surface: String,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

impl Default for ShareState {
    fn default() -> Self {
        Self {
            window_open: false,
            active: false,
            title: "共有タブ".into(),
            display_surface: String::new(),
            width: 0,
            height: 0,
            has_audio: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareReport {
    pub active: bool,
    pub title: String,
    pub display_surface: String,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSessionInfo {
    pub sender_url: String,
    pub api_base: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareBrowserOption {
    pub id: String,
    pub name: String,
    pub available: bool,
}

impl ShareReport {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.title.len() > 512 {
            anyhow::bail!("共有対象名が長すぎます。");
        }
        if self.active {
            if !matches!(
                self.display_surface.as_str(),
                "browser" | "window" | "monitor" | "unknown"
            ) {
                anyhow::bail!("共有対象の種類が不正です。");
            }
            if !(1..=16_384).contains(&self.width) || !(1..=16_384).contains(&self.height) {
                anyhow::bail!("共有映像のサイズが不正です。");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapData {
    pub settings: AppSettings,
    pub recording: RecordingStatus,
    pub share: ShareState,
}
