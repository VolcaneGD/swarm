use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

use tauri::{Monitor, PhysicalPosition, PhysicalSize, Position, Size, Window};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Geometry {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) maximized: bool,
}

impl Geometry {
    pub(crate) const fn default_window() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 880,
            height: 760,
            maximized: false,
        }
    }

    pub(crate) fn from_saved(value: swarm_core::WindowGeometry) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            maximized: value.maximized,
        }
    }

    pub(crate) fn to_saved(self) -> swarm_core::WindowGeometry {
        swarm_core::WindowGeometry {
            version: 1,
            x: self.x.clamp(-32_768, 32_767),
            y: self.y.clamp(-32_768, 32_767),
            width: self.width.clamp(320, 16_384),
            height: self.height.clamp(240, 16_384),
            maximized: self.maximized,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkArea {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) primary: bool,
}

impl From<&Monitor> for WorkArea {
    fn from(monitor: &Monitor) -> Self {
        let area = monitor.work_area();
        Self {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
            primary: false,
        }
    }
}

fn intersection_area(geometry: Geometry, area: WorkArea) -> u64 {
    let left = i64::from(geometry.x).max(i64::from(area.x));
    let top = i64::from(geometry.y).max(i64::from(area.y));
    let right = (i64::from(geometry.x) + i64::from(geometry.width))
        .min(i64::from(area.x) + i64::from(area.width));
    let bottom = (i64::from(geometry.y) + i64::from(geometry.height))
        .min(i64::from(area.y) + i64::from(area.height));
    if right <= left || bottom <= top {
        return 0;
    }
    ((right - left) as u64).saturating_mul((bottom - top) as u64)
}

fn clamp_to_area(mut geometry: Geometry, area: WorkArea) -> Geometry {
    let width = geometry.width.max(1).min(area.width.max(1));
    let height = geometry.height.max(1).min(area.height.max(1));
    let min_x = i64::from(area.x);
    let min_y = i64::from(area.y);
    let max_x = min_x + i64::from(area.width.saturating_sub(width));
    let max_y = min_y + i64::from(area.height.saturating_sub(height));
    geometry.x = i64::from(geometry.x)
        .clamp(min_x, max_x)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    geometry.y = i64::from(geometry.y)
        .clamp(min_y, max_y)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    geometry.width = width;
    geometry.height = height;
    geometry
}

pub(crate) fn clamp_geometry(geometry: Geometry, areas: &[WorkArea]) -> Option<Geometry> {
    let selected = areas
        .iter()
        .max_by_key(|area| (intersection_area(geometry, **area), area.primary))
        .filter(|area| intersection_area(geometry, **area) > 0)
        .or_else(|| areas.iter().find(|area| area.primary))
        .or_else(|| areas.first())?;
    Some(clamp_to_area(geometry, *selected))
}

fn current_geometry(window: &Window) -> Option<Geometry> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(Geometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: false,
    })
}

fn monitor_areas(window: &Window) -> Option<Vec<WorkArea>> {
    let monitors = window.available_monitors().ok()?;
    if monitors.is_empty() {
        return None;
    }
    let primary = window.primary_monitor().ok().flatten();
    Some(
        monitors
            .iter()
            .map(|monitor| {
                let mut area = WorkArea::from(monitor);
                area.primary = primary.as_ref().is_some_and(|primary| {
                    primary.name() == monitor.name()
                        && primary.position() == monitor.position()
                        && primary.size() == monitor.size()
                });
                area
            })
            .collect(),
    )
}

pub(crate) struct RestoreResult {
    pub(crate) geometry: Geometry,
    pub(crate) applied: bool,
}

