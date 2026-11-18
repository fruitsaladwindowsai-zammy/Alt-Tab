//! Core Fathom overlay state machine, animations, and input dispatch.

use crate::config::Config;
use crate::field::{self, WindowEntry, WorkspaceDescriptor, WorkspaceGroup};
use crate::layout::Rect;
use crate::palette::DerivedPalette;
use crate::recency::{self, RecencyState, WindowCandidate};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub const REVEAL_DELAY_MS: u64 = 90;
pub const WATCHDOG_TIMEOUT_MS: u64 = 15_000;
pub const CAMERA_ANIM_MS: f64 = 220.0;
pub const DEPTH_ANIM_MS: f64 = 260.0;
pub const REVEAL_ANIM_MS: f64 = 130.0;
pub const WALLPAPER_ANIM_MS: f64 = 160.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchMode {
    Hold,
    Browse,
}

#[derive(Clone, Debug)]
pub struct MonitorDesc {
    pub name: String,
    pub rect: Rect,
    pub focused: bool,
}

#[derive(Clone, Debug)]
pub struct AnimValue {
    pub current: f64,
    pub from: f64,
    pub target: f64,
    pub start_ms: f64,
    pub duration_ms: f64,
}

impl AnimValue {
    pub fn snap(value: f64) -> Self {
        Self {
            current: value,
            from: value,
            target: value,
            start_ms: 0.0,
            duration_ms: 0.0,
        }
    }

    pub fn animate_to(&mut self, target: f64, now_ms: f64, duration_ms: f64) {
        if (self.target - target).abs() < 1e-5 {
            return;
        }
        self.tick(now_ms);
        self.from = self.current;
        self.target = target;
        self.start_ms = now_ms;
        self.duration_ms = duration_ms;
    }

    pub fn tick(&mut self, now_ms: f64) -> bool {
        if (self.current - self.target).abs() < 1e-5 || self.duration_ms <= 0.0 {
            let changed = (self.current - self.target).abs() >= 1e-5;
            self.current = self.target;
            return changed;
        }
        let t = ((now_ms - self.start_ms) / self.duration_ms).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3); // OutCubic
        self.current = self.from + (self.target - self.from) * eased;
        if t >= 1.0 {
            self.current = self.target;
            false
        } else {
            true
        }
    }

    pub fn is_animating(&self) -> bool {
        (self.current - self.target).abs() >= 1e-5 && self.duration_ms > 0.0
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    None,
    Commit {
        hwnd: isize,
        on_current_desktop: bool,
        workspace_key: String,
        minimized: bool,
    },
    SwitchWorkspace {
        workspace_key: String,
    },
    Cancel,
}

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub struct Controller {
    pub config: Config,
    pub palette: DerivedPalette,
    pub recency: RecencyState,

    pub opened: bool,
    pub revealed: bool,
    pub mode: SwitchMode,
    pub pinned: bool,

    pub field: Vec<WindowEntry>,
    pub closed: HashSet<isize>,
    pub filter_text: String,
    pub order: Vec<usize>,
    pub slots: Vec<isize>,
    pub groups: Vec<WorkspaceGroup>,
    pub monitors: HashMap<String, MonitorDesc>,
    pub target_monitor: Rect,

    pub selected_index: Option<usize>,
    pub hovered_plane: Option<usize>,
    pub hovered_tile: Option<usize>,
    pub hovered_new_desktop: bool,

    pub camera_slot: AnimValue,
    pub scene_depth: AnimValue,
    pub reveal_opacity: AnimValue,
    pub wallpaper_opacity: AnimValue,
    pub camera_animated: bool,

    pub opened_at_ms: u64,
    pub revealed_at_ms: u64,
    pub last_input_ms: u64,
    pub wheel_carry_y: f64,
    pub wheel_carry_x: f64,
    pub pointer_primed: bool,
    pub pointer_last: (f64, f64),
    pub dragging_gauge: bool,
    pub drag_pending: Option<(usize, f64, f64)>,
    pub dragging_entry: Option<usize>,
    pub drag_pos: (f64, f64),
    pub drop_target_group: Option<usize>,
    pub drop_new_desktop: bool,
    pub demo_mode: bool,
}

