#!/usr/bin/env bash
# Start the debug APK on a running emulator (or device) and check that the
# app shows its sign-in screen without crashing. Leaves a screenshot, the
# screen's UI tree and the log in target/smoke/.
#   scripts/android-smoke.sh target/mobile/Mtrx-android-debug.apk
set -euo pipefail
cd "$(dirname "$0")/.."

apk="$1"
app=org.mtrx.app
out=target/smoke
mkdir -p "$out"

adb wait-for-device
adb install -r "$apk"
adb logcat -c
adb shell am start -W -n "$app/.MainActivity"

# Fresh install: no saved session, so the sign-in screen once restoring is done.
shown=false
for _ in $(seq 30); do
  sleep 2
  adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1 || continue
  adb exec-out cat /sdcard/ui.xml >"$out/android-ui.xml"
  if grep -q 'Sign in to your Matrix account' "$out/android-ui.xml"; then
    shown=true
    break
  fi
done
sleep 2

adb exec-out screencap -p >"$out/android.png"
adb logcat -d >"$out/android-logcat.txt"
pid="$(adb shell pidof "$app" || true)"

status=0
if grep -q 'FATAL EXCEPTION' "$out/android-logcat.txt"; then
  echo "The app crashed:"
  grep -A 30 'FATAL EXCEPTION' "$out/android-logcat.txt"
  status=1
fi
if [[ -z "$pid" ]]; then
  echo "The app is not running."
  status=1
fi
if [[ "$shown" != true ]]; then
  echo "The sign-in screen did not show."
  status=1
fi
[[ $status -eq 0 ]] && echo "Running as pid $pid, sign-in screen shown."
exit $status
