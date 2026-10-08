set dotenv-load := false
set positional-arguments

default:
    @just --list

frontend *args:
    @ cd frontend && \
      just {{ args }}

frontend-admin *args:
    @ cd frontend-admin && \
      just {{ args }}

backend *args:
    @ cd backend && \
      just {{ args }}

build:
    just frontend build \
      && just frontend-admin build \
      && just backend build \

run *args: build
    @ just backend run {{ args }}
