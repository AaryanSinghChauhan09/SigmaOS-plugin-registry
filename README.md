# SigmaOS Plugin Registry

Python CLI for generating, validating, indexing, and registering JSON
plugin/action manifests. The manifest schema checks identifiers, semantic
version strings, supported types, license identifier syntax, and entry-path
constraints. Registry indexes are sorted and derived from manifests.

## Data and trust boundaries

`register` appends a validated manifest to the registry's `index.json`; it does
not install, download, sign, execute, or sandbox a plugin. A syntactically valid
license identifier is not proof of license compatibility, and a validated
entry path is not proof that the referenced code is safe. Consumers must verify
the manifest-to-artifact relationship, publisher identity, signatures, and
runtime permissions before loading executable content.

## Development

`make test` runs deterministic manifest and index checks. The project is
inspired by [Omarchy's plugin registry](https://github.com/omacom/omarchy-plugin-registry)
at the manifest/index level; it does not inherit Omarchy's distribution or
trust guarantees.
