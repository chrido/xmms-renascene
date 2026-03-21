project('xmms', 'c',
  version: '2.0.0',
  license: 'GPL-2.0-or-later',
  meson_version: '>= 0.59',
  default_options: [
    'c_std=gnu11',
    'warning_level=1',
  ],
)

gnome = import('gnome')
gtk4 = dependency('gtk4', version: '>= 4.6')
gstreamer = dependency('gstreamer-1.0', version: '>= 1.16')
libarchive = dependency('libarchive', version: '>= 3.0', required: false)

conf_data = configuration_data()
conf_data.set_quoted('PACKAGE', meson.project_name())
conf_data.set_quoted('VERSION', meson.project_version())
conf_data.set_quoted('XMMS_DATADIR', get_option('prefix') / get_option('datadir') / 'xmms')
conf_data.set_quoted('XMMS_SKINDIR', get_option('prefix') / get_option('datadir') / 'xmms' / 'Skins')
if libarchive.found()
  conf_data.set('HAVE_LIBARCHIVE', 1)
endif
configure_file(output: 'config.h', configuration: conf_data)

resources = gnome.compile_resources('xmms-resources',
  'data/xmms.gresource.xml',
  source_dir: 'data',
  c_name: 'xmms',
)

sources = files(
  'src/main.c',
  'src/skin.c',
  'src/widget.c',
  'src/player.c',
  'src/playlist.c',
  'src/util.c',
  'src/playlistwin.c',
  'src/equalizerwin.c',
  'src/mpris.c',
  'src/skinwin.c',
)

executable('xmms',
  sources, resources,
  dependencies: [gtk4, gstreamer, libarchive],
  include_directories: [
    include_directories('src'),
    include_directories('.'),     # for config.h
  ],
  install: true,
)

install_data('data/org.xmms.XMMS.desktop',
  install_dir: get_option('datadir') / 'applications',
)

install_data('data/org.xmms.XMMS.metainfo.xml',
  install_dir: get_option('datadir') / 'metainfo',
)
