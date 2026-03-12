// ─────────────────────────────────────────────
// debug.rs — Debugger visuel de Rustaria
//
// Appuie sur G pour cycler entre :
//   0 — rendu normal
//   1 — bordures de chunks/blocs uniquement (terrain solide)
//   2 — bordures de chunks + wireframe des triangles
// ─────────────────────────────────────────────

/// État du debugger visuel.
/// mode: 0 = off, 1 = chunk borders, 2 = chunk borders + wireframe.
pub struct DebugOverlay {
    pub mode: u8,
}

impl DebugOverlay {
    pub fn new() -> Self {
        Self { mode: 0 }
    }

    pub fn show_chunk_borders(&self) -> bool {
        self.mode >= 1
    }

    pub fn show_wireframe(&self) -> bool {
        self.mode >= 2
    }

    /// Cycle G → 0 → 1 → 2 → 0 → …
    pub fn toggle_wireframe(&mut self) {
        self.mode = (self.mode + 1) % 3;
        match self.mode {
            0 => log::info!("[DEBUG] Mode OFF — rendu normal"),
            1 => log::info!("[DEBUG] Mode 1 — bordures de chunks"),
            2 => log::info!("[DEBUG] Mode 2 — bordures + wireframe"),
            _ => {}
        }
    }
}
