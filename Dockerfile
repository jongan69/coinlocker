# Stage 1: Use cargo-chef to manage dependency caching
FROM lukemathwalker/cargo-chef:latest AS chef
WORKDIR /app

# Stage 2: Planner - Create the build plan
FROM chef AS planner
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder - Build the dependencies and the application
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Only re-copy source files if they change
COPY . .
RUN cargo build --release
RUN mv ./target/release/coinlockerapi ./app

# Stage 4: Runtime - Use a minimal base image
FROM debian:stable-slim AS runtime
WORKDIR /app

# Install necessary libraries
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/app /usr/local/bin/coinlockerapi
ENTRYPOINT ["/usr/local/bin/coinlockerapi"]
EXPOSE 8080
