#!/usr/bin/env bash
# End-to-end test: LCL for Android on an emulator (or a phone on USB) against
# a real `lcl-remote serve` on this PC. Nothing is mocked: Android Keystore,
# TLS, the PC's trust store and the LCL engine are the real ones.
#
#   ANDROID_HOME=/path/to/sdk android/tools/e2e.sh
#
# Settings, all optional:
#   LCL_REMOTE          the lcl-remote binary (default: remote/target/debug/lcl-remote)
#   LCL_SPEC            Core 0.1.0 package (default: canonical/LCL_Core_0.1.0)
#   LCL_LOCALIZED_SPEC  Core 0.2.0 package (default: canonical/LCL_Core_0.2.0)
#   E2E_PORT            port for the service under test (default: 47310)
#   E2E_ADDRESS         where the device reaches this PC (default: 10.0.2.2:$E2E_PORT,
#                       the emulator's alias for the host; a phone needs the LAN address)
#   E2E_DIR             working directory (default: a new temporary one)
#   E2E_REBOOT=0        skip the reboot phase
#
# The service runs with its own XDG directories inside E2E_DIR, so this PC's
# real pairings and settings are never touched.
set -euo pipefail

android=$(cd "$(dirname "$0")/.." && pwd)
repo=$(dirname "$android")
adb="${ANDROID_HOME:?set ANDROID_HOME}/platform-tools/adb"
remote=${LCL_REMOTE:-$repo/remote/target/debug/lcl-remote}
spec=${LCL_SPEC:-$repo/canonical/LCL_Core_0.1.0}
localized=${LCL_LOCALIZED_SPEC:-$repo/canonical/LCL_Core_0.2.0}
port=${E2E_PORT:-47310}
address=${E2E_ADDRESS:-10.0.2.2:$port}
work=${E2E_DIR:-$(mktemp -d -t lcl-e2e.XXXXXX)}
out=$work/out
runner=io.lcl.workspace.test/androidx.test.runner.AndroidJUnitRunner

mkdir -p "$out"
export XDG_CONFIG_HOME=$work/config XDG_STATE_HOME=$work/state XDG_DATA_HOME=$work/data
project=$XDG_DATA_HOME/lcl/workspace
todo=$work/todo
mkdir -p "$project" "$todo"
printf 'Buy milk\n' >"$todo/todo.txt"
# The manual's file example, pointed at this run's own folder.
sed "s|/tmp/lcl-manual/todo|$todo|" "$repo/users_manual/examples/11/todo.lcl" >"$project/todo.lcl"
cp "$repo/users_manual/examples/11/todo.lcl" "$project/notes.lcl.txt"

