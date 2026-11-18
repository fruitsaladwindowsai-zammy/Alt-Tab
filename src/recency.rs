//! Focus recency state and field ordering matching Fathom's `Recency.js`.

use crate::depth;
use crate::field::WindowEntry;
use crate::layout::Rect;
use std::collections::{HashMap, HashSet};

pub const SEED_STEP_SECONDS: u64 = 30;

#[derive(Clone, Debug)]
pub struct WindowCandidate {
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
    pub app_id: String,
    pub app_name: String,
    pub exe_path: String,
    pub workspace_key: String,
    pub workspace_id: Option<i64>,
    pub workspace_name: String,
    pub monitor_name: String,
    pub geometry: Option<Rect>,
    pub minimized: bool,
    pub on_current_desktop: bool,
    pub urgent: bool,
    pub z_rank: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RecencyState {
    pub last_active: HashMap<isize, u64>,
    pub active: Option<isize>,
    pub estimated: HashSet<isize>,
    pub seeded: bool,
}

impl RecencyState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_focus(&mut self, hwnd: isize, now_ms: u64) -> bool {
        if hwnd == 0 {
            return false;
        }
        if let Some(prev) = self.active {
            if prev != hwnd {
                self.last_active.insert(prev, now_ms);
                self.estimated.remove(&prev);
            }
        }
        self.last_active.insert(hwnd, now_ms);
        self.estimated.remove(&hwnd);
        self.active = Some(hwnd);
        true
    }

    pub fn record_open(&mut self, hwnd: isize, now_ms: u64) -> bool {
        if hwnd == 0 || self.last_active.contains_key(&hwnd) {
            return false;
        }
        self.last_active.insert(hwnd, now_ms);
        self.estimated.remove(&hwnd);
        true
    }

    pub fn record_close(&mut self, hwnd: isize) -> bool {
        let mut changed = self.last_active.remove(&hwnd).is_some();
        changed |= self.estimated.remove(&hwnd);
        if self.active == Some(hwnd) {
            self.active = None;
            changed = true;
        }
        changed
    }

    /// Seeds recency timestamps from current OS Z-order (`z_rank` 0 = frontmost).
    pub fn seed_from_candidates(&mut self, candidates: &[WindowCandidate], now_ms: u64) -> usize {
        let mut seeded = 0;
        for cand in candidates {
            if cand.hwnd == 0 || self.last_active.contains_key(&cand.hwnd) {
                continue;
            }
            let age_ms = (cand.z_rank as u64) * SEED_STEP_SECONDS * 1000;
            self.last_active
                .insert(cand.hwnd, now_ms.saturating_sub(age_ms));
            if cand.z_rank == 0 && self.active.is_none() {
                self.active = Some(cand.hwnd);
            } else {
                self.estimated.insert(cand.hwnd);
            }
            seeded += 1;
        }
        self.seeded = true;
        seeded
    }

    pub fn seconds_since(&self, hwnd: isize, now_ms: u64, fallback_active: Option<isize>) -> f64 {
        if hwnd == 0 {
            return f64::INFINITY;
        }
        let active = self.active.or(fallback_active);
        if Some(hwnd) == active {
            return 0.0;
        }
        match self.last_active.get(&hwnd) {
            Some(&at) => (now_ms.saturating_sub(at)) as f64 / 1000.0,
            None => f64::INFINITY,
        }
    }

    pub fn build_field(
        &self,
        candidates: &[WindowCandidate],
        now_ms: u64,
        fallback_active: Option<isize>,
    ) -> Vec<WindowEntry> {
        let active = self.active.or(fallback_active);
        let mut entries = Vec::with_capacity(candidates.len());

        for cand in candidates {
            if cand.hwnd == 0 {
                continue;
            }
            let is_active = Some(cand.hwnd) == active;
            let seconds = self.seconds_since(cand.hwnd, now_ms, fallback_active);
            let d = depth::depth_for_seconds(seconds);
            let estimated = !is_active && self.estimated.contains(&cand.hwnd);
            entries.push(WindowEntry {
                index: 0,
                hwnd: cand.hwnd,
                pid: cand.pid,
                title: cand.title.clone(),
                app_id: cand.app_id.clone(),
                app_name: cand.app_name.clone(),
                exe_path: cand.exe_path.clone(),
                workspace_key: cand.workspace_key.clone(),
                workspace_id: cand.workspace_id,
                workspace_name: cand.workspace_name.clone(),
                monitor_name: cand.monitor_name.clone(),
                geometry: cand.geometry,
                minimized: cand.minimized,
                on_current_desktop: cand.on_current_desktop,
                urgent: cand.urgent,
                active: is_active,
                estimated,
                seconds,
                depth: d,
                z_rank: cand.z_rank,
            });
        }

        entries.sort_by(|a, b| {
            if a.active != b.active {
                return if a.active {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            if a.seconds != b.seconds {
                return a
                    .seconds
                    .partial_cmp(&b.seconds)
                    .unwrap_or(std::cmp::Ordering::Equal);
            }
            if a.z_rank != b.z_rank {
                return a.z_rank.cmp(&b.z_rank);
            }
            a.hwnd.cmp(&b.hwnd)
        });

        for (i, entry) in entries.iter_mut().enumerate() {
            entry.index = i;
        }
        entries
    }
}

pub fn initial_selection(count: usize, step: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    if step > 0 {
        Some(if count > 1 { 1 } else { 0 })
    } else if step < 0 {
        Some(count - 1)
    } else {
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_cand(hwnd: isize, z: usize) -> WindowCandidate {
        WindowCandidate {
            hwnd,
            pid: hwnd as u32,
            title: format!("Win {hwnd}"),
            app_id: "app.exe".into(),
            app_name: "App".into(),
            exe_path: String::new(),
            workspace_key: "id:1".into(),
            workspace_id: Some(1),
            workspace_name: "1".into(),
            monitor_name: "DISPLAY1".into(),
            geometry: None,
            minimized: false,
            on_current_desktop: true,
            urgent: false,
            z_rank: z,
        }
    }

    #[test]
    fn active_window_leads_field_and_previous_is_second() {
        let mut rec = RecencyState::new();
        rec.record_focus(10, 1000);
        rec.record_focus(20, 5000);
        rec.record_focus(30, 10000);

        let cands = vec![dummy_cand(30, 0), dummy_cand(20, 1), dummy_cand(10, 2)];
        let field = rec.build_field(&cands, 20_000, None);
        assert_eq!(field[0].hwnd, 30);
        assert!(field[0].active);
        assert_eq!(field[1].hwnd, 20);
        assert!(!field[1].estimated);
        assert_eq!(field[2].hwnd, 10);
    }
}
