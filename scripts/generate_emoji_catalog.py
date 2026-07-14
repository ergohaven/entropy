#!/usr/bin/env python3
"""Generate Entropy's Unicode emoji picker catalog."""

from __future__ import annotations

import argparse
import io
import re
import urllib.request
import zipfile
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


EMOJI_TEST_URL = "https://unicode.org/Public/emoji/latest/emoji-test.txt"
CLDR_CORE_URL = "https://unicode.org/Public/cldr/latest/core.zip"
SKIN_TONE_RANGE = range(0x1F3FB, 0x1F400)


SECTION_VARIANTS = {
    "Smileys & Emotion": "SmileysAndEmotion",
    "People & Body": "PeopleAndBody",
    "Animals & Nature": "AnimalsAndNature",
    "Food & Drink": "FoodAndDrink",
    "Travel & Places": "TravelAndPlaces",
    "Activities": "Activities",
    "Objects": "Objects",
    "Symbols": "Symbols",
    "Flags": "Flags",
}


STOP_WORDS = {
    "a",
    "an",
    "and",
    "button",
    "of",
    "the",
    "with",
}


@dataclass(frozen=True)
class EmojiRow:
    emoji: str
    name: str
    section: str
    group: str
    subgroup: str
    supports_skin_tone: bool


@dataclass(frozen=True)
class Annotation:
    name: str | None
    keywords: tuple[str, ...]


def read_text(path: str | None, url: str) -> str:
    if path:
        return Path(path).read_text(encoding="utf-8")
    with urllib.request.urlopen(url, timeout=30) as response:
        return response.read().decode("utf-8")


def read_cldr_annotations(path: str | None) -> str:
    if path:
        return Path(path).read_text(encoding="utf-8")
    with urllib.request.urlopen(CLDR_CORE_URL, timeout=60) as response:
        data = response.read()
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        return archive.read("common/annotations/en.xml").decode("utf-8")


def codepoints_to_emoji(codepoints: str) -> str:
    return "".join(chr(int(part, 16)) for part in codepoints.split())


def contains_skin_tone(emoji: str) -> bool:
    return any(ord(char) in SKIN_TONE_RANGE for char in emoji)


def rust_string(value: str) -> str:
    return (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
    )


def rust_ident(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "_", value).strip("_").lower()


def generated_keyword_candidates(name: str, group: str, subgroup: str) -> list[str]:
    raw = " ".join([name, group, subgroup.replace("-", " ")])
    words = re.findall(r"[A-Za-z0-9]+", raw.lower())
    return [word for word in words if len(word) > 1 and word not in STOP_WORDS]


def parse_annotations(xml: str) -> dict[str, Annotation]:
    root = ET.fromstring(xml)
    collected: dict[str, dict[str, object]] = {}
    for element in root.findall(".//annotation"):
        emoji = element.attrib.get("cp")
        if not emoji:
            continue
        item = collected.setdefault(emoji, {"name": None, "keywords": set()})
        text = (element.text or "").strip()
        if element.attrib.get("type") == "tts":
            item["name"] = text or None
        elif text:
            keywords = item["keywords"]
            assert isinstance(keywords, set)
            for keyword in text.split("|"):
                keyword = keyword.strip().lower()
                if keyword:
                    keywords.add(keyword)

    annotations = {}
    for emoji, item in collected.items():
        keywords = item["keywords"]
        assert isinstance(keywords, set)
        name = item["name"]
        assert name is None or isinstance(name, str)
        annotations[emoji] = Annotation(name=name, keywords=tuple(sorted(keywords)))
    return annotations


def parse_emoji_test(data: str) -> list[EmojiRow]:
    base_rows: list[tuple[str, str, str, str]] = []
    tone_variants: set[str] = set()
    group = ""
    subgroup = ""

    for raw_line in data.splitlines():
        line = raw_line.strip()
        if line.startswith("# group: "):
            group = line.removeprefix("# group: ")
            continue
        if line.startswith("# subgroup: "):
            subgroup = line.removeprefix("# subgroup: ")
            continue
        if "; fully-qualified" not in line or "#" not in line:
            continue
        if group == "Component" or group not in SECTION_VARIANTS:
            continue

        codepoints = line.split(";", 1)[0].strip()
        emoji = codepoints_to_emoji(codepoints)
        name = line.split("#", 1)[1].strip()
        name = re.sub(r"^\\S+\\s+E[0-9.]+\\s+", "", name)
        if contains_skin_tone(emoji):
            tone_variants.add(emoji)
            continue
        base_rows.append((emoji, name, group, subgroup))

    rows = []
    for emoji, name, group, subgroup in base_rows:
        supports_skin_tone = any(
            f"{emoji}{chr(tone)}" in tone_variants for tone in SKIN_TONE_RANGE
        )
        rows.append(
            EmojiRow(
                emoji=emoji,
                name=name,
                section=SECTION_VARIANTS[group],
                group=group,
                subgroup=subgroup,
                supports_skin_tone=supports_skin_tone,
            )
        )
    return rows


def write_catalog(rows: list[EmojiRow], annotations: dict[str, Annotation], output: Path) -> None:
    lines = [
        "// @generated by scripts/generate_emoji_catalog.py",
        "// Source data: https://unicode.org/Public/emoji/latest/emoji-test.txt",
        "// Optional keywords: https://unicode.org/Public/cldr/latest/core.zip common/annotations/en.xml",
        "use super::{EmojiEntry, EmojiSection};",
        "",
        "#[rustfmt::skip]",
        "pub const EMOJI_CATALOG: &[EmojiEntry] = &[",
    ]

    for row in rows:
        annotation = annotations.get(row.emoji) or annotations.get(row.emoji.replace("\ufe0f", ""))
        name = annotation.name if annotation and annotation.name else row.name
        keywords = set(generated_keyword_candidates(name, row.group, row.subgroup))
        if annotation:
            keywords.update(annotation.keywords)
        keywords.discard(name.lower())
        keyword_list = ", ".join(f'"{rust_string(keyword)}"' for keyword in sorted(keywords))
        lines.extend(
            [
                "    EmojiEntry {",
                f'        emoji: "{rust_string(row.emoji)}",',
                f'        name: "{rust_string(name)}",',
                f"        section: EmojiSection::{row.section},",
                f'        subgroup: "{rust_string(row.subgroup)}",',
                f"        keywords: &[{keyword_list}],",
                f"        supports_skin_tone: {str(row.supports_skin_tone).lower()},",
                "    },",
            ]
        )

    lines.extend(["];", ""])
    output.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--emoji-test", help="Path to emoji-test.txt")
    parser.add_argument("--annotations", help="Path to CLDR common/annotations/en.xml")
    parser.add_argument(
        "--output",
        default="src/emoji_catalog_data.rs",
        help="Generated Rust output path",
    )
    args = parser.parse_args()

    emoji_test = read_text(args.emoji_test, EMOJI_TEST_URL)
    annotations = parse_annotations(read_cldr_annotations(args.annotations))
    rows = parse_emoji_test(emoji_test)
    write_catalog(rows, annotations, Path(args.output))


if __name__ == "__main__":
    main()