log() { printf '%s  %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$out/summary.txt"; }

# check DESCRIPTION COMMAND...: a fact on the PC, or the run stops.
check() {
    local what=$1
    shift
    if "$@"; then log "PC: $what"; else log "FAIL on the PC: $what"; exit 1; fi
}
absent() { ! grep -q "$1" "$2"; }

serve() {
    "$remote" serve --spec "$spec" --localized-spec "$localized" --port "$port" >>"$out/serve.log" 2>&1 &
    echo $! >"$work/serve.pid"
    for _ in $(seq 100); do
        "$remote" status --json | grep -q '"running": true' && return 0
        sleep 0.2
    done
    log "the service did not start"; cat "$out/serve.log"; return 1
}
stop_serve() {
    local pid
    pid=$(cat "$work/serve.pid" 2>/dev/null) || return 0
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}
finish() {
    "$adb" pull /sdcard/Android/data/io.lcl.workspace/files/shots "$out/" >/dev/null 2>&1 || true
    stop_serve
}
trap finish EXIT

# One phase of RemoteEndToEndTest, in a fresh app process. Its output and its
# log lines are kept as NN_name.txt and NN_name.log, in the order run.
phase() {
    local name=$1
    shift
    # Counted from the records on disk: phases also run in the background.
    local done record
    done=$(find "$out" -maxdepth 1 -name '[0-9][0-9]_*.txt' | wc -l)
    record="$out/$(printf %02d $((done + 1)))_$name"
    "$adb" logcat -c
    "$adb" shell am instrument -w "$@" -e class "io.lcl.workspace.RemoteEndToEndTest#$name" "$runner" >"$record.txt" 2>&1 || true
    "$adb" logcat -d -s LclE2E:I >"$record.log" 2>&1 || true
    if grep -q '^OK (1 test)' "$record.txt"; then
        log "PASS $name"
    else
        log "FAIL $name"
        tail -60 "$record.txt"
        return 1
    fi
}

wait_log() {
    local marker=$1 seconds=$2
    for _ in $(seq $((seconds * 2))); do
        "$adb" logcat -d -s LclE2E:I | grep -q "$marker" && return 0
        sleep 0.5
    done
    log "timed out waiting for $marker"
    return 1
}

wake() {
    "$adb" shell input keyevent KEYCODE_WAKEUP
    "$adb" shell wm dismiss-keyguard || true
    "$adb" shell svc power stayon true
    # An emulator's own System UI can fail to start in time after a reboot and
    # cover the screen with its "isn't responding" dialog. Such dialogs are
    # hidden here; instead, no_lcl_failures fails the run on any ANR or crash
    # the system records for LCL itself.
    "$adb" shell settings put global hide_error_dialogs 1
}

# The system's own record of application failures, which a hidden dialog
# does not erase.
no_lcl_failures() {
    local found
    found=$("$adb" shell 'for t in data_app_anr data_app_crash data_app_native_crash; do dumpsys dropbox --print $t; done' | grep -E '^Process: io\.lcl\.workspace' || true)
    if [ -n "$found" ]; then
        log "FAIL: the system recorded an ANR or crash of LCL"
        echo "$found"
        return 1
    fi
    log "the system recorded no ANR or crash of LCL"
}

log "work directory $work; service on port $port; device reaches it at $address"
"$adb" wait-for-device
wake

log "building the app and its instrumented tests"
(cd "$android" && ./gradlew --console=plain -q assembleDebug assembleDebugAndroidTest)
# -d: an earlier run may have left the higher-versioned update build installed.
"$adb" install -r -t -d "$android/app/build/outputs/apk/debug/app-debug.apk" >/dev/null
"$adb" install -r -t -d "$android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk" >/dev/null
"$adb" shell pm clear io.lcl.workspace >/dev/null
log "installed; app data cleared"

serve
"$remote" identity --json >"$out/identity.json"
"$remote" pair --address "$address" --json >"$out/pair.json"
link=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["link"])' "$out/pair.json")
fingerprint=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["fingerprint"])' "$out/pair.json")

# 1. Pair, edit, save, check, inspect, run with approvals.
phase p1_pair_and_work -e link "'$link'" -e fp "$fingerprint" -e workdir "$todo"
check "the phone's edit is on disk" grep -q 'to-do list (phone)"' "$project/todo.lcl"
check "the run created todo_backup.txt" test -f "$todo/todo_backup.txt"
check "the run appended to todo.txt" grep -q 'Learn LCL' "$todo/todo.txt"
"$remote" devices --json >"$out/devices-paired.json"
device=$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1]))["devices"]; assert len(d)==1; print(d[0]["id"])' "$out/devices-paired.json")
log "PC lists device $device"
# The one-time code is spent, and the PC never stored the code itself.
check "the pairing code is consumed and was never stored" python3 - "$XDG_STATE_HOME/lcl/remote/pairing.json" "$link" <<'PY'
import json, sys, urllib.parse
path, link = sys.argv[1], sys.argv[2]
code = urllib.parse.parse_qs(urllib.parse.urlsplit(link).query)["c"][0]
text = open(path).read()
assert code not in text, "the one-time code is stored in plain text"
challenges = json.loads(text)["challenges"]
assert challenges and all(c["consumed_at"] for c in challenges), challenges
PY

