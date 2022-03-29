[![Docs Badge](https://img.shields.io/badge/%F0%9F%95%AE-Docs-blue)](https://gnome.pages.gitlab.gnome.org/fractal/fractal/)

# Fractal

Fractal is a Matrix messaging app for GNOME written in Rust. Its interface is optimized for collaboration in large groups, such as free software projects. The current development focus is on Fractal Next, a rewrite based on modern technologies.

* Come talk to us on Matrix: <https://matrix.to/#/#fractal:gnome.org>
* Main repository: <https://gitlab.gnome.org/GNOME/fractal/>

![screenshot](https://gitlab.gnome.org/GNOME/fractal/raw/fractal-next/screenshots/fractal.png)


## Work in Progress

We are working on rewriting [Fractal](https://gitlab.gnome.org/GNOME/fractal/) from scratch using [GTK4](https://www.gtk.org/) and the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk). This effort is called fractal-next.

We already talked several times in the past about rewriting the application, but for different reasons we didn't do it. Now that the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) exists, which does a lot of the heavy lifting for us, we have a good starting point to build Fractal without the need to implement every single feature from the Matrix API. Finally with the release of GTK4 we would need to rework most of Fractal's code anyways. Therefore, it just makes sense to start over and build Fractal with all the features (e.g end-to-end encryption) we have in mind.

The main development branch is [fractal-next](https://gitlab.gnome.org/GNOME/fractal/-/tree/fractal-next). Issues that target fractal-next should be labelled accordingly as "Fractal-next".
Our current work focuses on getting the same level of features as we already have in the stable version. Then fractal-next will replace our current codebase, merging it into the main git branch and becoming the new nightly version. You can follow along our progress towards that goal by looking at the [feature parity milestone](https://gitlab.gnome.org/GNOME/fractal/-/milestones/18).

## Installation instructions

Flatpak is the recommended installation method. Until Fractal Next is ready, you can get the official
Fractal Flatpak on Flathub.

<a href="https://flathub.org/apps/details/org.gnome.Fractal">
<img src="https://flathub.org/assets/badges/flathub-badge-i-en.png" width="190px" />
</a>

Fractal can also be installed as a snap on any distro with snap support enabled

<a href="https://snapcraft.io/fractal">
<img src="https://github.com/snapcore/snap-store-badges/raw/master/EN/[EN]-snap-store-white.png" width="182px" />
</a>

## Build Instructions

### Flatpak

Flatpak is the recommended way of building and installing Fractal.

First you need to make sure you have the GNOME SDK and Rust toolchain installed.

```
# Add Flathub and the gnome-nightly repo
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists gnome-nightly https://nightly.gnome.org/gnome-nightly.flatpakrepo

# Install the gnome-nightly Sdk and Platform runtime
flatpak install --user gnome-nightly org.gnome.Sdk org.gnome.Platform

# Install the required rust-stable extension from Flathub
flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//21.08

# Install the required llvm extension from Flathub
flatpak install --user flathub org.freedesktop.Sdk.Extension.llvm12//21.08
```
Move inside the `build-aux` folder and then build and install the app:

```
cd build-aux
flatpak-builder --user --install app org.gnome.Fractal.Devel.json
```

Fractal Next can then be entirely removed from your system with:

```
flatpak remove org.gnome.Fractal.Devel.json`
```

### GNU/Linux

If you decide to ignore our recommendation and build on your host system,
outside of Flatpak or snap, you will need Meson and Ninja (as well as Rust and Cargo).

```sh
meson . _build --prefix=/usr/local
ninja -C _build
sudo ninja -C _build install
```

### Translations

Fractal is translated by the GNOME translation team on
[Damned lies](https://l10n.gnome.org/).

If you want to add *a new language* you should update the file
`fractal-gtk/po/LINGUAS` and add the code for that language
to the list.

Get the pot file from [the Fractal module page on Damned lies](https://l10n.gnome.org/module/fractal/).

### Password Storage

Fractal uses [Secret Service](https://www.freedesktop.org/wiki/Specifications/secret-storage-spec/)
to store the password so you should have something providing
that service on your system. If you're using GNOME or KDE
this should work for you out of the box with gnome-keyring or
ksecretservice.

## Frequently Asked Questions

* Does Fractal have encryption support? Will it ever?

Yes, Fractal-next has encryption support using Cross-Signing.
See <https://gitlab.gnome.org/GNOME/fractal/-/issues/717> for more info on the state of encryption.

* Can I run Fractal with the window closed?

Currently Fractal does not support this. Fractal is a
GNOME application, and accordingly adheres GNOME
guidelines and paradigms. This will be revisited if or
when GNOME gets a "Do Not Disturb" feature.

## The origin of Fractal

Fractal-next is a complete rewrite of Fractal built on top of the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) using [GTK4](https://gtk.org/).

The previous version of Fractal was using GTK3 and its own backend to talk to a matrix homeserver.
Initial versions were based on Fest <https://github.com/fest-im/fest>, formerly called ruma-gtk.
In the origins of the project it was called guillotine, based on French revolution,
in relation with the Riot client name, but it's a negative name so we decide
to change for a math one.

The name Fractal was proposed by Regina Bíró.

## Code of Conduct

Fractal follows the official GNOME Foundation code of conduct. You can read it [here](/code-of-conduct.md).