impl Controller {
    pub fn new(config: Config, palette: DerivedPalette) -> Self {
        Self {
            config,
            palette,
            recency: RecencyState::new(),
            opened: false,
            revealed: false,
            mode: SwitchMode::Hold,
            pinned: false,
            field: Vec::new(),
            closed: HashSet::new(),
            filter_text: String::new(),
            order: Vec::new(),
            slots: Vec::new(),
            groups: Vec::new(),
            monitors: HashMap::new(),
            target_monitor: Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            selected_index: None,
            hovered_plane: None,
            hovered_tile: None,
            hovered_new_desktop: false,
            camera_slot: AnimValue::snap(0.0),
            scene_depth: AnimValue::snap(0.0),
            reveal_opacity: AnimValue::snap(0.0),
            wallpaper_opacity: AnimValue::snap(0.0),
            camera_animated: false,
            opened_at_ms: 0,
            revealed_at_ms: 0,
            last_input_ms: 0,
            wheel_carry_y: 0.0,
            wheel_carry_x: 0.0,
            pointer_primed: false,
            pointer_last: (0.0, 0.0),
            dragging_gauge: false,
            drag_pending: None,
            dragging_entry: None,
            drag_pos: (0.0, 0.0),
            drop_target_group: None,
            drop_new_desktop: false,
            demo_mode: false,
        }
    }

    pub fn selected_entry(&self) -> Option<&WindowEntry> {
        self.selected_index.and_then(|idx| self.field.get(idx))
    }

