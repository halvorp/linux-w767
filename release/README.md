# Release artifacts for `v1.0-iter66-wifi-up`

The binary artifacts (`Image`, `sc8180x-samsung-w767.dtb`, `w767-initramfs.img`,
`ath10k-WCN3990-hw1.0-board-2.bin`, `ath10k_core.ko`) are uploaded to the
GitHub release page rather than stored in git history (binaries don't diff
well and bloat the repo).

To use this release:

1. Open the release on GitHub:
   <https://github.com/halvorp/linux-w767/releases/tag/v1.0-iter66-wifi-up>
2. Download the artifacts under "Assets".
3. Verify with `sha256sum -c SHA256SUMS`.
4. Follow the "Flash and boot" section of `../RELEASE-NOTES.md`.

Reproducible build steps: see "Building from source (kernel)" in RELEASE-NOTES.md.
