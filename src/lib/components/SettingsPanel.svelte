<script>
  // @ts-nocheck
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  let { 
    isOpen = false, 
    settings = {}, 
    onClose = () => {}, 
    onSettingsChange = () => {} 
  } = $props();

  let localSettings = $state({});
  let isSaving = $state(false);
  let saveTimeout = $state(null);
  let recordingHotkeyFor = $state(null);
  let modelStatus = $state([]);
  let downloadingModel = $state(null);
  let downloadProgress = $state(0);
  let showApiKey = $state(false);
  let keyValidation = $state(null); // null | 'checking' | 'valid' | 'invalid'

  // Sync localSettings when settings prop changes
  $effect(() => {
    localSettings = { ...settings };
  });

  $effect(() => {
    if (isOpen) {
      refreshSettings();
      loadModelStatus();
    }
  });

  onMount(() => {
    let unlisteners = [];
    
    (async () => {
      unlisteners.push(await listen('model-download-progress', (event) => {
        const data = event.payload;
        downloadProgress = data.progress;
      }));
      
      unlisteners.push(await listen('model-download-complete', () => {
        downloadingModel = null;
        downloadProgress = 0;
        loadModelStatus();
      }));
    })();
    
    return () => unlisteners.forEach(fn => fn());
  });

  async function loadModelStatus() {
    try {
      modelStatus = await invoke('get_model_status');
    } catch (e) {
      console.error('Failed to load model status:', e);
    }
  }

  async function refreshSettings() {
    try {
      const loadedSettings = await invoke('get_settings');
      localSettings = { ...settings, ...loadedSettings };
      onSettingsChange(localSettings);
    } catch (e) {
      console.error('Failed to refresh settings:', e);
    }
  }

  async function downloadModel(size) {
    downloadingModel = size;
    downloadProgress = 0;
    try {
      await invoke('download_model', { size });
    } catch (e) {
      console.error('Failed to download model:', e);
      downloadingModel = null;
    }
  }

  let keyValidateTimeout = null;
  let rpKeyValidation = $state(null); // null | 'checking' | 'valid' | 'invalid'
  let showRpKey = $state(false);
  let rpKeyValidateTimeout = null;

  async function validateApiKey(key) {
    if (!key || key.length < 10) {
      keyValidation = null;
      return;
    }
    keyValidation = 'checking';
    try {
      const valid = await invoke('validate_openai_key', { apiKey: key });
      keyValidation = valid ? 'valid' : 'invalid';
    } catch (e) {
      keyValidation = 'invalid';
    }
  }

  function handleApiKeyChange(value) {
    updateSetting('openai_api_key', value);
    if (keyValidateTimeout) clearTimeout(keyValidateTimeout);
    keyValidateTimeout = setTimeout(() => validateApiKey(value), 800);
  }

  async function validateRpKey(key, endpointId) {
    if (!key || key.length < 10 || !endpointId || endpointId.length < 5) {
      rpKeyValidation = null;
      return;
    }
    rpKeyValidation = 'checking';
    try {
      const valid = await invoke('validate_runpod_key', { apiKey: key, endpointId });
      rpKeyValidation = valid ? 'valid' : 'invalid';
    } catch (e) {
      rpKeyValidation = 'invalid';
    }
  }

  function handleRpKeyChange(value) {
    updateSetting('runpod_api_key', value);
    if (rpKeyValidateTimeout) clearTimeout(rpKeyValidateTimeout);
    rpKeyValidateTimeout = setTimeout(() => validateRpKey(value, localSettings.runpod_endpoint_id || ''), 800);
  }

  function handleRpEndpointChange(value) {
    updateSetting('runpod_endpoint_id', value);
    if (rpKeyValidateTimeout) clearTimeout(rpKeyValidateTimeout);
    rpKeyValidateTimeout = setTimeout(() => validateRpKey(localSettings.runpod_api_key || '', value), 800);
  }

  async function saveSettings() {
    isSaving = true;
    try {
      await invoke('save_settings', { settings: localSettings });
      onSettingsChange(localSettings);
    } catch (e) {
      console.error('Failed to save settings:', e);
    } finally {
      isSaving = false;
    }
  }

  function debouncedSave() {
    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(() => saveSettings(), 300);
  }

  async function updateSetting(key, value) {
    console.log(`[AirType] Setting changed: ${key} = ${value}`);
    localSettings = { ...localSettings, [key]: value };
    
    if (key === 'hotkey_english' || key === 'hotkey_hebrew') {
      console.log(`[AirType] Hotkey updated: ${key} -> "${value}"`);
    }
    
    if (key === 'start_on_login') {
      try {
        await invoke('set_autostart', { enabled: value });
      } catch (e) {
        console.error('Failed to set autostart:', e);
      }
    }
    
    if (key === 'model_size') {
      const model = modelStatus.find(m => m.size === value);
      if (model && !model.downloaded) {
        downloadModel(value);
      }
    }
    
    debouncedSave();
  }

  let pendingModifiers = $state([]);
  let pendingKey = $state(null);
  let lastModifierCode = $state(null);

  function startRecordingHotkey(type) {
    recordingHotkeyFor = type;
    pendingModifiers = [];
    pendingKey = null;
    lastModifierCode = null;
  }

  function handleKeydown(e) {
    if (!recordingHotkeyFor) return;
    
    e.preventDefault();
    e.stopPropagation();
    
    console.log(`[AirType] Keydown: key=${e.key}, code=${e.code}, altKey=${e.altKey}, ctrlKey=${e.ctrlKey}`);
    
    if (e.key === 'Escape') {
      console.log('[AirType] Escape pressed, canceling hotkey recording');
      recordingHotkeyFor = null;
      pendingModifiers = [];
      pendingKey = null;
      lastModifierCode = null;
      return;
    }
    
    // Check if this is a modifier key press
    const isModifierKey = ['AltLeft', 'AltRight', 'ControlLeft', 'ControlRight', 'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight'].includes(e.code);
    
    if (isModifierKey) {
      // Track which modifier was pressed (for modifier-only hotkeys)
      console.log(`[AirType] Modifier key pressed: ${e.code}`);
      lastModifierCode = e.code;
      pendingModifiers = [e.code];
    } else {
      // Non-modifier key - build the full combination
      const mods = [];
      if (e.ctrlKey) mods.push('Ctrl');
      if (e.altKey) mods.push('Alt');
      if (e.shiftKey) mods.push('Shift');
      if (e.metaKey) mods.push('Super');
      pendingModifiers = mods;
      pendingKey = e.code.replace('Key', '').replace('Digit', '');
      
      // Save immediately when a non-modifier key is pressed
      const hotkey = mods.length > 0 ? [...mods, pendingKey].join('+') : pendingKey;
      console.log(`[AirType] Key combination detected: ${hotkey}`);
      updateSetting(recordingHotkeyFor, hotkey);
      recordingHotkeyFor = null;
      pendingModifiers = [];
      pendingKey = null;
      lastModifierCode = null;
    }
  }

  function handleKeyup(e) {
    if (!recordingHotkeyFor) return;
    
    e.preventDefault();
    e.stopPropagation();
    
    console.log(`[AirType] Keyup: key=${e.key}, code=${e.code}, lastModifierCode=${lastModifierCode}`);
    
    // Check if this is the release of a modifier-only keypress
    const isModifierRelease = ['AltLeft', 'AltRight', 'ControlLeft', 'ControlRight', 'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight'].includes(e.code);
    
    if (isModifierRelease && lastModifierCode === e.code) {
      // Single modifier key was pressed and released - save it
      console.log(`[AirType] Modifier-only hotkey detected: ${e.code}`);
      updateSetting(recordingHotkeyFor, e.code);
      recordingHotkeyFor = null;
      pendingModifiers = [];
      pendingKey = null;
      lastModifierCode = null;
    }
  }

  function handleBackdropClick(e) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} onkeyup={handleKeyup} />