    pub fn selected_slot(&self) -> isize {
        self.selected_index
            .and_then(|idx| self.slots.get(idx).copied())
            .unwrap_or(-1)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_field(
        &mut self,
        mode: SwitchMode,
        step_dir: isize,
        candidates: &[WindowCandidate],
        workspaces: &[WorkspaceDescriptor],
        monitors: Vec<MonitorDesc>,
        target_monitor: Rect,
        fallback_active: Option<isize>,
        now_ms: u64,
    ) -> bool {
        if self.opened {
            if step_dir != 0 {
                if !self.pinned && mode == SwitchMode::Hold {
                    self.mode = SwitchMode::Hold;
                }
                self.step(step_dir, now_ms);
            }
            return true;
        }

        self.recency.seed_from_candidates(candidates, now_ms);
        let entries = self
            .recency
            .build_field(candidates, now_ms, fallback_active);

        let min_windows = if mode == SwitchMode::Hold && !self.config.open_on_single {
            2
        } else {
            1
        };
        if entries.len() < min_windows {
            return false;
        }

        self.monitors.clear();
        for m in monitors {
            self.monitors.insert(m.name.clone(), m);
        }
        self.target_monitor = target_monitor;
        self.mode = mode;
        self.pinned = false;
        self.filter_text.clear();
        self.closed.clear();
        self.wheel_carry_x = 0.0;
        self.wheel_carry_y = 0.0;
        self.pointer_primed = false;
        self.dragging_gauge = false;
        self.hovered_plane = None;
        self.hovered_tile = None;
        self.hovered_new_desktop = false;
        self.clear_drag();

        self.groups = field::workspace_groups(&entries, workspaces);
        self.order = field::visible_order(&entries, &self.closed, "", self.config.show_scratchpads);
        self.slots = field::slots_for(&self.order, entries.len());
        self.field = entries;

        let all_indices: Vec<usize> = (0..self.field.len()).collect();
        let initial = recency::initial_selection(self.field.len(), step_dir);
        let picked = field::reselect(&self.order, initial, &all_indices);

        self.camera_animated = false;
        self.select(picked, now_ms as f64);
        self.camera_animated = true;

        self.opened_at_ms = now_ms;
        self.revealed_at_ms = if mode != SwitchMode::Hold { now_ms } else { 0 };
        self.last_input_ms = now_ms;
        self.opened = true;
        self.revealed = mode != SwitchMode::Hold;
        self.reveal_opacity = if self.revealed {
            AnimValue::snap(1.0)
        } else {
            AnimValue::snap(0.0)
        };
        self.wallpaper_opacity = AnimValue::snap(0.0);
        true
    }

    pub fn reveal_now(&mut self, now_ms: f64) {
        if self.opened && !self.revealed {
            self.revealed = true;
            self.revealed_at_ms = now_ms as u64;
            self.reveal_opacity = AnimValue::snap(0.0);
            self.reveal_opacity.animate_to(1.0, now_ms, REVEAL_ANIM_MS);
        }
    }

    pub fn notify_wallpaper_ready(&mut self, now_ms: f64) {
        self.wallpaper_opacity = AnimValue::snap(0.0);
        self.wallpaper_opacity
            .animate_to(1.0, now_ms, WALLPAPER_ANIM_MS);
    }

    pub fn tick_animations(&mut self, now_ms: u64) -> bool {
        let now_f = now_ms as f64;
        let mut active = false;
        active |= self.camera_slot.tick(now_f);
        active |= self.scene_depth.tick(now_f);
        active |= self.reveal_opacity.tick(now_f);
        active |= self.wallpaper_opacity.tick(now_f);
        active
    }

    pub fn is_animating(&self) -> bool {
        self.camera_slot.is_animating()
            || self.scene_depth.is_animating()
            || self.reveal_opacity.is_animating()
            || self.wallpaper_opacity.is_animating()
    }

    pub fn check_timers(&mut self, now_ms: u64) -> Option<Action> {
        if !self.opened {
            return None;
        }
        if self.mode == SwitchMode::Hold
            && !self.revealed
            && now_ms.saturating_sub(self.opened_at_ms) >= REVEAL_DELAY_MS
        {
            self.reveal_now(now_ms as f64);
        }
        if self.mode == SwitchMode::Hold
            && now_ms.saturating_sub(self.last_input_ms) >= WATCHDOG_TIMEOUT_MS
        {
            self.dismiss();
            return Some(Action::Cancel);
        }
        None
    }

    pub fn refresh_order(&mut self, now_ms: u64) {
        let previous = self.order.clone();
        self.order = field::visible_order(
            &self.field,
            &self.closed,
            &self.filter_text,
            self.config.show_scratchpads,
        );
        self.slots = field::slots_for(&self.order, self.field.len());
        let picked = field::reselect(&self.order, self.selected_index, &previous);
        self.select(picked, now_ms as f64);
    }

    pub fn select(&mut self, index: Option<usize>, now_ms: f64) {
        let valid = index.filter(|&i| i < self.slots.len() && self.slots[i] >= 0);
        self.selected_index = valid;
        if let Some(idx) = valid {
            let target_slot = self.slots[idx] as f64;
            let target_depth = self.field[idx].depth.clamp(0.0, 8.0);
            if self.camera_animated {
                self.camera_slot
                    .animate_to(target_slot, now_ms, CAMERA_ANIM_MS);
                self.scene_depth
                    .animate_to(target_depth, now_ms, DEPTH_ANIM_MS);
            } else {
                self.camera_slot = AnimValue::snap(target_slot);
                self.scene_depth = AnimValue::snap(target_depth);
            }
        }
    }

    pub fn note_input(&mut self, now_ms: u64) {
        if self.opened {
            self.last_input_ms = now_ms;
        }
    }

    pub fn step(&mut self, delta: isize, now_ms: u64) -> bool {
        if !self.opened || self.order.is_empty() {
            return false;
        }
        let next = field::step(&self.order, self.selected_index, delta, true);
        self.select(next, now_ms as f64);
        self.note_input(now_ms);
        true
    }

    pub fn move_by(&mut self, delta: isize, now_ms: u64) -> bool {
        if !self.opened || self.order.is_empty() {
            return false;
        }
        let next = field::step(&self.order, self.selected_index, delta, false);
        self.select(next, now_ms as f64);
        self.note_input(now_ms);
        true
    }

    pub fn jump_to(&mut self, index: usize, now_ms: u64) -> bool {
        if !self.opened {
            return false;
        }
        self.select(Some(index), now_ms as f64);
        self.note_input(now_ms);
        true
    }

    pub fn workspace_step(&mut self, direction: isize, now_ms: u64) -> bool {
        if !self.opened || self.order.is_empty() {
            return false;
        }
        if let Some(target) =
            field::neighbor_workspace(&self.groups, &self.order, self.selected_index, direction)
        {
            return self.jump_to(target, now_ms);
        }
        false
    }

    pub fn workspace_jump(&mut self, number: i64, now_ms: u64) -> bool {
        if !self.opened {
            return false;
        }
        if let Some(target) = field::workspace_by_number(&self.groups, &self.order, number) {
            return self.jump_to(target, now_ms);
        }
        false
    }

    pub fn set_filter(&mut self, text: String, now_ms: u64) -> bool {
        if !self.opened {
            return false;
        }
        let trimmed = text.trim_start().to_string();
        if trimmed == self.filter_text {
            return true;
        }
        self.filter_text = trimmed;
        self.refresh_order(now_ms);
        if let Some(&first) = self.order.first() {
            self.select(Some(first), now_ms as f64);
        }
        self.note_input(now_ms);
        true
    }

    pub fn pin(&mut self) -> bool {
        if !self.opened {
            return false;
        }
        self.pinned = true;
        self.mode = SwitchMode::Browse;
        if !self.revealed {
            self.reveal_now(now_epoch_ms() as f64);
        }
        true
    }

    pub fn alt_released(&mut self) -> Action {
        if !self.opened || self.mode != SwitchMode::Hold {
            return Action::None;
        }
        if self.selected_entry().is_some() {
            self.commit()
        } else {
            self.cancel()
        }
    }

    pub fn mark_closed(&mut self, hwnd: isize, now_ms: u64) -> Option<Action> {
        if !self.opened || self.closed.contains(&hwnd) {
            return None;
        }
        if !self.field.iter().any(|e| e.hwnd == hwnd) {
            return None;
        }
        self.closed.insert(hwnd);
        self.refresh_order(now_ms);
        if self.order.is_empty() && self.filter_text.is_empty() {
            return Some(self.cancel());
        }
        None
    }

    pub fn wheel(&mut self, angle_y: f64, pixel_y: f64, angle_x: f64, pixel_x: f64, now_ms: u64) {
        if !self.opened {
            return;
        }
        let (v_steps, v_rest) = field::wheel_steps(self.wheel_carry_y, angle_y, pixel_y);
        self.wheel_carry_y = v_rest;
        if v_steps != 0 {
            self.move_by(v_steps, now_ms);
        }
        let (h_steps, h_rest) = field::wheel_steps(self.wheel_carry_x, angle_x, pixel_x);
        self.wheel_carry_x = h_rest;
        let dir = if h_steps > 0 { 1 } else { -1 };
        for _ in 0..h_steps.abs() {
            self.workspace_step(dir, now_ms);
        }
        self.note_input(now_ms);
    }

    pub fn pointer_moved(&mut self, x: f64, y: f64, now_ms: u64) -> bool {
        if !self.pointer_primed {
            self.pointer_primed = true;
            self.pointer_last = (x, y);
            return false;
        }
        if (x - self.pointer_last.0).abs() < 3.0 && (y - self.pointer_last.1).abs() < 3.0 {
            return false;
        }
        self.pointer_last = (x, y);
        self.note_input(now_ms);
        true
    }

    pub fn commit(&mut self) -> Action {
        if !self.opened {
            return Action::None;
        }
        let target = self.selected_entry().cloned();
        let closed = target
            .as_ref()
            .map(|e| self.closed.contains(&e.hwnd))
            .unwrap_or(true);
        self.dismiss();
        if let Some(entry) = target
            && !closed
        {
            return Action::Commit {
                hwnd: entry.hwnd,
                on_current_desktop: entry.on_current_desktop,
                workspace_key: entry.workspace_key,
                minimized: entry.minimized,
            };
        }
        Action::Cancel
    }

    pub fn cancel(&mut self) -> Action {
        if !self.opened {
            return Action::None;
        }
        self.dismiss();
        Action::Cancel
    }

    pub fn switch_to_workspace(&mut self, group_idx: usize) -> Action {
        if !self.opened {
            return Action::None;
        }
        let Some(ws_key) = self.groups.get(group_idx).map(|g| g.key.clone()) else {
            return Action::Cancel;
        };
        self.dismiss();
        Action::SwitchWorkspace {
            workspace_key: ws_key,
        }
    }

    pub fn clear_drag(&mut self) {
        self.drag_pending = None;
        self.dragging_entry = None;
        self.drop_target_group = None;
        self.drop_new_desktop = false;
    }

    pub fn workspace_descriptors(&self) -> Vec<WorkspaceDescriptor> {
        self.groups
            .iter()
            .filter(|g| !g.special)
            .map(|g| WorkspaceDescriptor {
                key: g.key.clone(),
                id: g.id,
                name: g.name.clone(),
                monitor: g.monitor.clone(),
                on_screen: g.on_screen,
            })
            .collect()
    }

    pub fn next_demo_workspace_descriptor(&self) -> WorkspaceDescriptor {
        let next_id = self
            .groups
            .iter()
            .filter_map(|g| g.id)
            .filter(|&id| id > 0)
            .max()
            .unwrap_or(0)
            + 1;
        let monitor = self
            .monitors
            .values()
            .find(|m| m.focused)
            .or_else(|| self.monitors.values().next())
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "DISPLAY1".to_string());
        WorkspaceDescriptor {
            key: format!("id:{next_id}"),
            id: Some(next_id),
            name: next_id.to_string(),
            monitor,
            on_screen: false,
        }
    }

