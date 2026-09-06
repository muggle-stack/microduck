#!/bin/sh
# Select the already-working Bianbu ES8326; no mixer, kernel, DT or service changes.
set -eu
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != riscv64 ]; then
    echo "Run on the K1/Bianbu board, not the Radxa or build host." >&2
    exit 1
fi
if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root to install the opt-in ALSA PCM." >&2
    exit 1
fi
amixer -c sndes8326 info >/dev/null

pcm=/etc/alsa/conf.d/99-microduck-es8326.conf
params=/etc/robot/robotd.toml
# A local tuning belongs to the operator. Do not silently replace it on rerun.
if [ -e "$pcm" ] || [ -L "$pcm" ]; then
    if ! cmp -s deploy/audio/es8326.conf "$pcm"; then
        echo "$pcm already exists and differs; review/merge it manually." >&2
        exit 1
    fi
else
    install -D -m 644 deploy/audio/es8326.conf "$pcm"
fi

if [ ! -e "$params" ] && [ ! -L "$params" ]; then
    install -D -m 644 deploy/k1/robotd-audio.toml "$params"
    echo "Created $params with the audio-only K1 profile."
else
    echo "Preserved $params; set audio.device = \"microduck_es8326\" if not already selected."
fi
echo "Installed $pcm; existing ES8326 mixer settings and default PCM are unchanged."
echo "No daemon was started/restarted. Existing robotd needs a restart to reread its config."
