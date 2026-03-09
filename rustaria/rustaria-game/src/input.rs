use winit::{
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
};

use crate::debug::DebugOverlay;

// ─────────────────────────────────────────────
// InputState : état courant des entrées utilisateur
// Mis à jour dans handle_keyboard / handle_device_event
// Consommé par Camera chaque frame dans main.rs
// ─────────────────────────────────────────────
#[derive(Default)]
pub struct InputState {
    // Touches de déplacement maintenues
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,

    // Delta souris accumulé depuis le dernier frame (en pixels)
    // Remis à zéro après chaque frame dans main.rs
    pub mouse_dx: f32,
    pub mouse_dy: f32,

    // La souris est capturée (clic gauche dans la fenêtre)
    pub mouse_captured: bool,
}

// ─────────────────────────────────────────────
// handle_keyboard : traite les événements clavier reçus dans window_event
//
// Retourne true si l'événement a été consommé (fermeture, debug toggle, etc.)
// Les touches de déplacement sont juste stockées dans InputState.
// ─────────────────────────────────────────────
pub fn handle_keyboard(
    event: &WindowEvent,
    event_loop: &ActiveEventLoop,
    debug: &mut DebugOverlay,
    input: &mut InputState,
) -> bool {
    match event {
        // ── Clic gauche = capture la souris ─────────────────────────────
        WindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Left,
            ..
        } => {
            input.mouse_captured = true;
            false // ne consomme pas l'événement
        }

        // ── Échap = libère la souris OU ferme si déjà libre ─────────────
        WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Escape),
                    state: ElementState::Pressed,
                    ..
                },
            ..
        } => {
            if input.mouse_captured {
                input.mouse_captured = false;
            } else {
                event_loop.exit();
            }
            true
        }

        // ── G = toggle wireframe (debug grille) ─────────────────────────
        WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::KeyG),
                    state: ElementState::Pressed,
                    ..
                },
            ..
        } => {
            debug.toggle_wireframe();
            true
        }

        // ── Touches de déplacement (appui + relâche) ─────────────────────
        WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(key),
                    state,
                    ..
                },
            ..
        } => {
            let pressed = *state == ElementState::Pressed;
            match key {
                KeyCode::KeyW | KeyCode::ArrowUp => input.forward = pressed,
                KeyCode::KeyS | KeyCode::ArrowDown => input.backward = pressed,
                KeyCode::KeyA | KeyCode::ArrowLeft => input.left = pressed,
                KeyCode::KeyD | KeyCode::ArrowRight => input.right = pressed,
                KeyCode::Space => input.up = pressed,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => input.down = pressed,
                _ => return false,
            }
            false // ne consomme pas : main.rs peut encore voir l'événement si besoin
        }

        _ => false,
    }
}

// ─────────────────────────────────────────────
// handle_device_event : capte le delta brut de la souris (DeviceEvent)
// À appeler depuis ApplicationHandler::device_event dans main.rs
// N'est accumulé que si la souris est capturée
// ─────────────────────────────────────────────
pub fn handle_device_event(event: &DeviceEvent, input: &mut InputState) {
    if !input.mouse_captured {
        return;
    }
    if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
        input.mouse_dx += *dx as f32;
        input.mouse_dy += *dy as f32;
    }
}
