FROM rust:1.84-slim

# Install necessary dependencies
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev

WORKDIR /app

COPY . .

# Build the application
RUN cargo build --release

# Create directories for data persistence
RUN mkdir -p /app/data

CMD ["./target/release/mini-search-engine"]
