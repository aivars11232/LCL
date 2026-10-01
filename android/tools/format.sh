#!/bin/sh
# Format the app's Kotlin sources the one way they are kept: ktfmt, Kotlin
# language guidelines style (4 spaces, 100 columns, unused imports removed).
#
#   android/tools/format.sh            format in place
#   android/tools/format.sh --check    exit 1 if a file is not formatted
#
# KTFMT_JAR names the formatter: ktfmt 0.64 with dependencies
# (com.facebook:ktfmt:0.64:with-dependencies, SHA-256
# 5b3d5286fd2defcc7dc8e28c21ddf156cc6b2d8682bdcd929ce4333e7a6201f2). It is a
# developer tool, not a build dependency, and is not kept in the repository.
set -eu
jar=${KTFMT_JAR:?set KTFMT_JAR to ktfmt-0.64-with-dependencies.jar}
java=${JAVA_HOME:+$JAVA_HOME/bin/}java
cd "$(dirname "$0")/.."
check=
[ "${1:-}" = --check ] && check="--dry-run --set-exit-if-changed"
# shellcheck disable=SC2086
git ls-files '*.kt' '*.kts' | xargs "$java" -jar "$jar" --kotlinlang-style $check
