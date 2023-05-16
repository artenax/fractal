[![Our chat room](https://img.shields.io/matrix/fractal-gtk:matrix.org?color=blue&label=%23fractal%3Agnome.org&logo=matrix)](https://matrix.to/#/#fractal:gnome.org)
[![Our Gitlab project](https://img.shields.io/badge/gitlab.gnome.org%2F-GNOME%2FFractal-green?logo=gitlab)](https://gitlab.gnome.org/GNOME/fractal/)
[![Our documentation](https://img.shields.io/badge/%F0%9F%95%AE-Docs-B7410E?logo=rust)](https://gnome.pages.gitlab.gnome.org/fractal/)

# Fractal

Fractal is a Matrix messaging app for GNOME written in Rust. Its interface is optimized for
collaboration in large groups, such as free software projects.

![screenshot](https://gitlab.gnome.org/GNOME/fractal/raw/main/screenshots/fractal.png)

## Work in Progress

Up until the 4.4 series, Fractal was built with GTK 3 and used its own implementation of the Matrix
API. With the availability of the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk)
and [GTK 4](https://www.gtk.org/), the technological landscape had changed enough that a full
rewrite made more sense than simply trying to port the complex codebase that we had. That effort
started in March 2021 and the result is the current [Fractal 5 beta](#beta-version) (and
[nightly](#development-version)).

You can follow along our progress towards the stable release by looking at the
[Fractal 5](https://gitlab.gnome.org/GNOME/fractal/-/milestones/18) milestone.

## Installation instructions

### Stable version

The current stable version is 4.4.2 (released March 2023).

Flatpak is the recommended installation method.
Until our next iteration is ready, you can get the official Fractal Flatpak from Flathub.

<a href="https://flathub.org/apps/details/org.gnome.Fractal">
<img
    src="https://flathub.org/assets/badges/flathub-badge-i-en.svg"
    alt="Download Fractal on Flathub"
    width="240px"
    height="80px"
/>
</a>

### Beta version

The current beta version is 5.beta1 (released May 16th 2023).

It is available as a Flatpak on Flathub Beta.

To get it, first set up the Flatpak remote:

```sh
flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo
```

Or download and install the [repository file](https://flathub.org/beta-repo/flathub-beta.flatpakrepo).

Then install the application:

```sh
flatpak install --user flathub-beta org.gnome.Fractal
```

Or download and install the [application file](https://flathub.org/beta-repo/appstream/org.gnome.Fractal.flatpakref).

Finally, run the application:

```sh
flatpak run org.gnome.Fractal//beta
```

If you want to run the beta version by default instead of the stable version, use this command:

```sh
flatpak make-current org.gnome.Fractal beta
```

_Note that you can go back to using the stable version by default by using the same command and
replacing `beta` with `stable`._

### Development version

If you want to try the upcoming version of Fractal without building it yourself, it is available as a nightly Flatpak
in the gnome-nightly repo.

First, set up the GNOME nightlies.

<a href="https://nightly.gnome.org/gnome-nightly.flatpakrepo ">
<img
    src="https://gitlab.gnome.org/GNOME/fractal/uploads/c276f92660dcf50067714ac08e193fea/gnome-nightly-badge.svg"
    alt="Add gnome-nightly repository"
    width="240px"
    height="80px"
/>
</a>

Then install the application.

<a href="https://nightly.gnome.org/repo/appstream/org.gnome.Fractal.Devel.flatpakref">
<img
    src="https://gitlab.gnome.org/GNOME/fractal/uploads/5e42d322eaacc7da2a52bfda9f7a4e53/fractal-nightly-badge.svg"
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

Fractal doesn’t store your **password**, but it stores your **access token** and the **passphrase**
used to encrypt the database and the local cache.

The stable Flatpak available on Flathub and any version that is not sandboxed rely on software that
implements the [Secret Service API](https://www.freedesktop.org/wiki/Specifications/secret-storage-spec/)
to store those secrets. Therefore, you need to have software providing that service on your system,
like gnome-keyring, KeepassXC ([setup guide](https://avaldes.co/2020/01/28/secret-service-keepassxc.html)),
or a recent version of KWallet. If you are using GNOME this should just work.

With the nightly and beta Flatpaks, Fractal uses the [Secret portal](https://docs.flatpak.org/en/latest/portal-api-reference.html#gdbus-org.freedesktop.portal.Secret)
to store those secrets. Once again, if you are using GNOME this should just work. If you are using a
different desktop environment or are facing issues, make sure `xdg-desktop-portal` is installed
along with a service that provides the [Secret portal backend interface](https://docs.flatpak.org/en/latest/portal-api-reference.html#gdbus-org.freedesktop.impl.portal.Secret),
which is currently only implemented by gnome-keyring.

If you prefer to use other software that only implements the Secret Service API while using the
nightly or beta Flatpak, you need to make sure that no service implementing the Secret portal
backend interface is running, and you need to allow Fractal to access the D-Bus service with this
command:

```sh
flatpak override --user --talk-name=org.freedesktop.secrets org.gnome.Fractal.Devel
```

Or with [Flatseal](https://flathub.org/apps/details/com.github.tchx84.Flatseal), by adding
`org.freedesktop.secrets` in the **Session Bus** > **Talk** list of Fractal.

## Security Best Practices

You should use a strong **password** that is hard to guess to protect the secrets stored on your
device, whether the password is used directly to unlock your secrets (with a password manager for
example) or if it is used to open your user session and your secrets are unlocked automatically
(which is normally the case with a GNOME session).

Furthermore, make sure to lock your system when stepping away from the computer since an unlocked
computer can allow other people to access your private communications and your secrets.

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

Currently Fractal does not support this. Fractal is a GNOME application, and accordingly adheres to
the GNOME guidelines and paradigms. This will be revisited if or when GNOME gets a proper paradigm
to interact with apps running in the background.

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