# The PC's QR code is one a phone can read: ZXing, which the app's scanner is
# built on, reads exactly the pairing link out of the SVG the PC drew.
java=${JAVA_HOME:+$JAVA_HOME/bin/}java
zxing=$(find "${GRADLE_USER_HOME:-$HOME/.gradle}/caches" -name 'core-3.5.4.jar' | head -1)
python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["svg"], end="")' "$out/pair.json" >"$out/pair.svg"
check "the pairing QR code reads back as exactly the pairing link" \
    test "$("$java" -cp "$zxing" "$android/tools/QrDecode.java" "$out/pair.svg")" = "$link"

# An update, as a person installs one: a newer build signed with the same key,
# installed over the paired app. Phase 2 then proves the pairing and the key
# survived it.
(cd "$android" && ./gradlew --console=plain -q assembleDebug -PlclVersionCode=2 -PlclVersionName=0.1.0-update)
"$adb" install -r "$android/app/build/outputs/apk/debug/app-debug.apk" >/dev/null
check "a same-signed update (versionCode 2) installed over the paired app" \
    sh -c "'$adb' shell dumpsys package io.lcl.workspace | grep -q 'versionCode=2 '"

# The specification identities the PC's engine must report (task authority).
core01=00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed
core02=00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604

# 2. The app is closed and opened again: it reconnects with no QR code.
"$adb" shell am force-stop io.lcl.workspace
phase p2_restart_reconnects_then_disconnect -e core01 $core01 -e core02 $core02 -e settings set

# 3. The phone restarts.
if [ "${E2E_REBOOT:-1}" = 1 ]; then
    log "rebooting the device"
    "$adb" reboot
    "$adb" wait-for-device
    until [ "$("$adb" shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do sleep 2; done
    sleep 5
    wake
    phase p2_restart_reconnects_then_disconnect -e core01 $core01 -e core02 $core02 -e settings verify
fi

# 4. The network goes away and comes back.
phase p3_network_loss_and_return

# 5. The PC's service stops and starts again.
phase p4_pc_restart &
waiting=$!
wait_log READY_FOR_PC_RESTART 90
sleep 3
log "stopping the service"
stop_serve
sleep 12
log "starting the service again"
serve
wait "$waiting"

# 6. Edits on the PC reach the phone; a conflicting edit stops the phone's save.
phase p5_pc_edits_and_conflict &
waiting=$!
wait_log READY_FOR_PC_EDIT 90
sed -i 's/to-do list (phone)"/to-do list (phone, pc)"/' "$project/todo.lcl"
log "PC: edited todo.lcl while the phone had it open"
wait_log READY_FOR_CONFLICT 90
sed -i 's/to-do list (phone, pc)"/to-do list (phone, pc, pc2)"/' "$project/todo.lcl"
log "PC: edited todo.lcl again while the phone had unsaved edits"
wait "$waiting"
check "the phone's text was saved only after the person chose Keep mine" grep -q 'to-do list (phone, pc) \[phone2\]"' "$project/todo.lcl"
check "nothing was merged silently" absent 'pc2' "$project/todo.lcl"

# 7. The PC revokes the device.
phase p6_revoked &
waiting=$!
wait_log READY_FOR_REVOKE 90
sleep 2
"$remote" revoke "$device" | tee -a "$out/summary.txt"
wait "$waiting"
"$remote" devices --json >"$out/devices-revoked.json"

# 8. The spent QR code pairs nothing; a new one pairs again; then Forget.
"$remote" pair --address "$address" --json >"$out/pair-again.json"
again=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["link"])' "$out/pair-again.json")
phase p7_repair_then_forget -e oldlink "'$link'" -e link "'$again'"
"$remote" devices --json >"$out/devices-final.json"
check "the revoked device stays revoked; the new pairing is a new device" python3 - "$out/devices-final.json" "$device" <<'PY2'
import json, sys
devices = {d["id"]: d for d in json.load(open(sys.argv[1]))["devices"]}
old = devices[sys.argv[2]]
assert old["revoked_at"], old
assert len(devices) == 2, devices
PY2

no_lcl_failures
"$adb" shell settings put global hide_error_dialogs 0
log "all phases passed; results in $out"
