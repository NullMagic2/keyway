"""
Module: keyway.tools.generate_layout_labels
Purpose: Regenerate data/layout_labels.json and data/flags/ from XKB, iso-codes and flag-icons.
Created: 2026-11-09
Architecture: Development-time only. Usage: generate_layout_labels.py FLAG_ICONS_PACKAGE_DIR,
the unpacked flag-icons npm package. keyway-core embeds the table and the extensions read it.
"""

import json
import shutil
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import Path

PROJECT_DIR = Path(__file__).resolve().parent.parent
LABELS_PATH = PROJECT_DIR / "data" / "layout_labels.json"
FLAGS_DIR = PROJECT_DIR / "data" / "flags"
XKB_RULES_PATH = Path("/usr/share/X11/xkb/rules/evdev.xml")
LANGUAGE_CODES_PATH = Path("/usr/share/iso-codes/json/iso_639-2.json")
FLAG_SHAPE = "4x3"
VARIANT_SEPARATOR = "+"

# ISO 639-2 codes for "undetermined", "multiple" and the like, which name no language.
NOT_LANGUAGES = {"und", "mul", "mis", "zxx"}

# For a layout used in several countries: its language's home country, or a flag its speakers
# share. A variant without an entry takes its layout's.
SHARED_FLAGS = {"latam": "es", "ara": "arab", "gb+gla": "gb-sct"}


def two_letter_languages():
    """Map ISO 639-2 codes, both bibliographic and terminology forms, to ISO 639-1 where one exists."""
    entries = json.loads(LANGUAGE_CODES_PATH.read_text(encoding="utf-8"))["639-2"]
    mapping = {}

    for entry in entries:
        short_code = entry.get("alpha_2")

        if short_code:
            mapping[entry["alpha_3"]] = short_code
            mapping[entry.get("bibliographic", entry["alpha_3"])] = short_code

    return mapping


def codes(config_item, list_name, code_name):
    found = config_item.find(list_name)

    return [] if found is None else [code.text for code in found.iter(code_name)]


def layout_id(name, variant):
    return f"{name}{VARIANT_SEPARATOR}{variant}" if variant else name


def label(name, variant, languages, countries, short_languages):
    """Build one entry: the language-country code in capitals, and the flag. A layout used in
    several countries gets no country, and a flag only from SHARED_FLAGS."""
    languages = [code for code in languages if code not in NOT_LANGUAGES]
    language = short_languages.get(languages[0], languages[0]) if languages else name
    country = countries[0] if len(countries) == 1 else None
    entry = {"code": language.upper() + (f"-{country.upper()}" if country else "")}
    shared_flag = SHARED_FLAGS.get(layout_id(name, variant), SHARED_FLAGS.get(name))
    flag = country.lower() if country else shared_flag

    if flag:
        entry["flag"] = flag

    return entry


def build_labels(short_languages):
    root = ElementTree.parse(XKB_RULES_PATH).getroot()
    labels = {}

    for layout in root.find("layoutList").findall("layout"):
        item = layout.find("configItem")
        name = item.findtext("name")
        languages = codes(item, "languageList", "iso639Id")
        countries = codes(item, "countryList", "iso3166Id")
        labels[name] = label(name, None, languages, countries, short_languages)

        variants = layout.find("variantList")

        for variant in [] if variants is None else variants.findall("variant"):
            variant_item = variant.find("configItem")
            variant_name = variant_item.findtext("name")
            labels[layout_id(name, variant_name)] = label(
                name,
                variant_name,
                codes(variant_item, "languageList", "iso639Id") or languages,
                codes(variant_item, "countryList", "iso3166Id") or countries,
                short_languages,
            )

    return labels


def copy_flags(package_dir, flags):
    """Copy only the flags a layout uses, replacing any from an earlier run."""
    shutil.rmtree(FLAGS_DIR, ignore_errors=True)
    FLAGS_DIR.mkdir(parents=True)

    for flag in sorted(flags):
        shutil.copyfile(package_dir / "flags" / FLAG_SHAPE / f"{flag}.svg", FLAGS_DIR / f"{flag}.svg")

    shutil.copyfile(package_dir / "LICENSE", FLAGS_DIR / "LICENSE")


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: generate_layout_labels.py FLAG_ICONS_PACKAGE_DIR")

    package_dir = Path(sys.argv[1])
    flag_icons = json.loads((package_dir / "package.json").read_text(encoding="utf-8"))
    labels = build_labels(two_letter_languages())

    copy_flags(package_dir, {entry["flag"] for entry in labels.values() if "flag" in entry})

    # One layout per line, so the table reads and diffs as a list.
    layout_lines = ",\n".join(f"  {json.dumps(name)}: {json.dumps(entry, ensure_ascii=False)}"
                              for name, entry in sorted(labels.items()))
    flags_note = f"flag-icons {flag_icons['version']}, {flag_icons['license']}, in flags/"
    document = (f'{{\n "generator": "tools/generate_layout_labels.py",\n'
                f' "flags": {json.dumps(flags_note)},\n "layouts": {{\n{layout_lines}\n }}\n}}\n')

    LABELS_PATH.write_text(document, encoding="utf-8")


if __name__ == "__main__":
    main()
