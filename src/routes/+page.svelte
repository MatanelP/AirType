<script>
  /**
   * AirType - Main Page
   * Voice-to-text desktop app with modern minimal UI
   */
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import '../app.css';
  
  import StatusIndicator from '$lib/components/StatusIndicator.svelte';
  import Settings from '$lib/components/SettingsPanel.svelte';
  
  // App State
  let isRecording = $state(false);
  let isTranscribing = $state(false);
  let recordingDuration = $state(0);
  let lastTranscription = $state('');
  let settingsOpen = $state(false);
  /** @type {string | null} */
  let error = $state(null);
  let recordingLanguage = $state('en');
  let indicatorState = $state('idle'); // idle, recording, transcribing, done
  
  // Settings
  let settings = $state({
    language: 'en',
    mode: 'batch',
    model_size: 'base',
    hotkey_english: 'Ctrl+Shift+E',
    hotkey_hebrew: 'Ctrl+Shift+H',
    hotkey_mode: 'hold',
    live_transcription: false,
    start_minimized: false,
    start_on_login: false,
  });
  
  // Duration timer
  /** @type {ReturnType<typeof setInterval> | null} */
  let durationInterval = $state(null);
  
  // Derived values
  let languageLabel = $derived(settings.language === 'en' ? 'EN' : 'HE');
  let modeLabel = $derived(settings.live_transcription ? 'Live' : 'Batch');
  
  // Load settings on mount
  onMount(() => {
    /** @type {Array<() => void>} */
    let unlisteners = [];
    
    (async () => {
      // Load settings from backend
      try {
        const loadedSettings = await invoke('get_settings');
        if (loadedSettings) {
          settings = { ...settings, ...(/** @type {object} */ (loadedSettings)) };
        }
      } catch (err) {
        console.log('Using default settings');
      }
      
      // Set up event listeners
      unlisteners.push(await listen('recording-started', () => {
        isRecording = true;
        startDurationTimer();
      }));
      
      unlisteners.push(await listen('recording-stopped', () => {
        isRecording = false;
        isTranscribing = true;
        stopDurationTimer();
      }));
      
      unlisteners.push(await listen('transcription-partial', (event) => {
        lastTranscription = /** @type {string} */ (event.payload);
      }));
      
      unlisteners.push(await listen('transcription-complete', (event) => {
        lastTranscription = /** @type {string} */ (event.payload);
        isTranscribing = false;
      }));
      
      unlisteners.push(await listen('error', (event) => {
        error = /** @type {string} */ (event.payload);
        isRecording = false;
        isTranscribing = false;
        stopDurationTimer();
      }));
      
      // Listen for indicator events from backend
      unlisteners.push(await listen('indicator-show', (event) => {
        const data = /** @type {{language: string}} */ (event.payload);
        isRecording = true;
        recordingLanguage = data.language || 'en';
        indicatorState = 'recording';
        startDurationTimer();
      }));
      
      unlisteners.push(await listen('indicator-transcribing', () => {
        indicatorState = 'transcribing';
        isTranscribing = true;
      }));
      
      unlisteners.push(await listen('indicator-done', () => {
        indicatorState = 'done';
      }));
      
      unlisteners.push(await listen('indicator-hide', () => {
        indicatorState = 'idle';
        isRecording = false;
        isTranscribing = false;
        stopDurationTimer();
      }));
    })();
    
    // Cleanup on unmount
    return () => {
      unlisteners.forEach(fn => fn());
      stopDurationTimer();
    };
  });
  
  function startDurationTimer() {
    recordingDuration = 0;
    durationInterval = setInterval(() => {
      recordingDuration++;
    }, 1000);
  }
  
  function stopDurationTimer() {
    if (durationInterval) {
      clearInterval(durationInterval);
      durationInterval = null;
    }
  }
  
  async function toggleRecording() {
    try {
      if (isRecording) {
        await invoke('stop_recording');
      } else {
        await invoke('start_recording');
      }
    } catch (err) {
      error = String(err);
    }
  }
  
  /** @param {Record<string, unknown>} newSettings */
  function handleSettingsSave(newSettings) {
    settings = { ...settings, ...newSettings };
  }
  
  function dismissError() {
    error = null;
  }
</script>

