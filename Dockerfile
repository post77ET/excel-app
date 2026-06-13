FROM rust:1.76 as builder
WORKDIR /app
COPY . .
WORKDIR /app/rust_core
RUN cargo build --release --bin core1_etb

FROM python:3.11-slim
WORKDIR /app
COPY --from=builder /app /app
RUN pip install -r /app/web_app/requirements.txt
WORKDIR /app/web_app
ENV ETB_BIN_PATH=/app/rust_core/target/release/core1_etb
CMD ["gunicorn", "app:app", "--bind", "0.0.0.0:10000", "--timeout", "600"]
