# UI resources
ui_resources = gnome.compile_resources(
  'ui-resources',
  'ui-resources.gresource.xml',
  gresource_bundle: true,
  install: true,
  install_dir: pkgdatadir,
)

version = major_version
if pre_release_version != ''
  version += '.' + pre_release_version
endif
if devel_version != ''
  version += '-' + devel_version
endif

global_conf = configuration_data()
global_conf.set_quoted('APP_ID', application_id)
global_conf.set_quoted('PKGDATADIR', pkgdatadir)
global_conf.set('PROFILE', profile)
global_conf.set_quoted('VERSION', version)
global_conf.set_quoted('GETTEXT_PACKAGE', gettext_package)
global_conf.set_quoted('LOCALEDIR', localedir)
config = configure_file(
  input: 'config.rs.in',
  output: 'config.rs',
  configuration: global_conf
)
# Copy the config.rs output to the source directory.
run_command(
  'cp',
  meson.project_build_root() / 'src' / 'config.rs',
  meson.project_source_root() / 'src' / 'config.rs',
  check: true
)

cargo_options = [ '--manifest-path', meson.project_source_root() / 'Cargo.toml' ]
cargo_options += [ '--target-dir', meson.project_build_root() / 'src' ]

if profile == 'Hack'
  rust_target = 'debug'
  message('Building in debug mode')
else
  cargo_options += [ '--release' ]
  rust_target = 'release'
  message('Building in release mode')
endif

cargo_env = [ 'CARGO_HOME=' + meson.project_build_root() / 'cargo-home' ]

custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: bindir,
  depends: [resources, ui_resources],
  command: [
    'env',
    cargo_env,
    cargo, 'build',
    cargo_options,
    '&&',
    'cp', 'src' / rust_target / meson.project_name(), '@OUTPUT@',
  ]
)

rustdoc_flags = ' '.join([
  '-Zunstable-options',
  '--enable-index-page',
  '--extern-html-root-url=gio=https://gtk-rs.org/gtk-rs-core/stable/latest/docs/',
  '--extern-html-root-url=glib=https://gtk-rs.org/gtk-rs-core/stable/latest/docs/',
  '--extern-html-root-url=gsk4=https://gtk-rs.org/gtk4-rs/stable/latest/docs/',
  '--extern-html-root-url=gdk4=https://gtk-rs.org/gtk4-rs/stable/latest/docs/',
  '--extern-html-root-url=gtk4=https://gtk-rs.org/gtk4-rs/stable/latest/docs/',
  '--extern-html-root-url=libadwaita=https://world.pages.gitlab.gnome.org/Rust/libadwaita-rs/stable/latest/docs/',
  '--cfg=docsrs',
])
doc_env = ['RUSTDOCFLAGS=' + rustdoc_flags ]
doc_deps = [
  '--package=ruma',
  '--package=ruma-common',
  '--package=ruma-client-api',
  '--package=matrix-sdk',
  '--package=matrix-sdk-base',
  '--package=matrix-sdk-common',
  '--package=matrix-sdk-crypto',
  '--package=matrix-sdk-qrcode',
  '--package=matrix-sdk-sqlite',
  '--package=matrix-sdk-store-encryption',
  '--package=matrix-sdk-ui',
  '--package=fractal',
]

custom_target(
    'cargo-doc',
    build_by_default: false,
    build_always_stale: true,
    output: 'doc',
    console: true,
    command: [
      'env', cargo_env + doc_env,
      cargo, 'doc',
      cargo_options + doc_deps + ['--no-deps', '-Zrustdoc-map'] ,
    ],
  )
