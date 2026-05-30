# We need our own vagrant-vbguest Installer class to support
# the arm64 version of VirtualBox Guest Additions.
class DebianARM64 < VagrantVbguest::Installers::Debian
  def installer
    @installer ||= File.join(mount_point, 'VBoxLinuxAdditions-arm64.run')
  end

  def yes
    '' # Never use a `yes` during install.
  end
end
