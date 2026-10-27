"""Plugin/action registry engine for SigmaOS-plugin-registry / actions /
Github-Action (inspired by omarchy-plugin-registry, cinnamon-spices-actions,
github-actions).

Principles: the manifest schema is the contract (single source of truth),
validate-manifest is a real validator with an explicit error list, the
index is derived data that must stay consistent (duplicate ids fail),
registration is append-only and the only side effect.
"""
import argparse, json, os, re, sys

ENGINE = {
    "name": "sigma-registry",
    "summary": "Register, validate and index plugin/action manifests",
    "language": "python",
    "verbs": ["register", "manifest", "index", "validate-manifest"],
    "principles": ["Single Source of Truth", "Fail-Fast",
                   "Defensive Programming", "Plan Before Act", "KISS", "DRY",
                   "Deterministic Testing"],
}

cli_doc = """\
$ sigma-registry manifest build-cache@sigmaos "Build cache" \\
    --version 1.0.0 --entry action.py --license GPL-3.0-or-later --out build-cache.manifest.json
$ sigma-registry validate-manifest build-cache.manifest.json
valid: build-cache@sigmaos 1.0.0
$ sigma-registry index ./registry
2 manifest(s) indexed
$ sigma-registry register ./registry build-cache.manifest.json
registered build-cache@sigmaos -> ./registry/index.json
"""

_ID_RE = re.compile(r"^[a-z0-9][a-z0-9-]*@[a-z0-9][a-z0-9-]*$")
_VERSION_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")
_LICENSE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9.+-]*$")
_TYPES = ("plugin", "action", "theme", "engine")
_REQUIRED = ("id", "name", "version", "entry", "license", "type")
_MANIFEST_SUFFIX = ".manifest.json"


class RegistryError(ValueError):
    pass


def _read(path):
    if path == "-":
        return sys.stdin.read()
    with open(path, "r", encoding="utf-8") as fh:
        return fh.read()


def validate_manifest(data):
    """Explicit schema validation; returns a list of error strings."""
    errors = []
    if not isinstance(data, dict):
        return ["manifest must be a JSON object"]
    for field in _REQUIRED:
        if not data.get(field) and data.get(field) != 0:
            errors.append(f"missing required field {field!r}")
    if "id" in data and data["id"] and not _ID_RE.match(str(data["id"])):
        errors.append(f"id must look like 'name@scope' (lowercase, dashes), "
                      f"got {data['id']!r}")
    if "version" in data and data["version"] and \
            not _VERSION_RE.match(str(data["version"])):
        errors.append(f"version must be semver, got {data['version']!r}")
    if "type" in data and data["type"] and data["type"] not in _TYPES:
        errors.append(f"type must be one of {_TYPES}, got {data['type']!r}")
    if "license" in data and data["license"] and \
            not _LICENSE_RE.match(str(data["license"])):
        errors.append(f"bad SPDX-ish license id {data['license']!r}")
    if "entry" in data and data["entry"]:
        entry = str(data["entry"])
        if os.path.isabs(entry) or ".." in entry.split("/"):
            errors.append(f"entry must be a relative path, got {entry!r}")
        elif not entry.endswith((".py", ".sh", ".yml", ".yaml")):
            errors.append(f"entry must be .py/.sh/.yml, got {entry!r}")
    for key in ("depends", "keywords"):
        if key in data and (not isinstance(data[key], list) or
                            not all(isinstance(x, str) for x in data[key])):
            errors.append(f"{key} must be a list of strings")
    return errors


def _require_valid(data):
    errors = validate_manifest(data)
    if errors:
        raise RegistryError("invalid manifest: " + "; ".join(errors))
    return data


def make_manifest(mid, name, version, entry, license_id,
                  ptype="plugin", description="", depends=None):
    """Build a manifest skeleton (still validated before returning)."""
    data = {"id": mid, "name": name, "version": version, "entry": entry,
            "license": license_id, "type": ptype, "description": description,
            "depends": list(depends or [])}
    return _require_valid(data)


def scan_dir(registry_dir):
    """Load and validate every *.manifest.json in a registry directory."""
    if not os.path.isdir(registry_dir):
        raise RegistryError(f"no such registry dir: {registry_dir!r}")
    manifests, seen = [], {}
    for fname in sorted(os.listdir(registry_dir)):
        if not fname.endswith(_MANIFEST_SUFFIX):
            continue
        path = os.path.join(registry_dir, fname)
        try:
            data = json.loads(_read(path))
        except json.JSONDecodeError as e:
            raise RegistryError(f"{fname}: bad JSON: {e}") from e
        errors = validate_manifest(data)
        if errors:
            raise RegistryError(f"{fname}: " + "; ".join(errors))
        mid = data["id"]
        if mid in seen:
            raise RegistryError(f"duplicate id {mid!r} in {fname} "
                                f"and {seen[mid]}")
        seen[mid] = fname
        manifests.append(data)
    return manifests


