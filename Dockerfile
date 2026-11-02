FROM rust:1.90-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libgtk-4-dev libadwaita-1-dev \
    libgirepository1.0-dev librsvg2-dev \
    python3 python3-gi gir1.2-atspi-2.0 at-spi2-core \
    xvfb xauth dbus-x11 xdotool x11-utils imagemagick fonts-dejavu-core fonts-noto-core fonts-noto-cjk \
    shared-mime-info fontconfig-config dconf-gsettings-backend \
    xfce4-panel xfconf \
    ca-certificates curl file rpm cpio zstd desktop-file-utils shellcheck \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy

ENV CARGO_BUILD_JOBS=2 LC_ALL=C.UTF-8
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY resources ./resources
COPY tests/fixtures ./tests/fixtures
RUN cargo build --locked && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings
COPY . .
CMD ["sh", "scripts/ci-inside.sh", "check"]
