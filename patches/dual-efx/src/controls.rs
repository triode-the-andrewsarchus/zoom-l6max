//! Pure control-surface gesture logic. Firmware event delivery and timing are
//! revision-specific and must be connected separately.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EfxSelectAction {
    None,
    SelectNext,
    ToggleInspector,
}

/// Recognizes the observed L6max panel EFX SEL frames in a byte stream.
/// The caller must reset this on transport loss or protocol reinitialization.
#[derive(Default)]
pub struct PanelEfxDecoder {
    phase: u8,
    pressed: bool,
    column: u8,
}

impl PanelEfxDecoder {
    pub fn feed(&mut self, byte: u8) -> Option<bool> {
        if byte & 0x80 != 0 {
            self.phase = if byte == 0x90 || byte == 0x91 { 1 } else { 0 };
            self.pressed = byte == 0x91;
            return None;
        }
        match self.phase {
            1 => { self.column = byte; self.phase = 2; None }
            2 => { self.phase = 0; (self.column == 4 && byte == 6).then_some(self.pressed) }
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct EfxSelectGesture {
    started_at: Option<u32>,
    last_sequence: Option<u32>,
}

impl EfxSelectGesture {
    /// Defers the stock selection until release, so a hold can open the
    /// inspector without also advancing the effect. The caller supplies a
    /// threshold in the same wrapping tick units as `now`.
    pub fn edge(&mut self, pressed: bool, now: u32, hold_ticks: u32) -> EfxSelectAction {
        if self.last_sequence.take().is_some() { self.started_at = None; }
        self.classify_edge(pressed, now, hold_ticks)
    }

    /// Classifies an edge carrying a nonzero sequence assigned at reception.
    /// A gap discards an unfinished gesture before considering the new edge.
    /// Sequence zero means the event could not be associated with reception.
    pub fn sequenced_edge(&mut self, pressed: bool, now: u32, sequence: u32, hold_ticks: u32) -> EfxSelectAction {
        if sequence == 0 {
            self.cancel();
            return EfxSelectAction::None;
        }
        if let Some(previous) = self.last_sequence {
            let next = previous.wrapping_add(1);
            let expected = if next == 0 { 1 } else { next };
            if sequence != expected { self.started_at = None; }
        } else {
            self.started_at = None;
        }
        self.last_sequence = Some(sequence);
        self.classify_edge(pressed, now, hold_ticks)
    }

    fn classify_edge(&mut self, pressed: bool, now: u32, hold_ticks: u32) -> EfxSelectAction {
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
        self.last_sequence = None;
    }
}