def build_index(registry_dir):
    """Derived index of the registry, sorted by id (deterministic)."""
    return sorted(scan_dir(registry_dir), key=lambda m: m["id"])


def _load_index(registry_dir):
    path = os.path.join(registry_dir, "index.json")
    if not os.path.exists(path):
        return []
    try:
        data = json.loads(_read(path))
    except json.JSONDecodeError as e:
        raise RegistryError(f"corrupt index.json: {e}") from e
    if not isinstance(data, list):
        raise RegistryError("corrupt index.json: expected a list")
    return data


def register(registry_dir, manifest_path):
    """Validate + append a manifest to the registry index (idempotent-fail)."""
    try:
        data = json.loads(_read(manifest_path))
    except json.JSONDecodeError as e:
        raise RegistryError(f"{manifest_path}: bad JSON: {e}") from e
    _require_valid(data)
    index = _load_index(registry_dir)
    existing = {m.get("id") for m in index}
    if data["id"] in existing:
        raise RegistryError(f"already registered: {data['id']}")
    index.append({k: data[k] for k in sorted(data)})
    index.sort(key=lambda m: m["id"])
    os.makedirs(registry_dir, exist_ok=True)
    out = os.path.join(registry_dir, "index.json")
    tmp = out + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(index, fh, indent=2, sort_keys=True)
    os.replace(tmp, out)
    return data["id"], out


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog=ENGINE["name"],
                                description=ENGINE["summary"])
    sub = p.add_subparsers(dest="cmd", required=True)
    mf = sub.add_parser("manifest", help="generate a validated skeleton")
    mf.add_argument("id")
    mf.add_argument("name")
    mf.add_argument("--version", required=True)
    mf.add_argument("--entry", default="main.py")
    mf.add_argument("--license", required=True, dest="license_id")
    mf.add_argument("--type", choices=list(_TYPES), default="plugin",
                    dest="ptype")
    mf.add_argument("--description", default="")
    mf.add_argument("--depends", nargs="*", default=[])
    mf.add_argument("--out", default=None)
    vm = sub.add_parser("validate-manifest", help="validate a manifest file")
    vm.add_argument("manifest_file")
    ix = sub.add_parser("index", help="build the index of a registry dir")
    ix.add_argument("registry_dir")
    ix.add_argument("--out", default=None)
    rg = sub.add_parser("register", help="register a manifest into a dir")
    rg.add_argument("registry_dir")
    rg.add_argument("manifest_file")
    return p


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.cmd == "manifest":
            data = make_manifest(args.id, args.name, args.version,
                                 args.entry, args.license_id,
                                 ptype=args.ptype,
                                 description=args.description,
                                 depends=args.depends)
            blob = json.dumps(data, indent=2, sort_keys=True)
            if args.out:
                with open(args.out, "w", encoding="utf-8") as fh:
                    fh.write(blob + "\n")
                print(f"wrote {args.out}")
            else:
                print(blob)
        elif args.cmd == "validate-manifest":
            try:
                data = json.loads(_read(args.manifest_file))
            except json.JSONDecodeError as e:
                print(f"error: {args.manifest_file}: bad JSON: {e}",
                      file=sys.stderr)
                return 1
            errors = validate_manifest(data)
            if errors:
                for err in errors:
                    print(f"FAIL {err}")
                return 1
            print(f"valid: {data['id']} {data['version']}")
        elif args.cmd == "index":
            index = build_index(args.registry_dir)
            blob = json.dumps(index, indent=2, sort_keys=True)
            if args.out:
                with open(args.out, "w", encoding="utf-8") as fh:
                    fh.write(blob + "\n")
            print(f"{len(index)} manifest(s) indexed")
            for m in index:
                print(f"  {m['id']} {m['version']} [{m['type']}]")
        elif args.cmd == "register":
            mid, out = register(args.registry_dir, args.manifest_file)
            print(f"registered {mid} -> {out}")
        return 0
    except RegistryError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


