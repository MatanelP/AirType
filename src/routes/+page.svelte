<script>
  /**
   * AirType - Main Page
   * Voice-to-text desktop app with modern minimal UI
   */
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { dev } from '$app/environment';
  import '../app.css';
  
  import StatusIndicator from '$lib/components/StatusIndicator.svelte';
  import Settings from '$lib/components/SettingsPanel.svelte';
  
  // App State
  let isRecording = $state(false);
  let isTranscribing = $state(false);
  let recordingDuration = $state(0);
  let lastTranscription = $state('');
  /** @type {Array<{id: number, text: string, language: string, timestamp: number}>} */
  let history = $state([]);
  let historyOpen = $state(false);
  let testResult = $state({ language: '', text: '' });
  let testingLanguage = $state('');
  /** @type {string | null} */
  let testError = $state(null);
  let settingsOpen = $state(false);
  /** @type {string | null} */
  let error = $state(null);
  let recordingLanguage = $state('en');
  let indicatorState = $state('idle'); // idle, recording, transcribing, done
  let needsAccessibility = $state(false);

  // Self-update
  /** @type {{version: string, current_version: string, notes: string | null, date: string | null} | null} */
  let availableUpdate = $state(null);
  let updateInstalling = $state(false);
  let updateProgress = $state(0);
  /** @type {string | null} */
  let updateError = $state(null);
  let updateDismissed = $state(false);

  let settings = $state({
    language: 'en',
    hotkey_english: 'Ctrl+Shift+E',
    hotkey_hebrew: 'Ctrl+Shift+H',
    hotkey_mode: 'hold',
    start_minimized: false,
    start_on_login: false,
  });
  
  // Duration timer
  /** @type {ReturnType<typeof setInterval> | null} */
  let durationInterval = $state(null);
  /** @type {number | null} */
  let recordingStartedAt = $state(null);
  
  // Derived values
  let languageLabel = $derived(settings.language === 'en' ? 'EN' : 'HE');
  let modeLabel = $derived('Batch');
  let showTestButtons = $derived(dev);
  
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

      // Load transcription history
      try {
        history = /** @type {any} */ (await invoke('get_transcription_history')) || [];
      } catch (err) {
        console.error('Failed to load history:', err);
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
      
      unlisteners.push(await listen('indicator-warming-up', () => {
        indicatorState = 'warming_up';
        isTranscribing = true;
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

      // Accessibility permission check (macOS text injection requirement)
      try {
        const trusted = /** @type {boolean} */ (await invoke('check_accessibility_permission'));
        needsAccessibility = !trusted;
      } catch (_) {}

      unlisteners.push(await listen('accessibility-permission-needed', () => {
        needsAccessibility = true;
      }));

      // Self-update: backend checks on startup and emits when an update exists.
      unlisteners.push(await listen('update-available', (event) => {
        availableUpdate = /** @type {any} */ (event.payload);
        updateDismissed = false;
      }));

      unlisteners.push(await listen('update-progress', (event) => {
        updateProgress = Math.round(/** @type {number} */ (event.payload));
      }));

      // Prepend newly completed transcriptions to the history list.
      unlisteners.push(await listen('history-added', (event) => {
        const entry = /** @type {any} */ (event.payload);
        history = [entry, ...history.filter(h => h.id !== entry.id)];
      }));
    })();
    
    // Cleanup on unmount
    return () => {
      unlisteners.forEach(fn => fn());
      stopDurationTimer();
    };
  });
  
  function startDurationTimer() {
    if (durationInterval) return;

    recordingStartedAt = Date.now();
    recordingDuration = 0;
    durationInterval = setInterval(() => {
      if (!recordingStartedAt) {
        recordingDuration = 0;
        return;
      }
      recordingDuration = Math.floor((Date.now() - recordingStartedAt) / 1000);
    }, 200);
  }

  function stopDurationTimer() {
    if (durationInterval) {
      clearInterval(durationInterval);
      durationInterval = null;
    }
    recordingStartedAt = null;
    recordingDuration = 0;
  }
  
  /** @param {'en' | 'he'} language */
  async function startRecording(language) {
    try {
      await invoke('start_recording', { language });
    } catch (err) {
      error = String(err);
    }
  }

  async function stopRecording() {
    try {
      await invoke('stop_recording');
    } catch (err) {
      error = String(err);
    }
  }

  /** @param {'en' | 'he'} language */
  async function runTest(language) {
    if (testingLanguage || isRecording || isTranscribing) {
      return;
    }

    testingLanguage = language;
    testError = null;

    try {
      const text = /** @type {string} */ (await invoke('run_transcription_test', { language }));
      testResult = {
        language,
        text
      };
      lastTranscription = text;
    } catch (err) {
      testError = String(err);
    } finally {
      testingLanguage = '';
    }
  }
  
  /** @param {Record<string, unknown>} newSettings */
  function handleSettingsSave(newSettings) {
    settings = { ...settings, ...newSettings };
  }
  
  function dismissError() {
    error = null;
  }

  async function openAccessibilitySettings() {
    await invoke('open_accessibility_settings');
    // Poll for 10 s so the banner clears as soon as the user grants access
    for (let i = 0; i < 20; i++) {
      await new Promise(r => setTimeout(r, 500));
      try {
        const trusted = /** @type {boolean} */ (await invoke('check_accessibility_permission'));
        if (trusted) { needsAccessibility = false; break; }
      } catch (_) {}
    }
  }

  async function installUpdate() {
    updateError = null;
    updateInstalling = true;
    updateProgress = 0;
    try {
      // Downloads, installs, and relaunches — this call does not return on success.
      await invoke('download_and_install_update');
    } catch (e) {
      updateError = String(e);
      updateInstalling = false;
    }
  }

  // Dismiss the "Last Transcription" card (history keeps the entry).
  async function dismissLastTranscription() {
    lastTranscription = '';
    try {
      await invoke('clear_last_transcription');
    } catch (e) {
      console.error('clear_last_transcription failed:', e);
    }
  }

  /** @param {string} text */
  function copyText(text) {
    navigator.clipboard.writeText(text);
  }

  /** @param {number} id */
  async function deleteHistoryEntry(id) {
    history = history.filter(h => h.id !== id);
    try {
      await invoke('delete_transcription_entry', { id });
    } catch (e) {
      console.error('delete_transcription_entry failed:', e);
    }
  }

  async function clearHistory() {
    history = [];
    try {
      await invoke('clear_transcription_history');
    } catch (e) {
      console.error('clear_transcription_history failed:', e);
    }
  }

  /** @param {number} ts seconds since epoch */
  function formatHistoryTime(ts) {
    const d = new Date(ts * 1000);
    const now = new Date();
    const sameDay = d.toDateString() === now.toDateString();
    const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    return sameDay ? time : `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${time}`;
  }
