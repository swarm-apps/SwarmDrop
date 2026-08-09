Pod::Spec.new do |s|
  s.name           = 'AppPaths'
  s.version        = '0.1.0'
  s.summary        = 'iOS Application Support directory resolution for SwarmDrop'
  s.description    = 'Exposes the app-private data directory (Application Support) so the ' \
                     'database and receive staging area can live outside the user-visible Documents folder.'
  s.license        = 'MIT'
  s.author         = 'yexiyue'
  s.homepage       = 'https://github.com/yexiyue/SwarmDrop'
  s.platforms      = { :ios => '17.0' }
  s.swift_version  = '5.9'
  s.source         = { :git => 'https://github.com/yexiyue/SwarmDrop.git' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES'
  }

  s.source_files = "**/*.{h,m,swift}"
end
