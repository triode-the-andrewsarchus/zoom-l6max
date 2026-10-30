//! Pure control-surface gesture logic. Firmware event delivery and timing are
//! revision-specific and must be connected separately.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EfxSelectAction {
    None,
    SelectNext,
    ToggleInspector,
}

#[derive(Default)]
pub struct EfxSelectGesture {
    started_at: Option<u32>,
}

impl EfxSelectGesture {
    /// Defers the stock selection until release, so a hold can open the
    /// inspector without also advancing the effect. The caller supplies a
    /// threshold in the same wrapping tick units as `now`.
    pub fn edge(&mut self, pressed: bool, now: u32, hold_ticks: u32) -> EfxSelectAction {
        if pressed {
            if self.started_at.is_none() {
                self.started_at = Some(now);
            }
            return EfxSelectAction::None;
        }
        match self.started_at.take() {
            None => EfxSelectAction::None,
            Some(start) if now.wrapping_sub(start) >= hold_ticks => EfxSelectAction::ToggleInspector,
            Some(_) => EfxSelectAction::SelectNext,
        }
    }

    /// Discards a pending press when its window or control context is lost.
    pub fn cancel(&mut self) {
        self.started_at = None;
    }
}
