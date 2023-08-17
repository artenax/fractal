project('fractal',
        'rust',
        version: '5.beta2',
        license: 'GPL-3.0-or-later',
        meson_version: '>= 0.59')

i18n = import('i18n')
gnome = import('gnome')

base_id = 'org.gnome.Fractal'

major_version = '5'
pre_release_version = 'beta2'

dependency('glib-2.0', version: '>= 2.72') # update when changing gtk version
dependency('gio-2.0', version: '>= 2.72') # always same version as glib
dependency('gtk4', version: '>= 4.10.0')
dependency(
  'libadwaita-1', version: '>= 1.3.0',
  fallback: ['libadwaita', 'libadwaita_dep'],
  default_options: ['tests=false', 'examples=false', 'vapi=false']
)
dependency(
  'gtksourceview-5', version: '>= 5.0.0',
  fallback: ['gtksourceview', 'gtksource_dep'],
  default_options: ['gtk_doc=false', 'sysprof=false', 'gir=false', 'vapi=false', 'install_tests=false']
)
dependency('gstreamer-1.0', version: '>= 1.20')
dependency('gstreamer-base-1.0', version: '>= 1.20')
dependency('gstreamer-pbutils-1.0', version: '>= 1.20')
dependency('gstreamer-play-1.0', version: '>= 1.20')
dependency('gstreamer-video-1.0', version: '>= 1.20')
dependency('openssl', version: '>= 1.0.1')
dependency('sqlite3', version: '>= 3.24.0')
dependency('libpipewire-0.3', version: '>= 0.3.0')
dependency('shumate-1.0', version: '>= 1.0.0')
dependency('xdg-desktop-portal', version: '>= 1.14.1')

glib_compile_resources = find_program('glib-compile-resources', required: true)
glib_compile_schemas = find_program('glib-compile-schemas', required: true)
desktop_file_validate = find_program('desktop-file-validate', required: false)
appstream_util = find_program('appstream-util', required: false)
cargo = find_program('cargo', required: true)

prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
localedir = prefix / get_option('localedir')

datadir = prefix / get_option('datadir')
pkgdatadir = datadir / meson.project_name()
iconsdir = datadir / 'icons'
podir = meson.project_source_root() / 'po'
gettext_package = meson.project_name()

if get_option('profile') == 'hack'
  is_release = false
  profile = 'Hack'
elif get_option('profile') == 'development'
  is_release = false
  profile = 'Devel'
elif get_option('profile') == 'beta'
  is_release = true
  profile = 'Beta'
else
  is_release = true
  profile = 'Stable'
endif

if is_release
  devel_version = ''
  release_date = '' # should be unused with released versions
  application_id = base_id
else
  vcs_tag = run_command('git', 'rev-parse', '--short', 'HEAD').stdout().strip()
  if vcs_tag == ''
    devel_version = profile.to_lower()
  else
    devel_version = vcs_tag
  endif
  release_date = run_command('git', 'show', '-s', '--format=%cI').stdout().strip()
  application_id = '@0@.@1@'.format(base_id, profile)
endif

meson.add_dist_script(
  'build-aux/dist-vendor.sh',
  meson.project_build_root() / 'meson-dist' / meson.project_name() + '-' + major_version + '.' + pre_release_version,
  meson.project_source_root()
)

if profile == 'Hack'
  # Setup pre-commit hook for ensuring coding style is always consistent
  message('Setting up git pre-commit hook..')
  run_command('cp', '-f', 'hooks/pre-commit.hook', '.git/hooks/pre-commit')
endif

subdir('data')
subdir('po')
subdir('src')

gnome.post_install(
  gtk_update_icon_cache: true,
  glib_compile_schemas: true,
  update_desktop_database: true,
)