pub(crate) fn restore(window: &Window, saved: Option<swarm_core::WindowGeometry>) -> RestoreResult {
    let Some(areas) = monitor_areas(window) else {
        return RestoreResult {
            geometry: current_geometry(window).unwrap_or_else(Geometry::default_window),
            applied: false,
        };
    };
    let geometry = saved.map(Geometry::from_saved).unwrap_or_else(|| {
        let Some(primary) = areas
            .iter()
            .find(|area| area.primary)
            .or_else(|| areas.first())
        else {
            return Geometry::default_window();
        };
        let mut geometry = Geometry::default_window();
        geometry.width = geometry.width.min(primary.width.max(1));
        geometry.height = geometry.height.min(primary.height.max(1));
        geometry.x = (i64::from(primary.x)
            + i64::from(primary.width.saturating_sub(geometry.width)) / 2)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        geometry.y = (i64::from(primary.y)
            + i64::from(primary.height.saturating_sub(geometry.height)) / 2)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        geometry
    });
    let Some(geometry) = clamp_geometry(geometry, &areas) else {
        return RestoreResult {
            geometry: current_geometry(window).unwrap_or_else(Geometry::default_window),
            applied: false,
        };
    };
    if window.is_maximized().unwrap_or(false) && window.unmaximize().is_err() {
        return RestoreResult {
            geometry: current_geometry(window).unwrap_or_else(Geometry::default_window),
            applied: false,
        };
    }
    if window
        .set_position(Position::Physical(PhysicalPosition::new(
            geometry.x, geometry.y,
        )))
        .is_err()
    {
        return RestoreResult {
            geometry: current_geometry(window).unwrap_or_else(Geometry::default_window),
            applied: false,
        };
    }
    let decoration = window
        .outer_size()
        .ok()
        .zip(window.inner_size().ok())
        .map(|(outer, inner)| {
            (
                outer.width.saturating_sub(inner.width),
                outer.height.saturating_sub(inner.height),
            )
        })
        .unwrap_or_default();
    if window
        .set_size(Size::Physical(PhysicalSize::new(
            geometry.width.saturating_sub(decoration.0).max(1),
            geometry.height.saturating_sub(decoration.1).max(1),
        )))
        .is_err()
    {
        return RestoreResult {
            geometry: current_geometry(window).unwrap_or_else(Geometry::default_window),
            applied: false,
        };
    }
    if geometry.maximized && window.maximize().is_err() {
        return RestoreResult {
            geometry: current_geometry(window).unwrap_or_else(Geometry::default_window),
            applied: false,
        };
    }
    RestoreResult {
        geometry,
        applied: true,
    }
}

#[derive(Default)]
pub(crate) struct WindowTracker {
    geometry: Mutex<TrackedGeometry>,
    allow_close: AtomicBool,
    close_in_flight: AtomicBool,
    restore_in_flight: AtomicBool,
    persistence_blocked: AtomicBool,
    next_close_request: AtomicU64,
    pending_close: Mutex<Option<(u64, Sender<bool>)>>,
    last_restore_intent: Mutex<Option<String>>,
}

#[derive(Default)]
struct TrackedGeometry {
    geometry: Option<Geometry>,
    dirty_since: Option<Instant>,
    retry_after: Option<Instant>,
}

impl WindowTracker {
    pub(crate) fn initialize(&self, geometry: Geometry) {
        if let Ok(mut current) = self.geometry.lock() {
            current.geometry = Some(geometry);
            current.dirty_since = None;
            current.retry_after = None;
            self.persistence_blocked.store(false, Ordering::SeqCst);
        }
    }

    fn capture(&self, window: &Window) -> Option<Geometry> {
        if window.is_minimized().unwrap_or(false) {
            return self.snapshot();
        }
        let maximized = window.is_maximized().unwrap_or(false);
        let mut state = self.geometry.lock().ok()?;
        let mut changed = false;
        let current = state.geometry.get_or_insert_with(Geometry::default_window);
        if maximized {
            if !current.maximized {
                current.maximized = true;
                changed = true;
            }
            let captured = *current;
            if changed {
                state.dirty_since = Some(Instant::now());
                state.retry_after = None;
                self.persistence_blocked.store(false, Ordering::SeqCst);
            }
            return Some(captured);
        }
        if let Some(mut observed) = current_geometry(window) {
            observed.maximized = false;
            if *current != observed {
                *current = observed;
                changed = true;
                self.persistence_blocked.store(false, Ordering::SeqCst);
            }
        }
        let captured = *current;
        if changed {
            state.dirty_since = Some(Instant::now());
            state.retry_after = None;
        }
        Some(captured)
    }

