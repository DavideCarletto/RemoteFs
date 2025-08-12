Ciao Michi

Per far funzionare Il filesystem:

sudo apt-get install fuse3 libfuse3-dev pkg-config
sudo usermod -a -G fuse $USER
<!-- sudo sed -i 's/^#user_allow_other/user_allow_other/' /etc/fuse.conf -->