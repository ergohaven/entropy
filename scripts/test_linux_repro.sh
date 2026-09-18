#!/usr/bin/env bash
# Собирает Linux-артефакты дважды из одного дерева и сравнивает суммы.
# Утверждение «сборка воспроизводима» иначе ничем не подкреплено: и nfpm, и
# mksquashfs по умолчанию пишут в артефакт время сборки, и разъезжаются они
# молча.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TASK="${TASK:-task}"
command -v "$TASK" >/dev/null 2>&1 || {
	echo "go-task is required: scripts/prepare_env.sh or https://taskfile.dev" >&2
	exit 1
}

# Обе сборки должны видеть одну отметку времени, даже если дерево грязное и
# `git log` в них теоретически мог бы отличаться.
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --pretty=%ct 2>/dev/null || echo 315532800)}"
export SOURCE_DATE_EPOCH

rm -rf target/repro
# DIST через окружение, а не как `task ... DIST=`: у go-task уже выставленная
# переменная окружения перебивает и var из командной строки, а этот скрипт сам
# запущен из задачи, которая DIST в окружение уже положила.
DIST=target/repro/a "$TASK" linux:all
DIST=target/repro/b "$TASK" linux:all

failures=0
count=0
for first in target/repro/a/linux/*; do
	name="$(basename "$first")"
	second="target/repro/b/linux/$name"
	count=$((count + 1))
	if [[ ! -f $second ]]; then
		echo "FAIL: $name is missing from the second build" >&2
		failures=$((failures + 1))
		continue
	fi
	a="$(sha256sum "$first" | cut -d' ' -f1)"
	b="$(sha256sum "$second" | cut -d' ' -f1)"
	if [[ $a == "$b" ]]; then
		echo "ok: $name $a"
	else
		echo "FAIL: $name differs between builds ($a != $b)" >&2
		failures=$((failures + 1))
	fi
done

((count > 0)) || {
	echo "no artifacts were built" >&2
	exit 1
}
((failures == 0)) || {
	echo "$failures artifact(s) are not reproducible" >&2
	exit 1
}
echo "Reproducible: $count artifact(s) identical across two builds"
