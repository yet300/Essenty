#!/usr/bin/env bash
set -euo pipefail

: "${ANDROID_HOME:?Set ANDROID_HOME to an installed Android SDK}"
bridge_root="$(cd "$(dirname "$0")" && pwd)"
gradle_bin="${GRADLE_BIN:-gradle}"
"$gradle_bin" --project-dir "$bridge_root" \
    :bridge:assembleDebug :packaging-test-app:assembleDebug

apk="$bridge_root/packaging-test-app/build/outputs/apk/debug/packaging-test-app-debug.apk"
dexdump="$ANDROID_HOME/build-tools/35.0.0/dexdump"
test -f "$apk"
test -x "$dexdump"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
: > "$scratch/descriptors"
while IFS= read -r entry; do
    case "$entry" in
        classes*.dex)
            unzip -p "$apk" "$entry" > "$scratch/$entry"
            "$dexdump" -l plain "$scratch/$entry" \
                | grep 'Class descriptor' >> "$scratch/descriptors"
            ;;
    esac
done < <(unzip -Z1 "$apk")

for class in \
    'Landroidx/activity/OnBackPressedCallback;' \
    'Landroidx/activity/OnBackPressedDispatcher;' \
    'Landroidx/activity/BackEventCompat;' \
    'Landroidx/lifecycle/Lifecycle;' \
    'Landroidx/lifecycle/LifecycleOwner;' \
    'Landroidx/lifecycle/ViewModel;' \
    'Landroidx/lifecycle/ViewModelStore;' \
    'Landroidx/savedstate/SavedStateRegistry;'
do
    if ! grep -Fq "Class descriptor  : '$class'" "$scratch/descriptors"; then
        echo "Missing dex class: $class" >&2
        exit 1
    fi
done

echo 'AndroidX packaging verified: required classes are defined in the APK dex files.'
