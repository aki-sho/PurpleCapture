use crate::models::CaptureSource;
use anyhow::{Context, Result, bail};
use std::process;
use windows_capture::{monitor::Monitor, window::Window};

pub fn list_monitors() -> Result<Vec<CaptureSource>> {
    let monitors = Monitor::enumerate().context("モニターの一覧を取得できません。")?;
    Ok(monitors
        .into_iter()
        .filter_map(|monitor| {
            let index = monitor.index().ok()?;
            let width = monitor.width().ok()?;
            let height = monitor.height().ok()?;
            let name = monitor
                .name()
                .unwrap_or_else(|_| format!("モニター {index}"));
            let refresh = monitor.refresh_rate().unwrap_or_default();
            Some(CaptureSource {
                id: format!("monitor:{index}"),
                kind: "monitor".into(),
                name,
                detail: format!("{width} × {height} · {refresh} Hz"),
            })
        })
        .collect())
}

pub fn list_windows() -> Result<Vec<CaptureSource>> {
    let own_pid = process::id();
    let windows = Window::enumerate().context("ウィンドウの一覧を取得できません。")?;
    let mut result: Vec<_> = windows
        .into_iter()
        .filter(|window| window.process_id().is_ok_and(|pid| pid != own_pid))
        .filter_map(|window| {
            let title = window.title().ok()?.trim().to_string();
            if title.is_empty() {
                return None;
            }
            let process_name = window.process_name().unwrap_or_else(|_| "アプリ".into());
            let width = window.width().unwrap_or_default().max(0);
            let height = window.height().unwrap_or_default().max(0);
            Some(CaptureSource {
                id: format!("window:{}", window.as_raw_hwnd() as usize),
                kind: "window".into(),
                name: title,
                detail: format!("{process_name} · {width} × {height}"),
            })
        })
        .collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(result)
}

pub fn resolve_source(source: &CaptureSource) -> Result<ResolvedSource> {
    match source.kind.as_str() {
        "monitor" => {
            let index = source
                .id
                .strip_prefix("monitor:")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|index| *index > 0)
                .context("モニターIDが不正です。")?;
            let monitor =
                Monitor::from_index(index).context("選択したモニターが見つかりません。")?;
            let width = monitor.width()?;
            let height = monitor.height()?;
            Ok(ResolvedSource::Monitor {
                monitor,
                width,
                height,
            })
        }
        "window" => {
            let hwnd = source
                .id
                .strip_prefix("window:")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value != 0)
                .context("ウィンドウIDが不正です。")?;
            let window = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
            if !window.is_valid() || window.process_id().is_ok_and(|pid| pid == process::id()) {
                bail!("選択したウィンドウは録画できません。");
            }
            let width = window.width()?.max(1) as u32;
            let height = window.height()?.max(1) as u32;
            Ok(ResolvedSource::Window {
                window,
                width,
                height,
            })
        }
        _ => bail!("録画対象の種類が不正です。"),
    }
}

pub enum ResolvedSource {
    Monitor {
        monitor: Monitor,
        width: u32,
        height: u32,
    },
    Window {
        window: Window,
        width: u32,
        height: u32,
    },
}
