# Release notes

Each release has one file here, named after its tag (`v0.4.1.md`). The
release workflow (`.github/workflows/release.yml`) uses it as the release text
and takes the first heading as the release title.

## Cutting a release

1. Add `vX.Y.Z.md` here in a PR. The first line is the title,
   e.g. `## v0.4.1 — Kurzbeschreibung`. Merge it once CI is green.
2. Tag the merged commit on `main` and push the tag:

   ```sh
   git switch main && git pull --ff-only
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

3. The workflow checks that the tag is on `main` and that the notes exist,
   builds and tests in the toolchain image, and publishes a pre-release with
   `Kobo.tgz` and `NickelStorygraph-adds.zip`.

If the workflow fails, fix the cause on `main`, delete the tag
(`git push --delete origin vX.Y.Z && git tag -d vX.Y.Z`) and tag again.
