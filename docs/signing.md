# Code signing

Releases are not signed yet, so Windows SmartScreen warns on first run ("Windows protected your PC"). Signing removes that warning over time and makes antivirus false positives much less likely. The plan is the free [SignPath Foundation](https://signpath.org) program for open-source projects.

## Applying (the maintainer does this once)

1. Check the project meets SignPath Foundation's conditions:
   - an OSI-approved license (Glide is MIT OR Apache-2.0)
   - a public repository
   - releases built by CI from that repository (`.github/workflows/release.yml`)
   - no malware or potentially unwanted software
2. Apply at <https://signpath.org/apply> with the repository URL. Mention that Glide installs a low-level mouse hook to smooth wheel scrolling, is per-user and never elevated, and has no network access apart from GitHub.
3. After approval, SignPath provides an organization ID, a project slug, a signing policy slug and an API token.

## Wiring it into the release workflow

1. Add the API token as the repository secret `SIGNPATH_API_TOKEN`. Add the organization ID as the repository variable `SIGNPATH_ORGANIZATION_ID`.
2. In `.github/workflows/release.yml`, at the "Code signing goes here" comment:
   - upload `dist/glide.exe` and `dist/glide-settings.exe` as an artifact
   - submit that artifact with `signpath/github-action-submit-signing-request`, waiting for completion
   - download the signed files back into `dist/`
   - then build the installer, and sign `Glide-<version>-setup.exe` the same way
3. `scripts/package.ps1` currently builds and packages in one go. Split it (for example `-SkipBuild` plus a separate installer step) so signing can happen between building the exes and building the installer.

Once releases are signed, submit to winget and Scoop using the manifests each release attaches (`manifests/winget`, `manifests/scoop`).
