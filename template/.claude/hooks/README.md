# Hooks

Place hook scripts here. Configure them in `settings.json`.

Example notification hook (`notify.sh`):
```bash
#!/bin/bash
osascript -e "display notification \"$1\" with title \"Claude Code\""
```
