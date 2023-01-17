#!/bin/bash

HIGH=/usr/share/sounds/freedesktop/stereo/bell.oga
LOW=/usr/share/sounds/freedesktop/stereo/dialog-warning.oga

cat /dev/ttyUSB0 | while read a; do
  echo $a
  echo $a | grep -s High && mplayer $HIGH || mplayer $LOW
done
