#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

readonly TARGET_HOST=trevor@192.168.0.61
readonly TARGET_PATH=/home/trevor/rpi_cnc_remote
readonly TARGET_ARCH=armv7-unknown-linux-musleabihf
readonly SOURCE_PATH=./target/${TARGET_ARCH}/release/rpi_cnc_remote

cross build --release --target=${TARGET_ARCH}
scp -O -r ${SOURCE_PATH} ${TARGET_HOST}:${TARGET_PATH}
ssh -t ${TARGET_HOST} ${TARGET_PATH}

