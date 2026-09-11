<script>
  // @ts-nocheck
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  let state = $state('idle');
  let language = $state('en');
  let errorSummary = $state('Error');
  let curGen = -1;

  async function dismissIndicator() {
    try {
      // Hide the reusable window without changing the recording session.
      await getCurrentWindow().hide();
    } catch (error) {
      console.error('Failed to dismiss indicator:', error);
    }
  }

  let dismissLabel = $derived(language === 'he' ? 'הסתרת המחוון' : 'Dismiss indicator');
  let dismissHint = $derived(
    language === 'he'
      ? 'הסתרת המחוון בלבד — הפעולה תמשיך'
      : 'Dismiss indicator — the current action will continue'
  );

  onMount(() => {
    // Single unified event from the backend — stale sessions are dropped by
    // comparing the generation counter.
    listen('phase-changed', (e) => {
      const { phase, language: lang, generation } = e.payload;
      if (generation < curGen) return; // discard stale
      curGen = generation;
      language = lang || 'en';
      state = phase; // idle | recording | processing | done
    });

    // Critical, flow-blocking errors switch the indicator to a red error state.
    // The backend makes the window visible and auto-hides it after a few seconds.
    // The payload's `summary` is a short, human-readable reason (e.g. "No
    // microphone") shown in the pill; the full detail goes to the main-UI toast.
    listen('error', (e) => {
      const p = e.payload;
      errorSummary = (p && typeof p === 'object' && p.summary) ? p.summary : 'Error';
      state = 'error';
    });

    // indicator-hide is still emitted by hide_indicator to force-hide the window.
    listen('indicator-hide', () => {
      state = 'idle';
    });
  });

  let gradient = $derived(
    state === 'recording'   ? 'linear-gradient(135deg, #f87171 0%, #ef4444 100%)' :
    state === 'warming'     ? 'linear-gradient(135deg, #fb923c 0%, #f97316 100%)' :
    state === 'processing'  ? 'linear-gradient(135deg, #a78bfa 0%, #8b5cf6 100%)' :
    state === 'done'        ? 'linear-gradient(135deg, #4ade80 0%, #22c55e 100%)' :
    state === 'error'       ? 'linear-gradient(135deg, #ef4444 0%, #b91c1c 100%)' :
    'linear-gradient(135deg, #94a3b8 0%, #64748b 100%)'
  );

  let icon = $derived(
    state === 'recording'  ? '●' :
    state === 'warming'    ? '◌' :
    state === 'processing' ? '◐' :
    state === 'done'       ? '✓' :
    state === 'error'      ? '✕' : '●'
  );

  let label = $derived(
    state === 'recording'  ? (language === 'he' ? 'מקליט'  : 'Recording')   :
    state === 'warming'    ? (language === 'he' ? 'מתחמם'  : 'Warming up')  :
    state === 'processing' ? (language === 'he' ? 'מעבד'   : 'Processing')  :
    state === 'done'       ? (language === 'he' ? 'בוצע'   : 'Done')        :
    state === 'error'      ? errorSummary                                   :
    'Recording'
  );
</script>

<div class="wrap">
  <div class="indicator" style="background: {gradient};">
    <span class="icon" class:pulse={state === 'recording' || state === 'warming'} class:spin={state === 'processing'}>
      {icon}
    </span>
    <span class="label" title={label}>{label}</span>
    <button
      class="dismiss"
      type="button"
      aria-label={dismissLabel}
      title={dismissHint}
      onclick={dismissIndicator}
    >
      <svg width="10" height="10" viewBox="0 0 12 12" fill="none" aria-hidden="true">
        <path d="m3 3 6 6m0-6-6 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </button>
  </div>
</div>

<style>
  :global(*) { margin: 0; padding: 0; box-sizing: border-box; }
  :global(html), :global(body) {
    margin: 0; padding: 0; overflow: hidden;
    width: 100%; height: 100%;
    background: transparent;
  }

  .wrap {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px 10px;
    background: transparent;
  }

  .indicator {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 0 6px 0 10px;
    border-radius: 18px;
    transition: background 0.12s ease;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  }

  .icon, .label {
    color: white;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    text-shadow: 0 1px 2px rgba(0,0,0,0.2);
  }

  .icon { font-size: 10px; flex-shrink: 0; }
  .label {
    min-width: 0;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dismiss {
    flex: 0 0 24px;
    width: 24px;
    height: 24px;
    margin-left: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.85);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .dismiss:hover {
    background: rgba(255, 255, 255, 0.24);
    color: white;
  }

  .dismiss:active { background: rgba(0, 0, 0, 0.12); }
  .dismiss:focus-visible { outline: 2px solid white; outline-offset: -2px; }

  .icon.pulse { animation: pulse 1s infinite; }
  .icon.spin  { animation: spin 1s linear infinite; display: inline-block; }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  @keyframes spin  { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