</script>

<main class="app" data-tauri-drag-region>
  <!-- Title Bar -->
  <header class="title-bar" data-tauri-drag-region>
    <div class="app-name">
      <img class="app-logo" src="/favicon.png" alt="AirType Logo" width="32" height="32" />
      AirType
    </div>
    <button class="settings-btn" onclick={() => settingsOpen = true} aria-label="Open settings">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="3"></circle>
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
      </svg>
    </button>
  </header>

  <!-- Update Available Banner -->
  {#if availableUpdate && !updateDismissed}
    <div class="update-banner">
      {#if updateInstalling}
        <span>⬇️ <strong>Updating to v{availableUpdate.version}…</strong> {updateProgress}%</span>
        <div class="update-progress-track">
          <div class="update-progress-fill" style="width: {updateProgress}%"></div>
        </div>
      {:else}
        <span>🚀 <strong>Update available</strong> — v{availableUpdate.version} is ready to install.</span>
        <div class="update-actions">
          <button class="update-btn" onclick={installUpdate}>Install &amp; Restart</button>
          <button class="update-dismiss" onclick={() => updateDismissed = true} aria-label="Dismiss">Later</button>
        </div>
      {/if}
      {#if updateError}
        <span class="update-error">{updateError}</span>
      {/if}
    </div>
  {/if}

  <!-- Accessibility Permission Banner -->
  {#if needsAccessibility}
    <div class="accessibility-banner">
      <span>⚠️ <strong>Accessibility access needed</strong> — text injection won't work without it.</span>
      <button class="accessibility-btn" onclick={openAccessibilitySettings}>
        Open System Settings
      </button>
    </div>
  {/if}
  
  <!-- Main Content -->
  <section class="content">
    <!-- Status Indicator -->
    <StatusIndicator 
      {isRecording} 
      {isTranscribing} 
      duration={recordingDuration} 
    />
    
    <!-- Mode Badge -->
    <div class="badges">
      <span class="badge">{modeLabel}</span>
    </div>

    <!-- Record Buttons -->
    {#if isTranscribing}
      <button class="record-btn transcribing" disabled>Processing...</button>
    {:else if isRecording}
      <button class="record-btn recording" onclick={stopRecording}>
        Stop Recording ({recordingLanguage === 'he' ? 'Hebrew' : 'English'})
      </button>
    {:else}
      <div class="record-btns">
        <button class="record-btn lang-btn" onclick={() => startRecording('en')}>
          Record in English
        </button>
        <button class="record-btn lang-btn" onclick={() => startRecording('he')}>
          Record in Hebrew
        </button>
      </div>
    {/if}

    {#if showTestButtons}
      <div class="test-actions">
        <button
          class="test-btn"
          onclick={() => runTest('en')}
          disabled={!!testingLanguage || isRecording || isTranscribing}
        >
          {#if testingLanguage === 'en'}
            Testing English...
          {:else}
            Test English
          {/if}
        </button>
        <button
          class="test-btn"
          onclick={() => runTest('he')}
          disabled={!!testingLanguage || isRecording || isTranscribing}
        >
          {#if testingLanguage === 'he'}
            Testing Hebrew...
          {:else}
            Test Hebrew
          {/if}
        </button>
      </div>
    {/if}
    
    <!-- Hotkey Hint -->
    <p class="hotkey-hint">
      <kbd>{settings.hotkey_english}</kbd> English · <kbd>{settings.hotkey_hebrew}</kbd> Hebrew
    </p>
  </section>

  {#if testResult.text}
    <section class="transcription-preview animate-slideUp">
      <div class="preview-header">
        <span class="preview-label">
          {testResult.language === 'en' ? 'English Test Result' : 'Hebrew Test Result'}
        </span>
        <button class="copy-btn" onclick={() => navigator.clipboard.writeText(testResult.text)}>
          Copy
        </button>
      </div>
      <p class="preview-text">{testResult.text}</p>
    </section>
  {/if}
  
  {#if testError}
    <div class="error-toast animate-slideUp">
      <span>{testError}</span>
      <button class="dismiss-btn" onclick={() => testError = null}>×</button>
    </div>
  {/if}
  
  <!-- Transcription Preview -->
  {#if lastTranscription}
    <section class="transcription-preview animate-slideUp">
      <div class="preview-header">
        <span class="preview-label">Last Transcription</span>
        <div class="preview-actions">
          <button class="copy-btn" onclick={() => copyText(lastTranscription)}>
            Copy
          </button>
          <button class="dismiss-btn" onclick={dismissLastTranscription} aria-label="Dismiss last transcription" title="Dismiss">×</button>
        </div>
      </div>
      <p class="preview-text">{lastTranscription}</p>
    </section>
  {/if}

  <!-- Transcription History -->
  {#if history.length > 0}
    <section class="history-section">
      <div
        class="history-header"
        role="button"
        tabindex="0"
        onclick={() => historyOpen = !historyOpen}
        onkeydown={(e) => e.key === 'Enter' && (historyOpen = !historyOpen)}
      >
        <span class="history-title">History <span class="history-count">{history.length}</span></span>
        <div class="history-header-actions">
          {#if historyOpen}
            <button class="history-clear-btn" onclick={(e) => { e.stopPropagation(); clearHistory(); }} title="Clear all">Clear all</button>
          {/if}
          <span class="history-chevron">{historyOpen ? '▲' : '▼'}</span>
        </div>
      </div>

      {#if historyOpen}
        <ul class="history-list" style={lastTranscription ? 'padding-bottom: 72px' : ''}>
          {#each history as entry (entry.id)}
            <li class="history-item">
              <div class="history-item-main">
                <div class="history-item-meta">
                  <span class="history-lang">{entry.language === 'he' ? 'HE' : 'EN'}</span>
                  <span class="history-time">{formatHistoryTime(entry.timestamp)}</span>
                </div>
                <p class="history-text">{entry.text}</p>
              </div>
              <div class="history-item-actions">
                <button class="history-icon-btn" onclick={() => copyText(entry.text)} title="Copy" aria-label="Copy">⎘</button>
                <button class="history-icon-btn" onclick={() => deleteHistoryEntry(entry.id)} title="Delete" aria-label="Delete">×</button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
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
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.05em;
  }

  .app-logo {
    flex-shrink: 0;
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
    gap: 1.25rem;
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
  
  /* Record Buttons */
  .record-btns {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    justify-content: center;
  }

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

  .lang-btn {
    padding: 0.875rem 1.5rem;
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

  .test-actions {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    justify-content: center;
  }

  .test-btn {
    padding: 0.75rem 1.25rem;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    color: var(--color-text);
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: all var(--transition-normal);
  }

  .test-btn:hover:not(:disabled) {
    background: var(--color-surface-hover);
    border-color: var(--color-accent);
  }

  .test-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
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
    padding: 0.5rem 1rem;
    background: var(--color-surface);
    border-top: 1px solid var(--color-border);
  }
  
  .preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.25rem;
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
    font-size: 0.875rem;
    color: var(--color-text);
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
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

  /* Accessibility permission banner */
  .accessibility-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.6rem 1rem;
    background: rgba(245, 158, 11, 0.15);
    border-bottom: 1px solid rgba(245, 158, 11, 0.35);
    color: #fcd34d;
    font-size: 0.8125rem;
    flex-shrink: 0;
  }

  .accessibility-btn {
    white-space: nowrap;
    flex-shrink: 0;
    background: rgba(245, 158, 11, 0.2);
    border: 1px solid rgba(245, 158, 11, 0.4);
    border-radius: var(--radius-sm, 4px);
    color: #fcd34d;
    font-size: 0.75rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
    transition: background 0.15s;
  }

  .accessibility-btn:hover {
    background: rgba(245, 158, 11, 0.35);
  }

  /* Update available banner */
  .update-banner {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.6rem 1rem;
    background: rgba(99, 102, 241, 0.15);
    border-bottom: 1px solid rgba(99, 102, 241, 0.35);
    color: #c7d2fe;
    font-size: 0.8125rem;
    flex-shrink: 0;
  }

  .update-banner > span {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    justify-content: space-between;
  }

  .update-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    align-self: flex-end;
  }

  .update-btn {
    white-space: nowrap;
    background: var(--color-primary, #6366f1);
    border: 1px solid var(--color-primary, #6366f1);
    border-radius: var(--radius-sm, 4px);
    color: white;
    font-size: 0.75rem;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    transition: background 0.15s;
  }

  .update-btn:hover {
    background: #4f46e5;
  }

  .update-dismiss {
    background: none;
    border: none;
    color: #a5b4fc;
    font-size: 0.75rem;
    cursor: pointer;
    opacity: 0.8;
  }

  .update-dismiss:hover {
    opacity: 1;
  }

  .update-progress-track {
    width: 100%;
    height: 5px;
    background: rgba(99, 102, 241, 0.2);
    border-radius: 999px;
    overflow: hidden;
  }

  .update-progress-fill {
    height: 100%;
    background: var(--color-primary, #6366f1);
    border-radius: 999px;
    transition: width 0.2s ease;
  }

  .update-error {
    color: #fca5a5;
    font-size: 0.75rem;
  }

  /* preview-actions: copy + dismiss side by side */
  .preview-actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  /* History section */
  .history-section {
    flex-shrink: 0;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .history-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.4rem 1rem;
    cursor: pointer;
    user-select: none;
  }
  .history-header:hover .history-title { color: var(--color-text); }

  .history-title {
    font-size: 0.6875rem;
    font-weight: 500;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .history-count {
    background: var(--color-surface-elevated, #1a1a25);
    border: 1px solid var(--color-border);
    border-radius: 999px;
    font-size: 0.5625rem;
    padding: 0.05rem 0.45rem;
    color: var(--color-text-muted);
  }

  .history-header-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .history-clear-btn {
    background: none;
    border: none;
    font-size: 0.6875rem;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 0.15rem 0.4rem;
    border-radius: var(--radius-sm, 4px);
    transition: color 0.15s;
  }
  .history-clear-btn:hover { color: #fca5a5; }

  .history-chevron {
    font-size: 0.5rem;
    color: var(--color-text-muted);
  }

  .history-list {
    list-style: none;
    padding: 0;
    margin: 0;
    max-height: 200px;
    overflow-y: auto;
  }

  .history-item {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    padding: 0.45rem 1rem;
    border-top: 1px solid var(--color-border);
    transition: background 0.1s;
  }
  .history-item:hover { background: var(--color-surface-hover, rgba(255,255,255,0.03)); }

  .history-item-main {
    flex: 1;
    min-width: 0;
  }

  .history-item-meta {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 0.15rem;
  }

  .history-lang {
    font-size: 0.5625rem;
    font-weight: 600;
    color: var(--color-primary, #6366f1);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .history-time {
    font-size: 0.625rem;
    color: var(--color-text-muted);
  }

  .history-text {
    font-size: 0.8125rem;
    color: var(--color-text);
    line-height: 1.3;
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .history-item-actions {
    display: flex;
    align-items: center;
    gap: 0.1rem;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .history-item:hover .history-item-actions { opacity: 1; }

  .history-icon-btn {
    background: none;
    border: none;
    padding: 0.2rem 0.3rem;
    color: var(--color-text-muted);
    font-size: 0.875rem;
    line-height: 1;
    cursor: pointer;
    border-radius: var(--radius-sm, 4px);
    transition: all 0.1s;
  }
  .history-icon-btn:hover { color: var(--color-text); background: var(--color-surface-hover, rgba(255,255,255,0.05)); }
</style>