    pub fn add_workspace_descriptor(&mut self, desc: WorkspaceDescriptor) -> usize {
        let key = desc.key.clone();
        let mut descriptors = self.workspace_descriptors();
        if let Some(existing) = descriptors.iter_mut().find(|d| d.key == key) {
            *existing = desc;
        } else {
            descriptors.push(desc);
        }
        self.groups = field::workspace_groups(&self.field, &descriptors);
        self.groups
            .iter()
            .position(|g| g.key == key)
            .unwrap_or(self.groups.len().saturating_sub(1))
    }

    pub fn move_entry_to_workspace_group(&mut self, entry_idx: usize, group_idx: usize) -> bool {
        let Some(target) = self.groups.get(group_idx).cloned() else {
            return false;
        };
        let Some(entry) = self.field.get_mut(entry_idx) else {
            return false;
        };
        if entry.workspace_key == target.key {
            return false;
        }
        entry.workspace_key = target.key;
        entry.workspace_id = target.id;
        entry.workspace_name = target.name;
        entry.on_current_desktop = target.on_screen;
        if !target.monitor.is_empty() {
            entry.monitor_name = target.monitor;
        }
        let descriptors = self.workspace_descriptors();
        self.groups = field::workspace_groups(&self.field, &descriptors);
        true
    }

