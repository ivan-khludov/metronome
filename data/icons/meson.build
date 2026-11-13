install_data(
  '@0@.svg'.format(application_id),
  install_dir: iconsdir / 'hicolor' / 'scalable' / 'apps'
)

foreach size : ['48', '64', '128', '256']
  install_data(
    'metronome-@0@.png'.format(size),
    rename: '@0@.png'.format(application_id),
    install_dir: iconsdir / 'hicolor' / '@0@x@0@'.format(size) / 'apps'
  )
endforeach

install_data(
  '@0@-symbolic.svg'.format(base_id),
  install_dir: iconsdir / 'hicolor' / 'symbolic' / 'apps',
  rename: '@0@-symbolic.svg'.format(application_id)
)
