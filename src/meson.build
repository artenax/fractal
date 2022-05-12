global_conf = configuration_data()
global_conf.set_quoted('APP_ID', application_id)
global_conf.set_quoted('PKGDATADIR', pkgdatadir)
global_conf.set_quoted('PROFILE', profile)
global_conf.set_quoted('VERSION', version + version_suffix)
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

if get_option('profile') == 'default' or get_option('profile') == 'development'
  cargo_options += [ '--release' ]
  rust_target = 'release'
  message('Building in release mode')
else
  rust_target = 'debug'
  message('Building in debug mode')
endif

cargo_env = [ 'CARGO_HOME=' + meson.project_build_root() / 'cargo-home' ]

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: bindir,
  depends: resources,
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
  '--package=matrix-qrcode',
  '--package=matrix-sdk-sled',
  '--package=matrix-sdk-store-encryption',
  '--package=fractal',
]

cargo_doc = custom_target(
    'cargo-doc',
    build_by_default: false,
    build_always_stale: true,
    output: 'doc',
    console: true,
    command: [
      'env', cargo_env + doc_env,
      cargo, '+nightly', 'doc',
      cargo_options + doc_deps + ['--no-deps', '--document-private-items', '-Zrustdoc-map'] ,
    ],
  )
