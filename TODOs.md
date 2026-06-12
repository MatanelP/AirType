TODOs
- [IMPLEMENTED - UNDER REVIEW] indicator position + transparency: configurable bottom offset, alignment, size, x-offset via Settings > Advanced. shadow:false fixes the macOS box. preview button in settings.
- first run spin up time
- [IMPLEMENTED - UNDER REVIEW] warmup indicator: switched RunPod from /runsync to /run+poll; IN_QUEUE shows orange "Warming up", IN_PROGRESS shows purple "Processing". OpenAI goes straight to Processing.
- [IMPLEMENTED - UNDER REVIEW] double status bar icon: removed trayIcon from tauri.conf.json (was creating a second icon alongside the programmatic one in setup).
- [IMPLEMENTED - UNDER REVIEW] ui not showing on dock icon press: handle RunEvent::Reopen to show+focus main window when all windows are hidden.
- on macos make sure the app can sart on login when the flag is set in the settings.
- self update - check for update and installs + auto update mechanism
- changing hot key to alt, or control is crashing everything on macOS, need to fix that and thoghrly test. 
- history trancribes 
- x on  the last transcription in the ui to dismiss it