    pub(crate) fn observe(&self, window: &Window) {
        let _ = self.capture(window);
    }

    pub(crate) fn snapshot(&self) -> Option<Geometry> {
        self.geometry.lock().ok().and_then(|state| state.geometry)
    }

    pub(crate) fn capture_for_close(&self, window: &Window) -> Option<Geometry> {
        self.capture(window)
    }

    pub(crate) fn take_debounced_save(&self, now: Instant) -> Option<Geometry> {
        if self.close_in_flight.load(Ordering::SeqCst)
            || self.restore_in_flight.load(Ordering::SeqCst)
            || self.persistence_blocked.load(Ordering::SeqCst)
        {
            return None;
        }
        let mut state = self.geometry.lock().ok()?;
        let since = state.dirty_since?;
        if now.duration_since(since) < Duration::from_millis(500)
            || state
                .retry_after
                .is_some_and(|retry_after| now < retry_after)
        {
            return None;
        }
        state.dirty_since = None;
        state.retry_after = None;
        state.geometry
    }

    pub(crate) fn retry_save(&self) {
        if let Ok(mut state) = self.geometry.lock()
            && state.geometry.is_some()
        {
            state.dirty_since = Some(Instant::now());
            state.retry_after = Some(Instant::now() + Duration::from_secs(5));
        }
    }

    pub(crate) fn permit_close_once(&self) {
        self.allow_close.store(true, Ordering::SeqCst);
    }

    pub(crate) fn take_close_permission(&self) -> bool {
        let allowed = self.allow_close.swap(false, Ordering::SeqCst);
        if allowed {
            self.close_in_flight.store(false, Ordering::SeqCst);
        }
        allowed
    }

    pub(crate) fn should_write(&self, geometry: Geometry) -> bool {
        !self.close_in_flight.load(Ordering::SeqCst)
            && !self.restore_in_flight.load(Ordering::SeqCst)
            && !self.persistence_blocked.load(Ordering::SeqCst)
            && self.snapshot() == Some(geometry)
    }

    pub(crate) fn block_persistence(&self) {
        self.persistence_blocked.store(true, Ordering::SeqCst);
    }

    pub(crate) fn persistence_blocked(&self) -> bool {
        self.persistence_blocked.load(Ordering::SeqCst)
    }

    pub(crate) fn restore_in_flight(&self) -> bool {
        self.restore_in_flight.load(Ordering::SeqCst)
    }

    pub(crate) fn begin_restore_probe(&self) -> bool {
        !self.close_in_flight.load(Ordering::SeqCst)
            && self
                .restore_in_flight
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
    }

    pub(crate) fn record_restore_intent(&self, intent: &str) -> bool {
        let Ok(mut last) = self.last_restore_intent.lock() else {
            return false;
        };
        if last.as_deref() == Some(intent) {
            false
        } else {
            *last = Some(intent.to_owned());
            true
        }
    }

    pub(crate) fn finish_restore(&self) {
        self.restore_in_flight.store(false, Ordering::SeqCst);
    }

    pub(crate) fn begin_close_save(&self) -> bool {
        !self.restore_in_flight.load(Ordering::SeqCst)
            && self
                .close_in_flight
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
    }

    pub(crate) fn finish_close_save(&self) {
        self.allow_close.store(false, Ordering::SeqCst);
        self.close_in_flight.store(false, Ordering::SeqCst);
        self.cancel_close_request(0);
    }

    pub(crate) fn start_close_handshake(&self) -> Option<(u64, Receiver<bool>)> {
        let request_id = self.next_close_request.fetch_add(1, Ordering::SeqCst) + 1;
        let (sender, receiver) = mpsc::channel();
        let mut pending = self.pending_close.lock().ok()?;
        *pending = Some((request_id, sender));
        Some((request_id, receiver))
    }

    pub(crate) fn complete_close_handshake(&self, request_id: u64, success: bool) -> bool {
        let Ok(mut pending) = self.pending_close.lock() else {
            return false;
        };
        if pending.as_ref().is_some_and(|(id, _)| *id == request_id)
            && let Some((_, sender)) = pending.take()
        {
            return sender.send(success).is_ok();
        }
        false
    }

