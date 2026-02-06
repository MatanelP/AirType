//! Low-level keyboard listener for modifier-only hotkeys
//!
//! Uses rdev for system-wide keyboard event capture, enabling hotkeys
//! that are just modifier keys (e.g., Left Alt, Right Alt).

use parking_lot::RwLock;
use rdev::{listen, Event, EventType, Key};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

/// Represents a modifier key that can be used as a standalone hotkey
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifierKey {
    AltLeft,
    AltRight,
    ControlLeft,
    ControlRight,
    ShiftLeft,
    ShiftRight,
    MetaLeft,
    MetaRight,
}

impl ModifierKey {
    /// Parse a string into a ModifierKey
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "altleft" | "leftalt" | "lalt" => Some(ModifierKey::AltLeft),
            "altright" | "rightalt" | "ralt" | "altgr" => Some(ModifierKey::AltRight),
            "controlleft" | "leftctrl" | "lctrl" | "ctrlleft" => Some(ModifierKey::ControlLeft),
            "controlright" | "rightctrl" | "rctrl" | "ctrlright" => Some(ModifierKey::ControlRight),
            "shiftleft" | "leftshift" | "lshift" => Some(ModifierKey::ShiftLeft),
            "shiftright" | "rightshift" | "rshift" => Some(ModifierKey::ShiftRight),
            "metaleft" | "leftmeta" | "lmeta" | "superleft" | "leftsuper" => Some(ModifierKey::MetaLeft),
            "metaright" | "rightmeta" | "rmeta" | "superright" | "rightsuper" => Some(ModifierKey::MetaRight),
            _ => None,
        }
    }

    /// Convert to display string
    pub fn to_string(&self) -> &'static str {
        match self {
            ModifierKey::AltLeft => "AltLeft",
            ModifierKey::AltRight => "AltRight",
            ModifierKey::ControlLeft => "CtrlLeft",
            ModifierKey::ControlRight => "CtrlRight",
            ModifierKey::ShiftLeft => "ShiftLeft",
            ModifierKey::ShiftRight => "ShiftRight",
            ModifierKey::MetaLeft => "SuperLeft",
            ModifierKey::MetaRight => "SuperRight",
        }
    }

    /// Convert from rdev Key
    fn from_rdev_key(key: &Key) -> Option<Self> {
        match key {
            // On Linux, rdev reports both left and right Alt as Key::Alt
            // AltGr is typically right Alt on some keyboard layouts
            Key::Alt => Some(ModifierKey::AltLeft),  // Maps any Alt to AltLeft
            Key::AltGr => Some(ModifierKey::AltRight), // AltGr is typically right Alt
            Key::ControlLeft => Some(ModifierKey::ControlLeft),
            Key::ControlRight => Some(ModifierKey::ControlRight),
            Key::ShiftLeft => Some(ModifierKey::ShiftLeft),
            Key::ShiftRight => Some(ModifierKey::ShiftRight),
            Key::MetaLeft => Some(ModifierKey::MetaLeft),
            Key::MetaRight => Some(ModifierKey::MetaRight),
            _ => None,
        }
    }
}

/// Check if a hotkey string is a modifier-only hotkey
pub fn is_modifier_only_hotkey(hotkey: &str) -> bool {
    ModifierKey::from_str(hotkey).is_some()
}

/// State for tracking modifier key presses
struct KeyboardState {
    /// Currently pressed modifier keys
    pressed_modifiers: HashMap<ModifierKey, bool>,
    /// Registered modifier-only hotkeys and their callbacks
    hotkeys: HashMap<ModifierKey, HotkeyCallback>,
    /// Whether each hotkey is currently "active" (key held down)
    active_hotkeys: HashMap<ModifierKey, bool>,
}

type HotkeyCallback = Arc<dyn Fn(ModifierKey, bool) + Send + Sync>;

impl KeyboardState {
    fn new() -> Self {
        Self {
            pressed_modifiers: HashMap::new(),
            hotkeys: HashMap::new(),
            active_hotkeys: HashMap::new(),
        }
    }
}

/// Low-level keyboard listener for modifier-only hotkeys
pub struct KeyboardListener {
    state: Arc<RwLock<KeyboardState>>,
    running: Arc<RwLock<bool>>,
}

