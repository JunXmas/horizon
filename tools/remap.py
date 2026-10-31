"""Obfuscated releases (1.21.11 and older) to Mojang's names, so the tools that read 26.x jars read them too.

Mojang publishes its own names for every obfuscated release (`client_mappings`, `server_mappings` in the version
JSON, ProGuard format). `proguard_to_tiny` turns one into tiny v1 (official -> named) and `remap` runs the pinned
tiny-remapper (research-lock.json `tiny_remapper`) over a jar. The remapped jar never leaves `.local/` or the
version store, like the jar it comes from.
"""

from pathlib import Path
import re
import subprocess

try:
    from . import lab
except ImportError:
    import lab

PRIMITIVES = {"void": "V", "boolean": "Z", "byte": "B", "char": "C", "short": "S", "int": "I", "long": "J",
              "float": "F", "double": "D"}
CLASS_LINE = re.compile(r"^(\S+) -> (\S+):$")
MEMBER_LINE = re.compile(r"^\s+(?:\d+:\d+:)?(\S+) ([^\s(]+)(?:\(([^)]*)\))?(?::\d+:\d+)? -> (\S+)$")


def descriptor(java_type, classes):
    """`net.minecraft.Foo[]` -> `[La;` in the obfuscated namespace (`classes`: named -> official)."""
    dims = java_type.count("[]")
    base = java_type.replace("[]", "")
    if base in PRIMITIVES:
        return "[" * dims + PRIMITIVES[base]
    return "[" * dims + "L" + classes.get(base, base).replace(".", "/") + ";"


def proguard_to_tiny(text):
    """Mojang's ProGuard mappings as tiny v1 text, official -> named."""
    lines = [line for line in text.splitlines() if line and not line.startswith("#")]
    classes = {m[1]: m[2] for line in lines if (m := CLASS_LINE.match(line))}
    out = ["v1\tofficial\tnamed"]
    owner = None
    seen = set()
    for line in lines:
        if m := CLASS_LINE.match(line):
            owner = m[2].replace(".", "/")
            out.append(f"CLASS\t{owner}\t{m[1].replace('.', '/')}")
            continue
        m = MEMBER_LINE.match(line)
        if not m or owner is None:
            raise ValueError(f"Unreadable mapping line: {line!r}")
        java_type, name, args, obf = m.groups()
        if "." in name:  # an inlined frame of another class's method, not a member of this one
            continue
        if args is None:
            row = ("FIELD", owner, descriptor(java_type, classes), obf, name)
        else:
            params = "".join(descriptor(a, classes) for a in args.split(",") if a)
            row = ("METHOD", owner, f"({params}){descriptor(java_type, classes)}", obf, name)
        if row not in seen:  # line-number ranges repeat a method once per range
            seen.add(row)
            out.append("\t".join(row))
    return "\n".join(out) + "\n"


def tool():
    """The pinned tiny-remapper, downloaded once into .local/tools and checked."""
    pin = lab.json.loads((lab.ROOT / "research-lock.json").read_text())["tiny_remapper"]
    return lab.download(pin["url"], lab.LOCAL / "tools" / f"tiny-remapper-{pin['version']}-fat.jar",
                        pin["sha256"], "sha256")


def remap(java, jar, mappings_txt, destination, libraries=()):
    """`jar` (official names) to `destination` (Mojang's names). Libraries only resolve inheritance; the
    output is the same without them for Minecraft's own jars, so a missing one is not an error."""
    tiny = Path(destination).with_suffix(".tiny")
    tiny.write_text(proguard_to_tiny(Path(mappings_txt).read_text()))
    partial = Path(destination).with_suffix(".partial.jar")
    partial.unlink(missing_ok=True)
    result = subprocess.run([java, "-jar", str(tool()), str(jar), str(partial), str(tiny), "official", "named",
                             *map(str, libraries)], capture_output=True, text=True)
    if result.returncode or not partial.exists():
        raise SystemExit(f"tiny-remapper failed on {jar}:\n{(result.stderr or result.stdout)[-3000:]}")
    partial.replace(destination)
    return Path(destination)

