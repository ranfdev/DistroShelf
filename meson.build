project('distroshelf', 'rust',
          version: '1.0.5',
    meson_version: '>= 1.0.0',
  default_options: [ 'warning_level=2', 'werror=false', ],
)

i18n = import('i18n')
gnome = import('gnome')

version = meson.project_version()

meson.add_dist_script(
  'build-aux/dist-vendor.sh',
  join_paths(meson.project_build_root(), 'meson-dist', meson.project_name() + '-' + version),
  meson.project_source_root()
)

subdir('data')
subdir('src')
subdir('po')

gnome.post_install(
     glib_compile_schemas: true,
    gtk_update_icon_cache: true,
  update_desktop_database: true,
)
