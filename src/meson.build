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

if get_option('profile') == 'default'
  cargo_options += [ '--release' ]
  rust_target = 'release'
  message('Building in release mode')
else
  rust_target = 'debug'
  message('Building in debug mode')
endif

# Flatpak extracts cargo-sources.json into cargo/ and writes cargo/config.
# That build must stay offline. A local .cargo-home cache is the same idea
# outside Flatpak. Neither path is allowed to refresh the crates.io index.
fs = import('fs')
vendor_cargo_home = meson.project_source_root() / 'cargo'
source_cargo_home = meson.project_source_root() / '.cargo-home'
if fs.exists(vendor_cargo_home / 'config')
  cargo_env = [ 'CARGO_HOME=' + vendor_cargo_home ]
  cargo_options += [ '--offline', '--locked' ]
elif fs.exists(source_cargo_home / 'registry')
  cargo_env = [ 'CARGO_HOME=' + source_cargo_home ]
  cargo_options += [ '--offline', '--locked' ]
else
  cargo_env = [ 'CARGO_HOME=' + meson.project_build_root() / 'cargo-home' ]
endif

custom_target(
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
