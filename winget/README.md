# winget

Manifests for [winget-pkgs](https://github.com/microsoft/winget-pkgs), package `TheCosmicCrafter.CosmicFontFoundry`.

## First version (manual, once)

After the release is **published** on GitHub:

```powershell
./winget/prepare.ps1 0.3.0
winget validate --manifest winget/manifests/t/TheCosmicCrafter/CosmicFontFoundry/0.3.0
winget install --manifest winget/manifests/t/TheCosmicCrafter/CosmicFontFoundry/0.3.0
```

Submit via [wingetcreate](https://github.com/microsoft/winget-create) (`wingetcreate submit ...`) or a PR to `microsoft/winget-pkgs`. `winget/manifests/` is generated locally and gitignored.

## Future versions (automatic)

Once present in `winget-pkgs`, `.github/workflows/winget.yml` submits new releases via `komac` if the `WINGET_TOKEN` secret (`public_repo` scope) is configured.
