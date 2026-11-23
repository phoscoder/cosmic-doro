#!/usr/bin/env bash
# Regenerates Cosmic Doro's bundled tones with ffmpeg (sine-based bells,
# peak-normalized and gently compressed so they're clearly audible).
set -euo pipefail
cd "$(dirname "$0")"

# note FREQ START DECAY -> bell-like partial stack starting at START seconds
note() {
  local f=$1 t0=$2 k=$3
  local x="(t-$t0)"
  echo "gte(t,$t0)*exp(-$k*$x)*(1-exp(-300*$x))*(sin(2*PI*$f*$x)+0.35*sin(2*PI*2*$f*$x)+0.12*sin(2*PI*3*$f*$x))"
}

make() {
  local name=$1 dur=$2 expr=$3
  ffmpeg -hide_banner -loglevel error -y -f lavfi \
    -i "aevalsrc='$expr':s=48000:d=$dur" \
    -af "acompressor=threshold=-18dB:ratio=3:attack=2:release=80,afade=t=out:st=$(awk "BEGIN{print $dur-0.08}"):d=0.08" \
    -ac 1 /tmp/doro-$name.wav
  local peak
  peak=$(ffmpeg -hide_banner -i /tmp/doro-$name.wav -af volumedetect -f null - 2>&1 | grep -oP 'max_volume: \K-?[0-9.]+')
  ffmpeg -hide_banner -loglevel error -y -i /tmp/doro-$name.wav \
    -af "volume=$(awk "BEGIN{print -0.5-($peak)}")dB" -c:a libvorbis -q:a 6 "$name.oga"
  rm -f /tmp/doro-$name.wav
}

make start      0.45 "$(note 1318.5 0 12)+$(note 1975.5 0.11 12)"
make pause      0.45 "$(note 987.8 0 12)+$(note 659.3 0.11 12)"
make focus-end  1.70 "$(note 1046.5 0 5)+$(note 1318.5 0.16 5)+$(note 1568 0.32 5)+$(note 2093 0.48 4)"
make break-end  1.60 "$(note 880 0 4)+0.6*$(note 1760 0 6)+$(note 880 0.5 4)+0.6*$(note 1760 0.5 6)"
