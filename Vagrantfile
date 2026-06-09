# -*- mode: ruby -*-
# vi: set ft=ruby :

# We need the `vagrant-vbguest` plugin which hasn't been updated to
# work with more modern Ruby versions. See:
#
#     https://github.com/hashicorp/vagrant/issues/13404
#
# To fix the issue for our own installation, we use a monkeypatch to
# Extend the Ruby File class to restore the deprecated exists method
# calls `File.exist` instead.
unless File.respond_to?(:exists?)
  class << File
    def exists?(path)
      warn "File.exists? is deprecated; use File.exist? instead." unless ENV['SUPPRESS_FILE_EXISTS_WARNING']
      exist?(path)
    end
  end
end
# End of the monkeypatch.

# We're going to need this plugin, so install it.
unless Vagrant.has_plugin?("vagrant-vbguest")
  system("vagrant plugin install vagrant-vbguest")
  puts "Re-run your 'vagrant' command again to continue."
  exit(0)
end

# Vagrant configuration really starts here.
Vagrant.configure("2") do |config|
  config.vagrant.plugins = ["vagrant-vbguest"]
  config.vbguest.installer_hooks[:before_install] = [
    # Clear the way for the plugin to find the correct
    # Guest Additions iso file and use it, instead.
    "eject /dev/sr0",

    # Install additional dependencies for installation.
    "apt-get update",
    "apt-get install bzip2"
  ]
  require_relative "provision/vbguest/DebianARM64"
  config.vbguest.installer = DebianARM64

  config.vm.box = "cloud-image/debian-13"
  config.vm.disk :dvd, name: "vboxguest-installer", file: "provision/empty.iso"
  config.vm.synced_folder "tests/data/radicale/", "/var/lib/radicale", id: "radicale_home"
  config.vm.network "forwarded_port", guest: 443, host: 8443
  config.vm.provision "freedombox", type: "shell" do |p|
    p.inline = <<~EOF
      export DEBIAN_FRONTEND=noninteractive
      apt-get update && apt-get upgrade --assume-yes
      apt-get install --assume-yes freedombox

      useradd --home-dir /var/lib/radicale --create-home --user-group \
        --shell /usr/sbin/nologin --comment "Radicale CalDAV Server" \
        radicale

      echo
      echo To complete the installation of FreedomBox,
      echo access the FreedomBox Web interface and supply
      echo the installer with the following secret code:
      echo
      echo     $(cat /var/lib/plinth/firstboot-wizard-secret)
      echo
      echo Then, consider setting up an administrative user
      echo with credentials such as admin:freedombox.
    EOF
  end
  config.vm.provision "rm-radicale-cache", type: "shell" do |p|
    p.inline = <<~EOF
      echo -n Cleaning up Radicale cache and lock files...
      rm -rf /var/lib/radicale/collections/.Radicale.lock
      rm -rf /var/lib/radicale/collections/collection-root/admin/test-contacts/.Radicale.cache
      echo done.
    EOF
  end
  config.vm.provision "mountvboxsf", type: "shell", run: "always" do |p|
    p.inline = <<~EOF
      umount radicale_home
      mount -t vboxsf -o uid=$(id -u radicale),gid=$(id -g radicale) radicale_home /var/lib/radicale
    EOF
  end
  config.vm.provision "sync-radicale-native", type: "shell", run: "always" do |p|
    p.inline = <<~EOF
      # Radicale fsync fails on VirtualBox shared folders; serve from native ext4.
      NATIVE=/var/lib/radicale-native/collections
      mkdir -p "$NATIVE"
      cp -a /var/lib/radicale/collections/. "$NATIVE/"
      chown -R radicale:radicale /var/lib/radicale-native
      rm -rf "$NATIVE/collection-root/admin/test-contacts/.Radicale.cache"
      rm -rf /var/lib/radicale/collections/.Radicale.lock
      if ! grep -q 'filesystem_folder = /var/lib/radicale-native/collections' /etc/radicale/config; then
        sed -i 's|^#filesystem_folder = .*|filesystem_folder = /var/lib/radicale-native/collections|' /etc/radicale/config
      fi
      systemctl restart uwsgi
      ln -sf /run/uwsgi/app/radicale/socket /run/uwsgi/radicale.socket
    EOF
  end
  config.vm.provision "restart-radicale", type: "shell", run: "never" do |p|
    p.inline = <<~EOF
      echo -n Syncing test contacts to native storage and restarting CardDAV...
      NATIVE=/var/lib/radicale-native/collections
      mkdir -p "$NATIVE"
      cp -a /var/lib/radicale/collections/. "$NATIVE/"
      chown -R radicale:radicale /var/lib/radicale-native
      rm -rf /var/lib/radicale/collections/.Radicale.lock
      rm -rf "$NATIVE/collection-root/admin/test-contacts/.Radicale.cache"
      systemctl restart uwsgi
      ln -sf /run/uwsgi/app/radicale/socket /run/uwsgi/radicale.socket
      echo done.
    EOF
  end
  config.vm.post_up_message = <<~EOF
    Access the FreedomBox Web interface at:
       https://localhost:8443
  EOF
end
