    # SigmaOS-plugin-registry

    Plugin registry & manifest index

    Part of the **SigmaOS** sovereign operating-system ecosystem — a multi-repository
    distribution modelled on Linux Mint and Omarchy Linux. See the parent
    [`SigmaOS`](https://github.com/AaryanSinghChauhan09/SigmaOS) repository for the
    full component index.

    ## Upstream inspirations
    - https://github.com/omacom/omarchy-plugin-registry

    ## Design principles applied
    - **Single Source of Truth**
- **Fail-Fast**
- **Defensive Programming**
- **Plan Before Act**
- **KISS**
- **DRY**
- **Deterministic Testing**

    ## Usage
    Verbs: `register, manifest, index, validate-manifest`

    ```
    $ sigma-registry manifest build-cache@sigmaos "Build cache" \
    --version 1.0.0 --entry action.py --license GPL-3.0-or-later --out build-cache.manifest.json
$ sigma-registry validate-manifest build-cache.manifest.json
valid: build-cache@sigmaos 1.0.0
$ sigma-registry index ./registry
2 manifest(s) indexed
$ sigma-registry register ./registry build-cache.manifest.json
registered build-cache@sigmaos -> ./registry/index.json

    ```

    ## Build & test
    ```
    make build
    make test
    ```

    ## Licence
    GNU General Public License v3.0 (FOSS).
