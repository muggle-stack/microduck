#!/bin/sh
# Bounded ES8326 format/full-duplex check. Silent playback; captures are discarded.
# Run separately from the software-only k1-test.sh: this opens the real microphone.
set -eu

device="${1:-microduck_es8326}"
card=$(readlink /proc/asound/sndes8326)
case "$card" in
    card[0-9]*) ;;
    *) echo "ES8326 ALSA card is not registered." >&2; exit 1 ;;
esac
playback=/proc/asound/$card/pcm0p/sub0
capture=/proc/asound/$card/pcm0c/sub0
for stream in "$playback" "$capture"; do
    if ! grep -qx closed "$stream/status"; then
        echo "$stream is busy; stop its owner before testing." >&2
        exit 1
    fi
done

audio_tmp=$(mktemp -d /tmp/microduck-es8326.XXXXXX)
capture_pid=
playback_pid=
cleanup() {
    # These are only our timeout children; timeout forwards TERM to its audio process.
    for pid in "$capture_pid" "$playback_pid"; do
        if [ -n "$pid" ]; then
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    rm -f "$audio_tmp/capture.raw" "$audio_tmp/capture.log" "$audio_tmp/playback.log"
    rmdir "$audio_tmp"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

start_capture() {
    timeout -k 2 12 arecord -q -D "$device" -f S16_LE -r 16000 -c 1 -t raw -d 6 \
        "$audio_tmp/capture.raw" 2>"$audio_tmp/capture.log" &
    capture_pid=$!
}
start_playback() {
    # The same low-latency buffer/period requests as robotd's live synth.
    timeout -k 2 12 aplay -q -D "$device" -f S16_LE -r 48000 -c 1 -t raw -d 6 \
        --buffer-time=40000 --period-time=10000 /dev/zero 2>"$audio_tmp/playback.log" &
    playback_pid=$!
}
check_hardware() {
    for stream in "$playback" "$capture"; do
        cat "$stream/hw_params" || return 1
        grep -q '^state: RUNNING' "$stream/status" || return 1
        grep -qx 'format: S16_LE' "$stream/hw_params" || return 1
        grep -qx 'channels: 2' "$stream/hw_params" || return 1
        grep -q '^rate: 48000 ' "$stream/hw_params" || return 1
    done
}

for order in capture-first playback-first; do
    echo "$order: $device, capture 16 kHz mono / playback 48 kHz mono"
    if [ "$order" = capture-first ]; then
        start_capture
        sleep 0.5
        start_playback
    else
        start_playback
        sleep 0.5
        start_capture
    fi
    sleep 0.5
    verdict=0
    check_hardware || verdict=1
    wait "$capture_pid" || verdict=1
    capture_pid=
    wait "$playback_pid" || verdict=1
    playback_pid=
    # ALSA can return success after an xrun: do not turn that into a clean pass.
    for log in "$audio_tmp/capture.log" "$audio_tmp/playback.log"; do
        if [ -s "$log" ]; then
            cat "$log" >&2
            verdict=1
        fi
    done
    bytes=0
    if [ -f "$audio_tmp/capture.raw" ]; then
        bytes=$(wc -c < "$audio_tmp/capture.raw")
    fi
    [ "$bytes" -eq 192000 ] || verdict=1
    [ "$verdict" -eq 0 ] || { echo "$order: FAIL ($bytes capture bytes)" >&2; exit 1; }
    echo "$order: PASS, 96000 captured mono samples, both hardware streams 48 kHz stereo"
done
echo "ES8326 format/full-duplex check passed; no acoustic quality or classifier-accuracy claim."