impl KeyboardListener {
    /// Create a new keyboard listener
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(KeyboardState::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Register a modifier-only hotkey
    pub fn register_modifier_hotkey<F>(&self, modifier: ModifierKey, callback: F)
    where
        F: Fn(ModifierKey, bool) + Send + Sync + 'static,
    {
        let mut state = self.state.write();
        state.hotkeys.insert(modifier, Arc::new(callback));
        state.active_hotkeys.insert(modifier, false);
    }

    /// Unregister a modifier-only hotkey
    pub fn unregister_modifier_hotkey(&self, modifier: ModifierKey) {
        let mut state = self.state.write();
        state.hotkeys.remove(&modifier);
        state.active_hotkeys.remove(&modifier);
    }

    /// Start listening for keyboard events
    pub fn start(&self) {
        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);

        {
            let mut r = running.write();
            if *r {
                log::info!("Keyboard listener already running");
                return; // Already running
            }
            *r = true;
        }

        log::info!("Starting low-level keyboard listener for modifier-only hotkeys...");
        
        thread::spawn(move || {
            let state_clone = Arc::clone(&state);
            
            log::info!("Keyboard listener thread started");
            
            if let Err(e) = listen(move |event| {
                handle_event(&state_clone, event);
            }) {
                log::error!("Failed to start keyboard listener: {:?}", e);
            }
        });
    }

    /// Check if a modifier hotkey is currently active (pressed)
    pub fn is_hotkey_active(&self, modifier: ModifierKey) -> bool {
        let state = self.state.read();
        state.active_hotkeys.get(&modifier).copied().unwrap_or(false)
    }
}

impl Default for KeyboardListener {
    fn default() -> Self {
        Self::new()
    }
}

fn handle_event(state: &Arc<RwLock<KeyboardState>>, event: Event) {
    match event.event_type {
        EventType::KeyPress(key) => {
            // Log raw key for debugging
            log::debug!("Raw rdev key pressed: {:?}", key);
            
            if let Some(modifier) = ModifierKey::from_rdev_key(&key) {
                log::info!("Modifier key pressed: {:?} (from rdev key: {:?})", modifier, key);
                let mut state = state.write();
                
                // Track that this modifier is pressed
                state.pressed_modifiers.insert(modifier, true);
                
                // Check if this is a registered hotkey and not already active
                if let Some(callback) = state.hotkeys.get(&modifier).cloned() {
                    if !state.active_hotkeys.get(&modifier).copied().unwrap_or(false) {
                        log::info!("Modifier hotkey triggered (pressed): {:?}", modifier);
                        state.active_hotkeys.insert(modifier, true);
                        drop(state); // Release lock before callback
                        callback(modifier, true); // true = pressed
                    }
                } else {
                    log::debug!("No hotkey registered for modifier {:?}", modifier);
                }
            }
        }
        EventType::KeyRelease(key) => {
            if let Some(modifier) = ModifierKey::from_rdev_key(&key) {
                log::info!("Modifier key released: {:?} (from rdev key: {:?})", modifier, key);
                let mut state = state.write();
                
                // Track that this modifier is released
                state.pressed_modifiers.insert(modifier, false);
                
                // Check if this is a registered hotkey and currently active
                if let Some(callback) = state.hotkeys.get(&modifier).cloned() {
                    if state.active_hotkeys.get(&modifier).copied().unwrap_or(false) {
                        log::info!("Modifier hotkey triggered (released): {:?}", modifier);
                        state.active_hotkeys.insert(modifier, false);
                        drop(state); // Release lock before callback
                        callback(modifier, false); // false = released
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modifier_key_from_str() {
        assert_eq!(ModifierKey::from_str("AltLeft"), Some(ModifierKey::AltLeft));
        assert_eq!(ModifierKey::from_str("altright"), Some(ModifierKey::AltRight));
        assert_eq!(ModifierKey::from_str("LALT"), Some(ModifierKey::AltLeft));
        assert_eq!(ModifierKey::from_str("CtrlLeft"), Some(ModifierKey::ControlLeft));
        assert_eq!(ModifierKey::from_str("invalid"), None);
    }

    #[test]
    fn test_is_modifier_only_hotkey() {
        assert!(is_modifier_only_hotkey("AltLeft"));
        assert!(is_modifier_only_hotkey("AltRight"));
        assert!(is_modifier_only_hotkey("CtrlLeft"));
        assert!(!is_modifier_only_hotkey("Ctrl+A"));
        assert!(!is_modifier_only_hotkey("Space"));
    }
}