<main class="app" data-tauri-drag-region>
  <!-- Title Bar -->
  <header class="title-bar" data-tauri-drag-region>
    <div class="app-name">AirType</div>
    <button class="settings-btn" onclick={() => settingsOpen = true} aria-label="Open settings">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="3"></circle>
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
      </svg>
    </button>
  </header>
  
  <!-- Main Content -->
  <section class="content">
    <!-- Status Indicator -->
    <StatusIndicator 
      {isRecording} 
      {isTranscribing} 
      duration={recordingDuration} 
    />
    
    <!-- Mode Badges -->
    <div class="badges">
      <span class="badge">{languageLabel}</span>
      <span class="badge">{modeLabel}</span>
    </div>
    
    <!-- Record Button (for manual control) -->
    <button 
      class="record-btn" 
      class:recording={isRecording}
      class:transcribing={isTranscribing}
      onclick={toggleRecording}
      disabled={isTranscribing}
    >
      {#if isTranscribing}
        Processing...
      {:else if isRecording}
        Stop Recording
      {:else}
        Start Recording
      {/if}
    </button>
    
    <!-- Hotkey Hint -->
    <p class="hotkey-hint">
      <kbd>{settings.hotkey_english}</kbd> English · <kbd>{settings.hotkey_hebrew}</kbd> Hebrew
    </p>
  </section>
  
  <!-- Transcription Preview -->
  {#if lastTranscription}
    <section class="transcription-preview animate-slideUp">
      <div class="preview-header">
        <span class="preview-label">Last Transcription</span>
        <button class="copy-btn" onclick={() => navigator.clipboard.writeText(lastTranscription)}>
          Copy
        </button>
      </div>
      <p class="preview-text">{lastTranscription}</p>
    </section>
  {/if}
  
  <!-- Error Toast -->
  {#if error}
    <div class="error-toast animate-slideUp">
      <span>{error}</span>
      <button class="dismiss-btn" onclick={dismissError}>×</button>
    </div>
  {/if}
  
  <!-- Settings Panel -->
  <Settings 
    isOpen={settingsOpen} 
    {settings}
    onClose={() => settingsOpen = false}
    onSettingsChange={handleSettingsSave}
  />
</main>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 0;
  }
  
  /* Title Bar */
  .title-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    background: rgba(10, 10, 15, 0.8);
    border-bottom: 1px solid var(--color-border);
  }
  
  .app-name {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.05em;
  }
  
  .settings-btn {
    background: none;
    border: none;
    padding: 0.5rem;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-md);
    transition: all var(--transition-fast);
  }
  
  .settings-btn:hover {
    background: var(--color-surface-hover);
    color: var(--color-text);
  }
  
  /* Main Content */
  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 2rem;
    gap: 2rem;
  }
  
  /* Mode Badges */
  .badges {
    display: flex;
    gap: 0.5rem;
  }
  
  .badge {
    padding: 0.375rem 0.75rem;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  
  /* Record Button */
  .record-btn {
    padding: 0.875rem 2rem;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    color: var(--color-text);
    font-size: 0.9375rem;
    font-weight: 500;
    cursor: pointer;
    transition: all var(--transition-normal);
  }
  
  .record-btn:hover:not(:disabled) {
    background: var(--color-surface-hover);
    border-color: var(--color-accent);
  }
  
  .record-btn.recording {
    background: var(--color-recording);
    border-color: var(--color-recording);
    color: white;
  }
  
  .record-btn.recording:hover {
    background: #dc2626;
  }
  
  .record-btn.transcribing {
    background: var(--color-transcribing);
    border-color: var(--color-transcribing);
    color: white;
    cursor: not-allowed;
  }
  
  .record-btn:disabled {
    opacity: 0.7;
  }
  
  /* Hotkey Hint */
  .hotkey-hint {
    font-size: 0.8125rem;
    color: var(--color-text-muted);
  }
  
  .hotkey-hint kbd {
    font-family: var(--font-mono);
    font-size: 0.75rem;
    padding: 0.25rem 0.5rem;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-accent);
  }
  
  /* Transcription Preview */
  .transcription-preview {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    padding: 1rem 1.5rem;
    background: var(--color-surface);
    border-top: 1px solid var(--color-border);
  }
  
  .preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.5rem;
  }
  
  .preview-label {
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  
  .copy-btn {
    padding: 0.25rem 0.5rem;
    background: none;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-size: 0.6875rem;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all var(--transition-fast);
  }
  
  .copy-btn:hover {
    background: var(--color-surface-hover);
    color: var(--color-text);
  }
  
  .preview-text {
    font-size: 0.9375rem;
    color: var(--color-text);
    line-height: 1.5;
    max-height: 4.5rem;
    overflow-y: auto;
  }
  
  /* Error Toast */
  .error-toast {
    position: fixed;
    bottom: 1rem;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: var(--radius-md);
    color: #fca5a5;
    font-size: 0.875rem;
    z-index: 50;
  }
  
  .dismiss-btn {
    background: none;
    border: none;
    padding: 0.25rem;
    color: inherit;
    font-size: 1.25rem;
    line-height: 1;
    cursor: pointer;
    opacity: 0.7;
  }
  
  .dismiss-btn:hover {
    opacity: 1;
  }
</style>
