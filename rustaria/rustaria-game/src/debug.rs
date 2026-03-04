// ─────────────────────────────────────────────
// debug.rs — Debugger visuel de Rustaria
//
// Appuie sur G pour alterner entre :
//   • rendu normal (remplissage des polygones)
//   • wireframe    (grille des triangles / arêtes)
//
// Extensible : ajouter d'autres flags ici (show_normals, show_chunks, etc.)
// ─────────────────────────────────────────────

/// État du debugger visuel.
/// Un seul booléen pour l'instant : le mode wireframe.
pub struct DebugOverlay {
    pub wireframe: bool,
}

impl DebugOverlay {
    pub fn new() -> Self {
        Self { wireframe: false }
    }

    /// Alterne entre rendu normal et wireframe.
    /// Loggue l'état dans la console (visible avec RUST_LOG=info).
    pub fn toggle_wireframe(&mut self) {
        self.wireframe = !self.wireframe;
        if self.wireframe {
            log::info!("[DEBUG] Wireframe ON  — appuie sur G pour désactiver");
        } else {
            log::info!("[DEBUG] Wireframe OFF — appuie sur G pour activer");
        }
    }
}
