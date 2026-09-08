#!/usr/bin/env bash
# Тулчейн у разработчика (.tool-versions через asdf) и в образе сборки должен
# быть один: артефакты публикуются из контейнера, а воспроизвести их локально
# на другом компиляторе нельзя. Расхождение иначе замечается только по разным
# суммам одного и того же тега.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

failures=0
fail() {
	echo "FAIL: $*" >&2
	failures=$((failures + 1))
}

asdf_rust="$(awk '$1 == "rust" { print $2; exit }' "$ROOT/.tool-versions")"
image_rust="$(sed -n 's/^ARG RUST_IMAGE=rust:\([0-9][0-9.]*\)-.*/\1/p' "$ROOT/Dockerfile")"

[[ -n $asdf_rust ]] || fail ".tool-versions has no rust entry"
[[ -n $image_rust ]] || fail "Dockerfile has no ARG RUST_IMAGE=rust:<version>-... line"

# В теге образа патч-версии обычно нет (rust:1.97-bookworm), поэтому сравниваем
# по тому, что в нём указано.
if [[ -n $asdf_rust && -n $image_rust && $asdf_rust != "$image_rust"* ]]; then
	fail "rust is $asdf_rust in .tool-versions but $image_rust in the Dockerfile"
fi

# То же для go-task: CI ставит его экшеном, и «3.x» там означало бы другой Task,
# чем в образе.
# shellcheck disable=SC2016  # это литерал sed-скрипта, а не подстановка шелла
task_pin="$(sed -n 's/^TASK_VERSION="\${TASK_VERSION:-\([0-9.]*\)}"$/\1/p' "$ROOT/scripts/tool_pins.sh")"
[[ -n $task_pin ]] || fail "scripts/tool_pins.sh has no TASK_VERSION pin"
for workflow in "$ROOT"/.github/workflows/*.yml; do
	grep -q 'arduino/setup-task' "$workflow" || continue
	while read -r version; do
		[[ $version == "$task_pin" ]] ||
			fail "$(basename "$workflow"): setup-task version '$version', want the pinned $task_pin"
	done < <(sed -n "s/^ *version: *['\"]\{0,1\}\([0-9][0-9.x]*\)['\"]\{0,1\} *$/\1/p" "$workflow")
done

((failures == 0)) || {
	echo "$failures toolchain pin check(s) failed" >&2
	exit 1
}
echo "Toolchain pins are consistent"
