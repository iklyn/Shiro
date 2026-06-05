#!/usr/bin/env bash
# Reset all macOS privacy permissions for Shiro during development.
#
# Why: Shiro is ad-hoc signed, so every `tauri build` changes its code
# signature. macOS TCC ties Accessibility / Screen Recording grants to that
# signature, so after each rebuild the old grant goes stale (toggle looks ON
# but doesn't work) and a fresh build adds a NEW row, leaving ghost duplicates.
#
# This clears the entries tccutil can manage. "Failed to reset" just means
# there was nothing to reset for that service — safe to ignore.
#
# NOTE: ghost duplicate rows from PREVIOUS signatures can only be removed in
# System Settings → Privacy & Security → <pane> with the (–) button (needs
# Touch ID). Notifications is keyed by bundle id only, so it never duplicates.

BUNDLE_ID="com.kalyan.shiro"

SERVICES=(
  Accessibility
  ScreenCapture
  ListenEvent       # Input Monitoring
  PostEvent         # synthetic keystrokes
  AppleEvents       # automation
  Camera
  Microphone
)

echo "Resetting TCC permissions for $BUNDLE_ID …"
for svc in "${SERVICES[@]}"; do
  if tccutil reset "$svc" "$BUNDLE_ID" >/dev/null 2>&1; then
    echo "  ✓ $svc"
  else
    echo "  – $svc (nothing to reset)"
  fi
done

echo
echo "Done. Notifications can't be reset via CLI — toggle it in System Settings"
echo "if needed. Ghost duplicate rows: remove with (–) in the relevant pane."
