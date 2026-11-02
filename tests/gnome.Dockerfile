FROM registry.fedoraproject.org/fedora:44
RUN dnf -y --setopt=install_weak_deps=False install \
    gnome-shell python3 dbus-daemon mesa-dri-drivers mesa-libEGL \
    xorg-x11-server-Xwayland util-linux fuse3 \
    && dnf clean all \
    && useradd -m shelltest
WORKDIR /workspace
COPY --chown=shelltest:shelltest . .
RUN mkdir -p target/ci && chown -R shelltest:shelltest target
USER shelltest
CMD ["sh", "scripts/check-gnome.sh"]
