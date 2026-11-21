# AUR package: `muninn-bin`

Binary package recipe for the Linux **x86_64** v0.9.1 release of
[Muninn](https://github.com/raythurman2386/muninn).

- `arch=('x86_64')` only. Arch Linux ARM / aarch64 is **unofficial** and is not
  packaged here.
- The prebuilt binary is installed as-is (`options=('!strip')`).
- Installing the package does not write to `$HOME` and does not run any
  systemd or desktop integration. An existing vault (`~/.config/muninn`) is
  never touched — removing the package only deletes files under `/usr`.

## Files

- `PKGBUILD` — the recipe.
- `.SRCINFO` — generated with `makepkg --printsrcinfo`. Regenerate and commit
  it whenever the `PKGBUILD` changes.

The `sha256sums` value is taken verbatim from the
`muninn_v0.9.1_linux_x86_64.tar.gz` line of the v0.9.1 release `checksums.txt`:

<https://github.com/raythurman2386/muninn/releases/download/v0.9.1/checksums.txt>

## Test in a clean Arch container

Requires a working Docker daemon. The commands use `sudo docker`; drop `sudo`
if your user is in the `docker` group. A native Arch host with `makepkg` also
works for the validation steps.

```sh
# Start a clean Arch container and install the build tools.
sudo docker run -dit --name muninn-aur-test archlinux:latest bash
sudo docker exec muninn-aur-test bash -lc 'pacman -Syu --noconfirm base-devel sudo'
sudo docker exec muninn-aur-test bash -lc \
  'useradd -m -s /bin/bash builder && echo "builder ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/builder && chmod 440 /etc/sudoers.d/builder'

# Copy the recipe in and hand it to the unprivileged builder.
sudo docker cp . muninn-aur-test:/home/builder/aur
sudo docker exec muninn-aur-test chown -R builder:builder /home/builder/aur

# Validate the recipe and fetch/verify the release tarball.
sudo docker exec -u builder muninn-aur-test bash -lc \
  'cd /home/builder/aur && bash -n PKGBUILD && makepkg --printsrcinfo && makepkg --verifysource'

# Build and install the package (deps are pulled in by pacman).
sudo docker exec -u builder muninn-aur-test bash -lc \
  'cd /home/builder/aur && makepkg -si --noconfirm'

# The installed binary must report the packaged version.
sudo docker exec muninn-aur-test bash -lc 'muninn --version'   # => muninn 0.9.1

# Removal must leave a user config/vault untouched.
sudo docker exec muninn-aur-test bash -lc 'pacman -R --noconfirm muninn-bin'

# Clean up.
sudo docker rm -f muninn-aur-test
```

## Publish to the AUR (manual — account owner only)

Publishing is deliberately **not** automated; it is the account owner's step.

1. Clone the AUR repo for `muninn-bin` (`https://aur.archlinux.org/muninn-bin.git`).
2. Copy in `PKGBUILD` and `.SRCINFO`, then commit.
3. Push to the AUR remote (`git push aur master`).
4. For each new release, bump `pkgver`/`pkgrel` and refresh `sha256sums` from
   that release's `checksums.txt`, then regenerate `.SRCINFO` with
   `makepkg --printsrcinfo`.
