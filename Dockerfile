FROM rust:1.54.0 as build

# create a new empty shell project
RUN USER=root cargo new --bin youtubeservice
RUN USER=root mv -v youtubeservice/src/main.rs youtubeservice/src/server.rs
WORKDIR /youtubeservice

# copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# this build step will cache your dependencies
RUN cargo build --release
RUN rm src/*.rs

# copy your source tree and additional build files
COPY ./src ./src
COPY ./migrations ./migrations
COPY ./proto ./proto
COPY ./diesel.toml ./diesel.toml
COPY ./build.rs ./build.rs

# build for release
RUN rm ./target/release/deps/youtubeservice*
RUN rustup component add rustfmt
RUN cargo build --release

# our final base
FROM rust:1.54.0-slim-buster

# copy the build artifact from the build stage
COPY --from=build /youtubeservice/target/release/youtubeservice-server .

RUN apt update && apt install -y libpq-dev

# set the startup command to run your binary
CMD ["./youtubeservice-server"]