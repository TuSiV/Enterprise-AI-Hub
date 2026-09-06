# 多阶段构建（§36.2/§36.4）：Rust Core + Web 控制台 + Python Runtime 单镜像（可选部署附件）
FROM node:22-slim AS web-builder
WORKDIR /build/web
COPY web/package.json web/package-lock.json* ./
RUN npm install --registry=https://registry.npmmirror.com
COPY web/ ./
RUN npm run build

FROM rust:1.94-slim AS rust-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock* .cargo/ ./
COPY crates/ crates/
COPY apps/ apps/
COPY runtime/ runtime/
RUN cargo build --release -p aihub-server -p aihub-mock-openai

FROM python:3.12-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-builder /build/target/release/aihub-server /usr/local/bin/aihub-server
COPY --from=rust-builder /build/target/release/aihub-mock-openai /usr/local/bin/aihub-mock-openai
COPY runtime/ ./runtime/
RUN pip install --no-cache-dir -r runtime/requirements.txt pypdf python-docx
COPY --from=web-builder /build/web/dist /app/web/dist
ENV AIHUB_MODE=server \
    AIHUB_DATA_DIR=/data \
# Web dist 默认查找 ../web/dist 与 ./web/dist —— server 模式数据目录之外固定指向 /app/web/dist
VOLUME ["/data"]
EXPOSE 8787
CMD ["aihub-server"]