{#if isOpen}
  <div class="settings-backdrop" onclick={handleBackdropClick} role="presentation">
    <div class="settings-panel" role="dialog" aria-label="Settings">
      <div class="settings-header">
        <h2>Settings</h2>
        {#if isSaving}
          <span class="saving-indicator">Saved</span>
        {/if}
        <button class="close-btn" onclick={onClose} aria-label="Close settings">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 6L6 18M6 6l12 12"/>
          </svg>
        </button>
      </div>
      
      <div class="settings-content">
        <section class="settings-section">
          <h3>Hotkeys</h3>
          <p class="section-note">Press a key combination or single modifier key</p>
          
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">English</span>
              <span class="setting-desc">Trigger recording in English</span>
            </div>
            {#if recordingHotkeyFor === 'hotkey_english'}
              <button class="hotkey-btn recording" onclick={() => { recordingHotkeyFor = null; pendingModifiers = []; pendingKey = null; }}>
                {pendingModifiers.length > 0 || pendingKey ? [...pendingModifiers, pendingKey].filter(Boolean).join('+') : 'Press keys...'}
              </button>
            {:else}
              <button class="hotkey-btn" onclick={() => startRecordingHotkey('hotkey_english')}>
                {localSettings.hotkey_english || 'Not set'}
              </button>
            {/if}
          </div>
          
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Hebrew</span>
              <span class="setting-desc">Trigger recording in Hebrew</span>
            </div>
            {#if recordingHotkeyFor === 'hotkey_hebrew'}
              <button class="hotkey-btn recording" onclick={() => { recordingHotkeyFor = null; pendingModifiers = []; pendingKey = null; }}>
                {pendingModifiers.length > 0 || pendingKey ? [...pendingModifiers, pendingKey].filter(Boolean).join('+') : 'Press keys...'}
              </button>
            {:else}
              <button class="hotkey-btn" onclick={() => startRecordingHotkey('hotkey_hebrew')}>
                {localSettings.hotkey_hebrew || 'Not set'}
              </button>
            {/if}
          </div>
        </section>
        
        <section class="settings-section">
          <h3>Transcription Engine</h3>
          
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Engine</span>
              <span class="setting-desc">Local Whisper is free & offline. OpenAI streams text live.</span>
            </div>
            <select 
              class="select-input"
              value={localSettings.transcription_engine || 'localwhisper'}
              onchange={(e) => updateSetting('transcription_engine', e.target.value)}
            >
              <option value="localwhisper">Local Whisper (free, offline)</option>
              <option value="openai">OpenAI (paid, live)</option>
            </select>
          </div>
          
          {#if localSettings.transcription_engine === 'openai'}
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">OpenAI API Key</span>
                <span class="setting-desc">For English live transcription (gpt-4o-transcribe)</span>
              </div>
              <div class="api-key-input">
                <input 
                  type={showApiKey ? 'text' : 'password'}
                  class="text-input"
                  class:input-valid={keyValidation === 'valid'}
                  class:input-invalid={keyValidation === 'invalid'}
                  placeholder="sk-..."
                  value={localSettings.openai_api_key || ''}
                  oninput={(e) => handleApiKeyChange(e.target.value)}
                />
                <button class="icon-btn" onclick={() => showApiKey = !showApiKey} aria-label="Toggle visibility">
                  {showApiKey ? '🙈' : '👁'}
                </button>
              </div>
            </div>
            {#if keyValidation === 'checking'}
              <p class="key-status checking">⏳ Validating key...</p>
            {:else if keyValidation === 'valid'}
              <p class="key-status valid">✓ API key is valid</p>
            {:else if keyValidation === 'invalid'}
              <p class="key-status invalid">✗ Invalid API key</p>
            {/if}
            
            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">RunPod API Key</span>
                <span class="setting-desc">For Hebrew transcription (ivrit-ai via RunPod Serverless)</span>
              </div>
              <div class="api-key-input">
                <input 
                  type={showRpKey ? 'text' : 'password'}
                  class="text-input"
                  class:input-valid={rpKeyValidation === 'valid'}
                  class:input-invalid={rpKeyValidation === 'invalid'}
                  placeholder="rp_..."
                  value={localSettings.runpod_api_key || ''}
                  oninput={(e) => handleRpKeyChange(e.target.value)}
                />
                <button class="icon-btn" onclick={() => showRpKey = !showRpKey} aria-label="Toggle visibility">
                  {showRpKey ? '🙈' : '👁'}
                </button>
              </div>
            </div>

            <div class="setting-row">
              <div class="setting-info">
                <span class="setting-label">RunPod Endpoint ID</span>
                <span class="setting-desc">Deploy ivrit-ai from <a href="https://www.runpod.io/console/hub/ivrit-ai/runpod-serverless" target="_blank" style="color: var(--accent)">RunPod Hub</a></span>
              </div>
              <div class="api-key-input">
                <input 
                  type="text"
                  class="text-input"
                  class:input-valid={rpKeyValidation === 'valid'}
                  class:input-invalid={rpKeyValidation === 'invalid'}
                  placeholder="e.g. abc123xyz"
                  value={localSettings.runpod_endpoint_id || ''}
                  oninput={(e) => handleRpEndpointChange(e.target.value)}
                />
              </div>
            </div>
            {#if rpKeyValidation === 'checking'}
              <p class="key-status checking">⏳ Validating RunPod endpoint...</p>
            {:else if rpKeyValidation === 'valid'}
              <p class="key-status valid">✓ RunPod endpoint is reachable</p>
            {:else if rpKeyValidation === 'invalid'}
              <p class="key-status invalid">✗ Cannot reach endpoint — check API key and endpoint ID</p>
            {/if}
            
            <p class="section-note">
              <strong>English</strong>: Live streaming via OpenAI (text appears as you speak).<br/>
              <strong>Hebrew</strong>: Best-in-class ivrit-ai model via RunPod Serverless (pay-per-second, text after recording).
            </p>
          {:else}
            <p class="section-note">Text appears after you stop recording. No internet or API key needed.</p>
          {/if}
        </section>
        
        <section class="settings-section">
          <h3>Recording</h3>
          
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Mode</span>
              <span class="setting-desc">Hold: release to stop. Toggle: press again to stop</span>
            </div>
            <select 
              class="select-input"
              value={localSettings.hotkey_mode || 'hold'}
              onchange={(e) => updateSetting('hotkey_mode', e.target.value)}
            >
              <option value="hold">Hold to record</option>
              <option value="toggle">Toggle recording</option>
            </select>
          </div>
        </section>
        
        {#if localSettings.transcription_engine !== 'openai'}
        <section class="settings-section">
          <h3>Local Models</h3>
          
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Whisper model</span>
              <span class="setting-desc">Used for both English and Hebrew. Use <strong>small</strong> or larger for good Hebrew accuracy.</span>
            </div>
            <select 
              class="select-input"
              value={localSettings.model_size || 'base'}
              onchange={(e) => updateSetting('model_size', e.target.value)}
            >
              {#each modelStatus as model}
                <option value={model.size}>
                  {model.size} ({model.size_mb}MB) {model.downloaded ? '✓' : ''}
                </option>
              {/each}
              {#if modelStatus.length === 0}
                <option value="tiny">tiny (75MB)</option>
                <option value="base">base (150MB)</option>
                <option value="small">small (500MB)</option>
                <option value="medium">medium (1.5GB)</option>
                <option value="large">large (3GB)</option>
              {/if}
            </select>
          </div>
          
          {#if downloadingModel}
            <div class="download-progress">
              <span>Downloading {downloadingModel}...</span>
              <div class="progress-bar">
                <div class="progress-fill" style="width: {downloadProgress}%"></div>
              </div>
              <span class="progress-text">{downloadProgress.toFixed(0)}%</span>
            </div>
          {/if}
          
          <p class="section-note">
            <strong>tiny/base</strong>: Fast, good for English.
            <strong>small</strong>: Best balance — recommended for Hebrew.
            <strong>medium/large</strong>: Most accurate but slow on CPU.
          </p>
        </section>
        {/if}
        
        <section class="settings-section">
          <h3>System</h3>
          
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Start minimized</span>
              <span class="setting-desc">Hide window on launch, show in tray</span>
            </div>
            <label class="toggle">
              <input 
                type="checkbox" 
                checked={localSettings.start_minimized || false}
                onchange={(e) => updateSetting('start_minimized', e.target.checked)}
              />
              <span class="toggle-slider"></span>
            </label>
          </div>
          
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Launch on login</span>
              <span class="setting-desc">Start AirType when you log in</span>
            </div>
            <label class="toggle">
              <input 
                type="checkbox" 
                checked={localSettings.start_on_login || false}
                onchange={(e) => updateSetting('start_on_login', e.target.checked)}
              />
              <span class="toggle-slider"></span>
            </label>
          </div>
        </section>
      </div>
    </div>
  </div>
{/if}

<style>
  .settings-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(12px);
    z-index: 100;
  }
  
  .settings-panel {
    position: fixed;
    right: 0;
    top: 0;
    bottom: 0;
    width: min(400px, 90vw);
    background: var(--color-surface, #0d0d12);
    border-left: 1px solid var(--color-border, #1a1a25);
    display: flex;
    flex-direction: column;
    animation: slideIn 0.2s ease-out;
  }
  
  @keyframes slideIn {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }
  
  .settings-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem 1.5rem;
    border-bottom: 1px solid var(--color-border, #1a1a25);
  }
  
  .settings-header h2 {
    margin: 0;
    flex: 1;
    font-size: 1rem;
    font-weight: 600;
    color: var(--color-text, #f0f0f5);
  }
  
  .saving-indicator {
    font-size: 0.75rem;
    color: var(--color-success, #10b981);
    font-weight: 500;
  }
  
  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: transparent;
    border: none;
    color: var(--color-text-muted, #8888a0);
    cursor: pointer;
    border-radius: var(--radius-md, 8px);
    transition: all 0.15s;
  }
  
  .close-btn:hover {
    background: var(--color-surface-hover, #1a1a25);
    color: var(--color-text, #f0f0f5);
  }
  
  .settings-content {
    flex: 1;
    overflow-y: auto;
    padding: 1rem 1.5rem;
  }
  
  .settings-section {
    margin-bottom: 1.75rem;
  }
  
  .settings-section h3 {
    font-size: 0.6875rem;
    color: var(--color-text-muted, #8888a0);
    margin: 0 0 0.5rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 600;
  }
  
  .section-note {
    font-size: 0.75rem;
    color: var(--color-text-muted, #8888a0);
    margin: 0 0 0.75rem;
    font-style: italic;
  }
  
  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 0;
    border-bottom: 1px solid var(--color-border, #1a1a25);
  }
  
  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }
  
  .setting-label {
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--color-text, #f0f0f5);
  }
  
  .setting-desc {
    font-size: 0.6875rem;
    color: var(--color-text-muted, #8888a0);
  }
  
  .hotkey-btn {
    padding: 0.5rem 1rem;
    background: var(--color-surface-elevated, #1a1a25);
    border: 1px solid var(--color-border, #2a2a35);
    border-radius: var(--radius-md, 8px);
    color: var(--color-text, #f0f0f5);
    font-size: 0.8125rem;
    font-family: monospace;
    cursor: pointer;
    transition: all 0.15s;
    min-width: 100px;
    text-align: center;
  }
  
  .hotkey-btn:hover {
    background: var(--color-surface-hover, #252530);
    border-color: var(--color-primary, #6366f1);
  }
  
  .hotkey-btn.recording {
    background: var(--color-primary, #6366f1);
    border-color: var(--color-primary, #6366f1);
    animation: pulse 1s infinite;
  }
  
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.7; }
  }
  
  .select-input {
    padding: 0.5rem 2rem 0.5rem 0.75rem;
    background-color: #1a1a25;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%238888a0' d='M6 8L1 3h10z'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 0.5rem center;
    border: 1px solid #2a2a35;
    border-radius: 8px;
    color: #f0f0f5;
    font-size: 0.8125rem;
    cursor: pointer;
    min-width: 150px;
    -webkit-appearance: none;
    -moz-appearance: none;
    appearance: none;
  }
  
  .select-input:focus {
    outline: none;
    border-color: #6366f1;
  }
  
  .select-input option {
    background: #1a1a25;
    color: #f0f0f5;
    padding: 0.5rem;
  }
  
  .toggle {
    position: relative;
    display: inline-block;
    width: 44px;
    height: 24px;
  }
  
  .toggle input {
    opacity: 0;
    width: 0;
    height: 0;
  }
  
  .toggle-slider {
    position: absolute;
    cursor: pointer;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: var(--color-surface-elevated, #1a1a25);
    border: 1px solid var(--color-border, #2a2a35);
    transition: 0.2s;
    border-radius: 24px;
  }
  
  .toggle-slider:before {
    position: absolute;
    content: "";
    height: 18px;
    width: 18px;
    left: 2px;
    bottom: 2px;
    background-color: var(--color-text-muted, #8888a0);
    transition: 0.2s;
    border-radius: 50%;
  }
  
  .toggle input:checked + .toggle-slider {
    background-color: var(--color-primary, #6366f1);
    border-color: var(--color-primary, #6366f1);
  }
  
  .toggle input:checked + .toggle-slider:before {
    transform: translateX(20px);
    background-color: white;
  }
  
  .download-progress {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem;
    background: var(--color-surface-elevated, #1a1a25);
    border-radius: var(--radius-md, 8px);
    margin-top: 0.5rem;
  }
  
  .progress-bar {
    height: 6px;
    background: var(--color-border, #2a2a35);
    border-radius: 3px;
    overflow: hidden;
  }
  
  .progress-fill {
    height: 100%;
    background: var(--color-primary, #6366f1);
    transition: width 0.3s ease;
  }
  
  .progress-text {
    font-size: 0.75rem;
    color: var(--color-text-muted, #8888a0);
    text-align: right;
  }
  
  .api-key-input {
    display: flex;
    gap: 0.25rem;
    align-items: center;
  }
  
  .text-input {
    padding: 0.5rem 0.75rem;
    background: var(--color-surface-elevated, #1a1a25);
    border: 1px solid var(--color-border, #2a2a35);
    border-radius: var(--radius-md, 8px);
    color: var(--color-text, #f0f0f5);
    font-size: 0.8125rem;
    font-family: monospace;
    width: 180px;
  }
  
  .text-input:focus {
    outline: none;
    border-color: var(--color-primary, #6366f1);
  }
  
  .text-input.input-valid {
    border-color: #10b981;
  }
  
  .text-input.input-invalid {
    border-color: #ef4444;
  }
  
  .key-status {
    font-size: 0.75rem;
    margin: 0.25rem 0 0;
    padding: 0;
  }
  .key-status.checking { color: var(--color-text-muted, #8888a0); }
  .key-status.valid { color: #10b981; }
  .key-status.invalid { color: #ef4444; }
  
  .icon-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    font-size: 0.875rem;
    padding: 0.25rem;
  }
  
  .badge {
    font-size: 0.75rem;
    padding: 0.25rem 0.75rem;
    border-radius: 12px;
    font-weight: 500;
  }
  
  .badge-success {
    background: rgba(16, 185, 129, 0.15);
    color: #10b981;
  }
  
  .download-btn {
    padding: 0.375rem 1rem;
    background: var(--color-primary, #6366f1);
    border: none;
    border-radius: var(--radius-md, 8px);
    color: white;
    font-size: 0.8125rem;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  
  .download-btn:hover { opacity: 0.85; }
  .download-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
