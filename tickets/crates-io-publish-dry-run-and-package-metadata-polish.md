---
id: crates-io-publish-dry-run-and-package-metadata-polish
title: crates.io publish dry-run and package metadata polish
status: done
priority: p1
dependencies: []
related: [epic-public-readiness-for-crates-io-github]
scopes: [meta]
shared_scopes: []
paths: []
tags: [public-readiness, release]
---
## Work
- `cargo publish --dry-run` and fix exclude/include issues
- Review Cargo.toml: description, keywords, categories, license, repository, documentation URL
- Confirm license-header.txt / scripts / tickets are not bloating the package (exclude if needed)
- Decide whether Cargo.lock should stay gitignored (library default)

## Done when
Dry-run succeeds; package tarball contents reviewed; notes left on ticket if human crates.io token needed.