    pub fn dismiss(&mut self) {
        self.opened = false;
        self.revealed = false;
        self.camera_animated = false;
        self.field.clear();
        self.order.clear();
        self.slots.clear();
        self.groups.clear();
        self.closed.clear();
        self.filter_text.clear();
        self.selected_index = None;
        self.hovered_plane = None;
        self.hovered_tile = None;
        self.hovered_new_desktop = false;
        self.clear_drag();
        self.dragging_gauge = false;
    }

    pub fn shown_workspace_count(&self) -> usize {
        self.groups
            .iter()
            .filter(|g| {
                g.entries
                    .iter()
                    .any(|&idx| self.slots.get(idx).copied().unwrap_or(-1) >= 0)
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette;

    #[test]
    fn hold_mode_reveals_after_90ms_and_pins_on_space() {
        let cfg = Config::default();
        let pal = palette::derive(palette::windows_plain_theme(false, None));
        let mut ctrl = Controller::new(cfg, pal);

        let cands = vec![
            WindowCandidate {
                hwnd: 1,
                pid: 1,
                title: "A".into(),
                app_id: "a.exe".into(),
                app_name: "A".into(),
                exe_path: String::new(),
                workspace_key: "id:1".into(),
                workspace_id: Some(1),
                workspace_name: "1".into(),
                monitor_name: "DISPLAY1".into(),
                geometry: None,
                minimized: false,
                on_current_desktop: true,
                urgent: false,
                z_rank: 0,
            },
            WindowCandidate {
                hwnd: 2,
                pid: 2,
                title: "B".into(),
                app_id: "b.exe".into(),
                app_name: "B".into(),
                exe_path: String::new(),
                workspace_key: "id:1".into(),
                workspace_id: Some(1),
                workspace_name: "1".into(),
                monitor_name: "DISPLAY1".into(),
                geometry: None,
                minimized: false,
                on_current_desktop: true,
                urgent: false,
                z_rank: 1,
            },
        ];

        assert!(ctrl.open_field(
            SwitchMode::Hold,
            1,
            &cands,
            &[],
            vec![],
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0
            },
            Some(1),
            1000,
        ));
        assert!(!ctrl.revealed);
        ctrl.check_timers(1095);
        assert!(ctrl.revealed);
        assert_eq!(ctrl.selected_entry().unwrap().hwnd, 2);

        ctrl.pin();
        assert_eq!(ctrl.mode, SwitchMode::Browse);
        assert!(matches!(ctrl.alt_released(), Action::None));
    }

    #[test]
    fn creates_workspace_and_moves_window_across_desktops() {
        let cfg = Config::default();
        let pal = palette::derive(palette::windows_plain_theme(false, None));
        let mut ctrl = Controller::new(cfg, pal);

        let cands = vec![
            WindowCandidate {
                hwnd: 101,
                pid: 1,
                title: "Editor".into(),
                app_id: "code.exe".into(),
                app_name: "Code".into(),
                exe_path: String::new(),
                workspace_key: "id:1".into(),
                workspace_id: Some(1),
                workspace_name: "1".into(),
                monitor_name: "DISPLAY1".into(),
                geometry: None,
                minimized: false,
                on_current_desktop: true,
                urgent: false,
                z_rank: 0,
            },
            WindowCandidate {
                hwnd: 102,
                pid: 2,
                title: "Browser".into(),
                app_id: "chrome.exe".into(),
                app_name: "Chrome".into(),
                exe_path: String::new(),
                workspace_key: "id:1".into(),
                workspace_id: Some(1),
                workspace_name: "1".into(),
                monitor_name: "DISPLAY1".into(),
                geometry: None,
                minimized: false,
                on_current_desktop: true,
                urgent: false,
                z_rank: 1,
            },
        ];
        let ws = vec![WorkspaceDescriptor {
            key: "id:1".into(),
            id: Some(1),
            name: "1".into(),
            monitor: "DISPLAY1".into(),
            on_screen: true,
        }];
        let mon = vec![MonitorDesc {
            name: "DISPLAY1".into(),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            focused: true,
        }];
        assert!(ctrl.open_field(
            SwitchMode::Browse,
            0,
            &cands,
            &ws,
            mon,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0
            },
            Some(101),
            1000,
        ));
        assert_eq!(ctrl.groups.len(), 1);

        let desc2 = ctrl.next_demo_workspace_descriptor();
        assert_eq!(desc2.name, "2");
        let g2 = ctrl.add_workspace_descriptor(desc2);
        assert_eq!(ctrl.groups.len(), 2);
        assert!(ctrl.groups[g2].entries.is_empty());

        assert!(ctrl.move_entry_to_workspace_group(0, g2));
        assert_eq!(ctrl.field[0].workspace_name, "2");
        assert!(!ctrl.field[0].on_current_desktop);
        assert_eq!(ctrl.groups[1].entries.len(), 1);
        assert_eq!(ctrl.groups[0].entries.len(), 1);
    }
}
