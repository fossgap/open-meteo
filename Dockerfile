FROM rust:1.90-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libgtk-4-dev libadwaita-1-dev \
    libgirepository1.0-dev librsvg2-dev \
    at-spi2-core \
    xvfb xauth dbus-x11 xdotool x11-utils imagemagick fonts-dejavu-core fonts-noto-core fonts-noto-cjk \
    shared-mime-info fontconfig-config dconf-gsettings-backend \
    xfce4-panel xfconf \
    ca-certificates curl file rpm cpio zstd desktop-file-utils shellcheck \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy

ENV CARGO_BUILD_JOBS=2 LC_ALL=C.UTF-8
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY vendor ./vendor
COPY tools ./tools
COPY LICENSE ./
COPY packaging/copyright ./packaging/copyright
COPY src ./src
COPY resources ./resources
COPY tests/fixtures ./tests/fixtures
RUN cargo build --locked --workspace --features open-meteo-checks/desktop \
    && cargo test --locked --workspace --features open-meteo-checks/desktop \
    && cargo clippy --locked --workspace --all-targets --features open-meteo-checks/desktop -- -D warnings
COPY . .
CMD ["sh", "scripts/ci-inside.sh", "check"]