def self_test() -> None:
    import tempfile
    good = {"id": "build-cache@sigmaos", "name": "Build cache",
            "version": "1.0.0", "entry": "action.py",
            "license": "GPL-3.0-or-later", "type": "action",
            "description": "Caches build artefacts", "depends": []}
    assert validate_manifest(good) == []

    # every schema rule is exercised
    cases = [
        (dict(good, id="Nope"), "name@scope"),
        (dict(good, id="has space@x"), "name@scope"),
        (dict(good, version="1.0"), "semver"),
        (dict(good, version="v1.0.0"), "semver"),
        (dict(good, type="gadget"), "type must be"),
        (dict(good, license="GPL 3 bad"), "license"),
        (dict(good, entry="/etc/passwd"), "relative path"),
        (dict(good, entry="../../escape.py"), "relative path"),
        (dict(good, entry="notes.txt"), ".py/.sh/.yml"),
        (dict(good, depends="gtk3"), "list of strings"),
        ({k: v for k, v in good.items() if k != "version"},
         "missing required field 'version'"),
        ({k: v for k, v in good.items() if k != "license"},
         "missing required field 'license'"),
        (["not", "a", "dict"], "JSON object"),
        (dict(good, version="1.0.0-rc.1"), None),   # pre-release allowed
    ]
    for data, fragment in cases:
        errors = validate_manifest(data)
        if fragment is None:
            assert errors == [], (data, errors)
        else:
            assert any(fragment in e for e in errors), (fragment, errors)

    # skeleton generation validates too
    skel = make_manifest("widget@sigmaos", "Widget", "0.2.0", "main.py",
                         "MIT", depends=["gtk3"])
    assert skel["type"] == "plugin" and skel["depends"] == ["gtk3"]
    try:
        make_manifest("Bad", "X", "0.1", "main.py", "MIT")
    except RegistryError:
        pass
    else:
        raise AssertionError("bad skeleton accepted")

    with tempfile.TemporaryDirectory() as d:
        reg = os.path.join(d, "registry")
        os.makedirs(reg)

        # validate-manifest on files
        gp = os.path.join(reg, "build-cache" + _MANIFEST_SUFFIX)
        with open(gp, "w") as fh:
            json.dump(good, fh)
        bp = os.path.join(d, "bad.manifest.json")
        with open(bp, "w") as fh:
            json.dump(dict(good, id="other@sigmaos", version="x"), fh)

        # index over a clean registry
        second = dict(good, id="tweak@sigmaos", name="Tweak",
                      entry="tweak.sh", version="0.1.0")
        with open(os.path.join(reg, "tweak" + _MANIFEST_SUFFIX), "w") as fh:
            json.dump(second, fh)
        index = build_index(reg)
        assert [m["id"] for m in index] == \
            ["build-cache@sigmaos", "tweak@sigmaos"]

        # duplicate ids across two files fail
        with open(os.path.join(reg, "dup" + _MANIFEST_SUFFIX), "w") as fh:
            json.dump(good, fh)
        try:
            build_index(reg)
        except RegistryError as e:
            assert "duplicate id" in str(e)
        else:
            raise AssertionError("duplicate id accepted")
        os.unlink(os.path.join(reg, "dup" + _MANIFEST_SUFFIX))

        # invalid manifest inside the registry fails the whole index
        with open(os.path.join(reg, "broken" + _MANIFEST_SUFFIX), "w") as fh:
            json.dump({"id": "broken@x"}, fh)
        try:
            build_index(reg)
        except RegistryError as e:
            assert "missing required field" in str(e)
        else:
            raise AssertionError("broken manifest accepted")
        os.unlink(os.path.join(reg, "broken" + _MANIFEST_SUFFIX))

        # register: append, sort, refuse duplicates, atomic write
        mid, out = register(reg, gp)
        assert mid == "build-cache@sigmaos"
        stored = json.load(open(out))
        assert [m["id"] for m in stored] == ["build-cache@sigmaos"]
        mid, out = register(reg, os.path.join(reg, "tweak" + _MANIFEST_SUFFIX))
        stored = json.load(open(out))
        assert [m["id"] for m in stored] == \
            ["build-cache@sigmaos", "tweak@sigmaos"]
        try:
            register(reg, gp)
        except RegistryError as e:
            assert "already registered" in str(e)
        else:
            raise AssertionError("duplicate registration accepted")
        try:
            register(reg, bp)
        except RegistryError as e:
            assert "invalid manifest" in str(e)
        else:
            raise AssertionError("invalid manifest registered")
        # registering into a fresh directory creates the index there
        reg2 = os.path.join(d, "fresh")
        mid, _ = register(reg2, gp)
        assert mid == "build-cache@sigmaos"
        stored = json.load(open(os.path.join(reg2, "index.json")))
        assert [m["id"] for m in stored] == ["build-cache@sigmaos"]


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
