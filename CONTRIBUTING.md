# Contributing

## Newcomers

[Fractal](https://gitlab.gnome.org/GNOME/fractal/) follows the [GNOME Newcomers workflow](https://wiki.gnome.org/Newcomers/).
Follow these pages to learn how to contribute.

Here are also a few links to help you get started with Rust and the GTK Rust bindings:

- [Learn Rust](https://www.rust-lang.org/learn)
- [GUI development with Rust and GTK 4](https://gtk-rs.org/gtk4-rs/stable/latest/book)
- [gtk-rs website](https://gtk-rs.org/)

[The Rust docs of our application](https://gnome.pages.gitlab.gnome.org/fractal/) might also be
useful.

Don't hesitate to join [our Matrix room](https://matrix.to/#/#fractal:gnome.org) to come talk to us
and ask us any questions you might have.

## Build Instructions

### Prerequisites

Fractal is written in Rust, so you will need to have at least Rust 1.63 and Cargo available on your
system. You will also need to install the Rust nightly toolchain to be able to run our
[pre-commit hook](#pre-commit).

If you're building Fractal with Flatpak (via GNOME Builder or the command line), you will need to
manually add the necessary remotes and install the required FreeDesktop extensions:

```sh
# Add Flathub and the gnome-nightly repo
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists gnome-nightly https://nightly.gnome.org/gnome-nightly.flatpakrepo

# Install the gnome-nightly Sdk and Platform runtime
flatpak install --user gnome-nightly org.gnome.Sdk//master org.gnome.Platform//master

# Install the required rust-stable extension from Flathub
flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//22.08

# Install the required llvm extension from Flathub
flatpak install --user flathub org.freedesktop.Sdk.Extension.llvm14//22.08
```

### GNOME Builder

Using [GNOME Builder](https://wiki.gnome.org/Apps/Builder) with [flatpak](https://flatpak.org/) is
the recommended way of building and installing Fractal.

By default, GNOME Builder should select the `org.gnome.Fractal.Devel.json` manifest, which is the
manifest used for building the nightly version. It is recommended to switch to the
`org.gnome.Fractal.Hack.json` manifest which will build much faster.

### Flatpak via fenv

As an alternative, [fenv](https://gitlab.gnome.org/ZanderBrown/fenv) allows to setup a flatpak
environment from the command line and execute commands in that environment.

First, install fenv:

```sh
# Clone the project somewhere on your system
git clone https://gitlab.gnome.org/ZanderBrown/fenv.git

# Move into the folder
cd fenv

# Install fenv with Cargo
cargo install --path .
```

You can now discard the `fenv` directory if you want.

After that, move into the directory where you cloned Fractal and setup the project:

```sh
# Setup the flatpak environment
fenv gen build-aux/org.gnome.Fractal.Hack.json

# Initialize the build system
fenv exec -- meson --prefix=/app _build
```

Finally, build and run the application:

```sh
# Build the project
fenv exec -- ninja -C _build

# Install the application in the flatpak environment
fenv exec -- ninja -C _build install

# Launch Fractal
fenv exec ./_build/src/fractal
```

To test changes you make to the code, re-run these three last commands.

### Install the flatpak

Some features that interact with the system require the app to be installed to test them (i.e.
notifications, command line arguments, etc.).

Move inside the `build-aux` folder and then build and install the app:

```sh
cd build-aux
flatpak-builder --user --install app org.gnome.Fractal.Hack.json
```

It can then be entirely removed from your system with:

```sh
flatpak remove --delete-data org.gnome.Fractal.Hack
```

### GNU/Linux

If you decide to ignore our recommendation and build on your host system, outside of Flatpak, you
will need Meson and Ninja.

```sh
meson . _build --prefix=/usr/local
ninja -C _build
sudo ninja -C _build install
```

## Pre-commit

We expect all code contributions to be correctly formatted. To help with that, a pre-commit hook
should get installed as part of the building process. It runs the `scripts/checks.sh` script. It's a
quick script that makes sure that the code is correctly formatted with `rustfmt`, among other
things. Make sure that this script is effectively run before submitting your merge request,
otherwise CI will probably fail right away.

You should also run `cargo clippy` as that will catch common errors and improve the quality of your
submissions and is once again checked by our CI.

## Commit

Please follow the [GNOME commit message guidelines](https://wiki.gnome.org/Git/CommitMessages).

## Merge Request

Before submitting a merge request, make sure that [your fork is available publicly](https://gitlab.gnome.org/help/user/public_access.md),
otherwise CI won't be able to run.

Use the title of your commit as the title of your MR if there's only one. Otherwise it should
summarize all your commits. If your commits do several tasks that can be separated, open several
merge requests.

In the details, write a more detailed description of what it does. If your changes include a change
in the UI or the UX, provide screenshots in both light and dark mode, and/or a screencast of the
new behavior.

Don't forget to mention the issue that this merge request solves or is related to, if applicable.
GitLab recognizes the syntax `Closes #XXXX` or `Fixes #XXXX` that will close the corresponding
issue accordingly when your change is merged.

We expect to always work with a clean commit history. When you apply fixes or suggestions,
[amend](https://git-scm.com/docs/git-commit#Documentation/git-commit.txt---amend) or
[fixup](https://git-scm.com/docs/git-commit#Documentation/git-commit.txt---fixupamendrewordltcommitgt)
and [squash](https://git-scm.com/docs/git-rebase#Documentation/git-rebase.txt---autosquash) your
previous commits that you can then [force push](https://git-scm.com/docs/git-push#Documentation/git-push.txt--f).
