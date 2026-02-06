/**
 * AirType App State Store
 * Centralized state management using Svelte 5 runes
 */

// App state using module-level $state
export const appState = $state({
  // Recording state
  isRecording: false,
  recordingDuration: 0,
  
  // Transcription
  lastTranscription: '',
  isTranscribing: false,
  
  // Settings
  settings: {
    language: 'en',
    mode: 'batch', // 'batch' or 'live'
    model: 'base',
    hotkey: 'Ctrl+Shift+Space',
    hotkeyMode: 'hold', // 'hold' or 'toggle'
    startOnLogin: false,
  },
  
  // UI state
  settingsOpen: false,
  /** @type {string | null} */
  error: /** @type {string | null} */ (null),
});

// Derived values
export function getLanguageLabel() {
  return appState.settings.language === 'en' ? 'EN' : 'HE';
}

export function getModeLabel() {
  return appState.settings.mode === 'batch' ? 'Batch' : 'Live';
}

// Actions
export function toggleSettings() {
  appState.settingsOpen = !appState.settingsOpen;
}

/** @param {boolean} value */
export function setRecording(value) {
  appState.isRecording = value;
  if (!value) {
    appState.recordingDuration = 0;
  }
}

/** @param {string} text */
export function setTranscription(text) {
  appState.lastTranscription = text;
  appState.isTranscribing = false;
}

/** @param {boolean} value */
export function setTranscribing(value) {
  appState.isTranscribing = value;
}

/** @param {Record<string, unknown>} newSettings */
export function updateSettings(newSettings) {
  appState.settings = { ...appState.settings, ...newSettings };
}

/** @param {string | null} error */
export function setError(error) {
  appState.error = error;
}

export function clearError() {
  appState.error = null;
}
