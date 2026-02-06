<script>
  // @ts-nocheck
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  
  let state = $state('recording');
  let language = $state('en');
  
  onMount(() => {
    listen('indicator-show', (e) => {
      language = e.payload?.language || 'en';
      state = 'recording';
    });
    
    listen('indicator-transcribing', () => {
      state = 'transcribing';
    });
    
    listen('indicator-done', () => {
      state = 'done';
    });
    
    listen('indicator-hide', () => {
      state = 'recording';
    });
  });
  
  let gradient = $derived(
    state === 'recording' ? 'linear-gradient(135deg, #f87171 0%, #ef4444 100%)' :
    state === 'transcribing' ? 'linear-gradient(135deg, #a78bfa 0%, #8b5cf6 100%)' :
    state === 'done' ? 'linear-gradient(135deg, #4ade80 0%, #22c55e 100%)' : 
    'linear-gradient(135deg, #f87171 0%, #ef4444 100%)'
  );
  
  let icon = $derived(
    state === 'recording' ? '●' :
    state === 'transcribing' ? '◐' :
    state === 'done' ? '✓' : '●'
  );
  
  let label = $derived(
    state === 'recording' ? (language === 'he' ? 'מקליט' : 'Recording') :
    state === 'transcribing' ? (language === 'he' ? 'מעבד' : 'Processing') :
    state === 'done' ? (language === 'he' ? 'בוצע' : 'Done') : 'Recording'
  );
</script>

<div class="indicator" style="background: {gradient};">
  <span class="icon" class:pulse={state === 'recording'} class:spin={state === 'transcribing'}>
    {icon}
  </span>
  <span class="label">{label}</span>
</div>

<style>
  :global(*) { margin: 0; padding: 0; box-sizing: border-box; }
  :global(html), :global(body) {
    margin: 0; padding: 0; overflow: hidden;
    width: 100%; height: 100%;
    background: transparent;
  }
  
  .indicator {
    width: 100vw;
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border-radius: 18px;
    transition: background 0.3s ease;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  }
  
  .icon, .label {
    color: white;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    text-shadow: 0 1px 2px rgba(0,0,0,0.2);
  }
  
  .icon { font-size: 10px; }
  .label { font-size: 11px; font-weight: 600; letter-spacing: 0.3px; }
  
  .icon.pulse { animation: pulse 1s infinite; }
  .icon.spin { animation: spin 1s linear infinite; display: inline-block; }
  
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
