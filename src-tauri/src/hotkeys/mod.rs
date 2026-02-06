//! Global hotkey management for AirType
//!
//! This module provides hotkey handling with support for both hold and toggle modes.
//! Supports both standard key combinations and modifier-only hotkeys.

mod keyboard;
mod manager;

pub use keyboard::{is_modifier_only_hotkey, KeyboardListener, ModifierKey};
pub use manager::{
    build_global_shortcut_plugin, default_hotkey_configs, parse_shortcut, shortcut_to_string,
    HotkeyAction, HotkeyConfig, HotkeyError, HotkeyEvent, HotkeyManager, HotkeyMode,
};
