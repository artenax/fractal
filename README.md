[![Our chat room](https://img.shields.io/matrix/fractal-gtk:matrix.org?color=blue&label=%23fractal%3Agnome.org&logo=matrix)](https://matrix.to/#/#fractal:gnome.org)
[![Our Gitlab project](https://img.shields.io/badge/gitlab.gnome.org%2F-GNOME%2FFractal-green?logo=gitlab)](https://gitlab.gnome.org/GNOME/fractal/)
[![Our documentation](https://img.shields.io/badge/%F0%9F%95%AE-Docs-B7410E?logo=rust)](https://gnome.pages.gitlab.gnome.org/fractal/)

# Fractal

Fractal is a Matrix messaging app for GNOME written in Rust. Its interface is optimized for
collaboration in large groups, such as free software projects.

![screenshot](https://gitlab.gnome.org/GNOME/fractal/raw/main/screenshots/fractal.png)

## Work in Progress

We already talked several times in the past about rewriting the application, but for different
reasons we didn't do it. Now that the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk)
exists, which does a lot of the heavy lifting for us, we have a good starting point to build Fractal
without the need to implement every single feature from the Matrix API. Finally with the release of
GTK4 we would need to rework most of Fractal's code anyways. Therefore, it just makes sense to start
over and build Fractal with all the features (e.g end-to-end encryption) we have in mind.

A year ago we started working on rewriting [Fractal](https://gitlab.gnome.org/GNOME/fractal/) from
scratch using [GTK4](https://www.gtk.org/) and the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk).
This effort was called Fractal Next.

Fractal Next now replaced our previous codebase, and has become the new nightly version. It isn't
yet ready for a release and you can follow along our progress towards it by looking at the
[Fractal v5 (Fractal-next)](https://gitlab.gnome.org/GNOME/fractal/-/milestones/18) milestone.

## Installation instructions

### Stable version

The current stable version is 4.4.0 (released August 2020).

Flatpak is the recommended installation method.
Until our next iteration is ready, you can get the official Fractal Flatpak on Flathub.

<a href="https://flathub.org/apps/details/org.gnome.Fractal">
<img
    src="https://flathub.org/assets/badges/flathub-badge-i-en.png"
    alt="Download Fractal on Flathub"
    width="240px"
    height="80px"
/>
</a>

### Development version

If you want to try Fractal Next without building it yourself, it is available as a nightly Flatpak
in the gnome-nightly repo.

First, setup the GNOME nightlies.

<a href="https://nightly.gnome.org/gnome-nightly.flatpakrepo ">
<img
    src="https://gitlab.gnome.org/GNOME/fractal/uploads/447997cccc862eb27483b9c61b8a8a12/gnome-nightly.png"
    alt="Add gnome-nightly repository"
    width="240px"
    height="80px"
/>
</a>

Then install the application.

<a href="https://nightly.gnome.org/repo/appstream/org.gnome.Fractal.Devel.flatpakref">
<img
    src="https://gitlab.gnome.org/GNOME/fractal/uploads/a688e9176e8e76d630993869c13a0222/download-fractal-nightly.png"
    alt="Download Fractal Nightly"
    width="240px"
    height="80px"
/>
</a>

Or from the command line:

```sh
# Add the gnome-nightly repo
flatpak remote-add --user --if-not-exists gnome-nightly https://nightly.gnome.org/gnome-nightly.flatpakrepo

# Install the nightly build
flatpak install --user gnome-nightly org.gnome.Fractal.Devel
```

### Runtime Dependencies

Fractal doesn't store your **password** but uses [Secret Service](https://www.freedesktop.org/wiki/Specifications/secret-storage-spec/)
to store your **access token** and **passphrase** used to encrypt the local cache.
Therefore, you need to have software providing that service on your system.
If you're using GNOME this should work for you out of the box and gnome-keyring or ksecretservice
should already be installed and setup.

## Security Best Practices

Additionally to setting up the [Secret Service](https://www.freedesktop.org/wiki/Specifications/secret-storage-spec/),
make sure to use a strong **password** for the keyring, or for the user session if used to unlock the keyring
(normally it's the case), since it will be used to encrypt secrets in **Secret Service**.
Furthermore, make sure to lock your system when stepping away from the computer since an unlocked computer
gives other people access to your private communications and stored secrets.

## Contributing

### Code

Please follow our [contributing guidelines](CONTRIBUTING.md).

### Translations

Fractal is translated by the GNOME translation team on [Damned lies](https://l10n.gnome.org/).

Find your language in the list on [the Fractal module page on Damned lies](https://l10n.gnome.org/module/fractal/).

The names of the emoji displayed during verification come from [the Matrix specification repository](https://github.com/matrix-org/matrix-spec/tree/main/data-definitions).
They are translated on [Element’s translation platform](https://translate.element.io/projects/matrix-doc/sas-emoji-v1).

## Frequently Asked Questions

* Does Fractal have encryption support? Will it ever?

Yes, the current development version (`main` branch) has encryption support using Cross-Signing. See
<https://gitlab.gnome.org/GNOME/fractal/-/issues/717> for more info on the state of encryption.

* Can I run Fractal with the window closed?

Currently Fractal does not support this. Fractal is a GNOME application, and accordingly adheres GNOME
guidelines and paradigms. This will be revisited if or when GNOME gets a "Do Not Disturb" feature.

## The origin of Fractal

The development version is a complete rewrite of Fractal built on top of the
[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) using [GTK4](https://gtk.org/).

The previous version of Fractal was using GTK3 and its own backend to talk to a matrix homeserver,
the code can be found in the [`legacy` branch](https://gitlab.gnome.org/GNOME/fractal/-/tree/legacy).

Initial versions were based on Fest <https://github.com/fest-im/fest>, formerly called ruma-gtk.
In the origins of the project it was called guillotine, based on French revolution, in relation with
the Riot client name, but it's a negative name so we decide to change for a math one.

The name Fractal was proposed by Regina Bíró.

## Code of Conduct

Fractal follows the official GNOME Foundation code of conduct. You can read it [here](/code-of-conduct.md).
