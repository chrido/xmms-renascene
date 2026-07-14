#!/bin/sh

test_status=0
XMMS_E2E_ANDROID_SKIP_BUILD=1 ./repo pye2e -m android || test_status=$?

mkdir -p testoutput/android-ci || true
adb logcat -d > testoutput/android-ci/logcat.txt 2>&1 || true

exit "$test_status"
