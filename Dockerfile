# Use a Rust base image
FROM rust:latest as builder
RUN apt-get update && apt-get install -y libclang-dev 

# Create a new directory for your app
WORKDIR /app

# Copy the Cargo.toml and Cargo.lock files to the container
COPY Cargo.toml Cargo.lock build.rs ./

# Copy the source code to the container
COPY src ./src
COPY protos ./protos

# Build the dependencies (cached)
RUN cargo build

# Build your application
#RUN cargo build --release --locked

# Create a new stage for the runtime image
FROM debian:bullseye-slim

# Install any necessary system dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage to the runtime image
COPY --from=builder /app/target/debug/shinkai_node /usr/local/bin/shinkai_node

# Set the command to run your application when the container starts
ENTRYPOINT ["/usr/local/bin/shinkai_node"]