    pub(crate) fn cancel_close_request(&self, request_id: u64) {
        if let Ok(mut pending) = self.pending_close.lock()
            && pending
                .as_ref()
                .is_some_and(|(id, _)| request_id == 0 || *id == request_id)
        {
            *pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Geometry, WorkArea, clamp_geometry};

    #[test]
    fn negative_monitor_coordinates_are_valid_and_preserved() {
        let geometry = Geometry {
            x: -1700,
            y: -120,
            width: 1200,
            height: 800,
            maximized: false,
        };
        let areas = [WorkArea {
            x: -1920,
            y: -200,
            width: 1920,
            height: 1080,
            primary: false,
        }];
        assert_eq!(clamp_geometry(geometry, &areas), Some(geometry));
    }

    #[test]
    fn removed_monitor_relocates_window_to_primary_work_area() {
        let geometry = Geometry {
            x: -2200,
            y: 100,
            width: 1100,
            height: 700,
            maximized: true,
        };
        let areas = [WorkArea {
            x: 0,
            y: 0,
            width: 1600,
            height: 900,
            primary: true,
        }];
        assert_eq!(
            clamp_geometry(geometry, &areas),
            Some(Geometry {
                x: 0,
                y: 100,
                ..geometry
            })
        );
    }

    #[test]
    fn oversized_bounds_are_reduced_to_available_work_area() {
        let geometry = Geometry {
            x: 50,
            y: 40,
            width: 2200,
            height: 1400,
            maximized: false,
        };
        let areas = [WorkArea {
            x: -100,
            y: -30,
            width: 1200,
            height: 700,
            primary: true,
        }];
        assert_eq!(
            clamp_geometry(geometry, &areas),
            Some(Geometry {
                x: -100,
                y: -30,
                width: 1200,
                height: 700,
                maximized: false,
            })
        );
    }

    #[test]
    fn no_monitor_leaves_startup_default_unmodified() {
        assert_eq!(clamp_geometry(Geometry::default_window(), &[]), None);
    }

    #[test]
    fn close_handshake_accepts_only_the_pending_request_id() {
        let tracker = super::WindowTracker::default();
        assert!(tracker.begin_close_save());
        let (request_id, receiver) = tracker.start_close_handshake().unwrap();
        assert!(!tracker.complete_close_handshake(request_id + 1, true));
        assert!(tracker.complete_close_handshake(request_id, true));
        assert!(receiver.recv().unwrap());
        tracker.finish_close_save();
        assert!(!tracker.complete_close_handshake(request_id, true));
    }

    #[test]
    fn close_permission_is_one_shot_and_failure_revokes_it() {
        let tracker = super::WindowTracker::default();
        tracker.permit_close_once();
        assert!(tracker.take_close_permission());
        assert!(!tracker.take_close_permission());
        tracker.permit_close_once();
        tracker.finish_close_save();
        assert!(!tracker.take_close_permission());
    }

    #[test]
    fn restore_probe_suppresses_autosave_until_native_bounds_are_reset() {
        let tracker = super::WindowTracker::default();
        assert!(tracker.begin_restore_probe());
        assert!(!tracker.should_write(Geometry::default_window()));
        tracker.initialize(Geometry::default_window());
        tracker.finish_restore();
        assert!(tracker.should_write(Geometry::default_window()));
        tracker.block_persistence();
        assert!(!tracker.should_write(Geometry::default_window()));
        tracker.initialize(Geometry::default_window());
        assert!(tracker.should_write(Geometry::default_window()));
    }

    #[test]
    fn stored_geometry_is_clamped_to_core_validation_limits() {
        let stored = Geometry {
            x: 60_000,
            y: -60_000,
            width: 20_000,
            height: 10,
            maximized: false,
        }
        .to_saved();
        assert_eq!(stored.x, 32_767);
        assert_eq!(stored.y, -32_768);
        assert_eq!(stored.width, 16_384);
        assert_eq!(stored.height, 240);
    }
}
