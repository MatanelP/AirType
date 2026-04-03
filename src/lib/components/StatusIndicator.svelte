<script>
  /**
   * StatusIndicator - Visual feedback for recording state
   * Shows pulsing animation when recording
   */
  
  let { isRecording = false, isTranscribing = false, duration = 0 } = $props();
  
  /** @param {number} seconds */
  function formatDuration(seconds) {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }
  
  let statusText = $derived(
    isTranscribing ? 'Transcribing...' : 
    isRecording ? 'Recording' : 
    'Ready'
  );
  
  let statusClass = $derived(
    isTranscribing ? 'transcribing' :
    isRecording ? 'recording' : 
    'idle'
  );
</script>

<div class="status-indicator {statusClass}">
  <div class="orb-container">
    <div class="orb">
      <div class="orb-inner"></div>
    </div>
    {#if isRecording}
      <div class="pulse-ring"></div>
      <div class="pulse-ring delay"></div>
    {/if}
  </div>
  
  <div class="status-info">
    <span class="status-text">{statusText}</span>
    {#if isRecording}
      <span class="duration">{formatDuration(duration)}</span>
    {/if}
  </div>
</div>

<style>
  .status-indicator {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.5rem;
    width: 100%;
  }
  
  .orb-container {
    position: relative;
    width: 120px;
    height: 120px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  
  .orb {
    width: 80px;
    height: 80px;
    border-radius: 50%;
    background: var(--gradient-orb);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 
      0 0 40px var(--color-accent-glow),
      inset 0 0 20px rgba(255, 255, 255, 0.1);
    transition: all 0.3s ease;
  }
  
  .orb-inner {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.15);
    backdrop-filter: blur(10px);
  }
  
  /* Idle state */
  .idle .orb {
    background: var(--gradient-orb-idle);
    box-shadow: 
      0 0 20px var(--color-idle-glow),
      inset 0 0 20px rgba(255, 255, 255, 0.05);
  }
  
  /* Recording state */
  .recording .orb {
    background: var(--gradient-orb-recording);
    box-shadow: 
      0 0 60px var(--color-recording-glow),
      inset 0 0 20px rgba(255, 255, 255, 0.2);
    animation: breathe 1.5s ease-in-out infinite;
  }
  
  /* Transcribing state */
  .transcribing .orb {
    background: var(--gradient-orb-transcribing);
    box-shadow: 
      0 0 40px var(--color-transcribing-glow),
      inset 0 0 20px rgba(255, 255, 255, 0.15);
    animation: spin 2s linear infinite;
  }
  
  .pulse-ring {
    position: absolute;
    width: 80px;
    height: 80px;
    border-radius: 50%;
    border: 2px solid var(--color-recording);
    animation: pulse 2s ease-out infinite;
    opacity: 0;
  }
  
  .pulse-ring.delay {
    animation-delay: 0.5s;
  }
  
  @keyframes breathe {
    0%, 100% {
      transform: scale(1);
    }
    50% {
      transform: scale(1.05);
    }
  }
  
  @keyframes pulse {
    0% {
      transform: scale(1);
      opacity: 0.8;
    }
    100% {
      transform: scale(2);
      opacity: 0;
    }
  }
  
  @keyframes spin {
    0% {
      transform: rotate(0deg);
    }
    100% {
      transform: rotate(360deg);
    }
  }
  
  .status-info {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    min-height: 4.5rem;
    justify-content: center;
    width: 100%;
  }
  
  .status-text {
    font-size: 1.25rem;
    font-weight: 500;
    color: var(--color-text);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    text-align: center;
  }
  
  .recording .status-text {
    color: var(--color-recording);
  }
  
  .transcribing .status-text {
    color: var(--color-transcribing);
  }
  
  .duration {
    font-size: 2rem;
    font-weight: 300;
    font-family: 'JetBrains Mono', monospace;
    color: var(--color-text-muted);
    text-align: center;
  }
</style>
